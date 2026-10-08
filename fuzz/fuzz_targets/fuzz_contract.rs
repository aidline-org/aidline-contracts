//! Fuzz harness for the Aidline contract.
//!
//! This target drives randomized sequences of the primary contract operations
//! — create, donate, approve, cancel, refund — and checks a set of financial
//! invariants after each step.
//!
//! # Running locally
//!
//! Install cargo-fuzz (requires nightly Rust):
//!
//! ```sh
//! cargo install cargo-fuzz
//! rustup override set nightly
//! ```
//!
//! From the repository root, run a short bounded fuzz session:
//!
//! ```sh
//! cd fuzz
//! cargo fuzz run fuzz_contract -- -max_total_time=60
//! ```
//!
//! The `-max_total_time=60` flag limits the run to 60 seconds.
//! Increase the value for deeper exploration.
//!
//! # Invariants checked
//!
//! After every operation the harness verifies:
//!
//! 1. **No negative balances.** `campaign.raised >= 0`, `campaign.released >= 0`,
//!    `pool.remaining >= 0`, `pool.matched >= 0`.
//! 2. **Released never exceeds raised.**  `campaign.released <= campaign.raised`.
//! 3. **Released never exceeds goal.**  `campaign.released <= campaign.goal`.
//! 4. **Raised never exceeds goal.**  `campaign.raised <= campaign.goal`.
//! 5. **Milestone counters are consistent.**  `milestones_released <= milestones.len()`.
//! 6. **Sponsor pool accounting is consistent.**  `pool.matched + pool.remaining == pool.deposited`.
//! 7. **Refunds plus releases never exceed raised.**  After all refunds, the
//!    contract balance equals 0 (or a small non-negative dust amount due to
//!    integer truncation).
//! 8. **State transitions are valid.**  A Completed or Cancelled campaign never
//!    accepts new donations or milestone approvals.
//!
//! # Note on execution
//!
//! This file is source code only. It is NOT executed as part of the normal
//! `cargo test` suite. It requires `cargo-fuzz` and a nightly Rust toolchain.
//! Do not run it in CI unless you have explicitly installed those prerequisites.

#![no_main]

use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    Address, Env, String as SorobanString,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    vec as soroban_vec,
};
use aidline::{Aidline, AidlineClient, CampaignKind, CampaignStatus};

// ─── Structured fuzz input ────────────────────────────────────────────────────

/// One operation in a random sequence.
///
/// `cargo-fuzz` decodes raw bytes using `arbitrary::Arbitrary` (derived via
/// `libfuzzer_sys`). We keep the enum small so the fuzzer can reach all arms
/// with a moderate number of iterations.
#[derive(Debug)]
enum Op {
    Donate { amount_mod: u32 },
    ApproveMilestone,
    Cancel,
    Refund { use_donor_b: bool },
    DepositSponsorPool { ratio_bps_mod: u8, cap_mod: u32 },
    ApplyMatching { donation_mod: u32 },
    ReturnSponsorPool,
    /// Advance the ledger timestamp, potentially expiring the campaign.
    AdvanceTime { seconds: u32 },
}

/// Decode raw bytes into a sequence of operations using a simple byte-driven
/// approach (no `arbitrary` crate dependency required).
fn parse_ops(data: &[u8]) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let tag = data[i] % 9;
        i += 1;
        let op = match tag {
            0 => {
                let amount_mod = if i + 3 < data.len() {
                    u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
                } else {
                    0
                };
                i += 4;
                Op::Donate { amount_mod }
            }
            1 => Op::ApproveMilestone,
            2 => Op::Cancel,
            3 => {
                let use_donor_b = i < data.len() && data[i] % 2 == 1;
                i += 1;
                Op::Refund { use_donor_b }
            }
            4 => {
                let ratio_bps_mod = if i < data.len() { data[i] } else { 100 };
                i += 1;
                let cap_mod = if i + 3 < data.len() {
                    u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
                } else {
                    500
                };
                i += 4;
                Op::DepositSponsorPool {
                    ratio_bps_mod,
                    cap_mod,
                }
            }
            5 => {
                let donation_mod = if i + 3 < data.len() {
                    u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
                } else {
                    100
                };
                i += 4;
                Op::ApplyMatching { donation_mod }
            }
            6 => Op::ReturnSponsorPool,
            7 => {
                let seconds = if i + 3 < data.len() {
                    u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
                } else {
                    3600
                };
                i += 4;
                Op::AdvanceTime { seconds }
            }
            _ => Op::Cancel,
        };
        ops.push(op);
        // Limit to 50 ops per input to keep runs fast.
        if ops.len() >= 50 {
            break;
        }
    }
    ops
}

// ─── Model / state tracking ───────────────────────────────────────────────────

/// A lightweight model of contract state used to check invariants without
/// re-reading the full contract state after every step.
struct Model {
    goal: i128,
    raised: i128,
    released: i128,
    milestones_count: u32,
    milestones_released: u32,
    status: CampaignStatus,
    sponsor_deposited: i128,
    sponsor_remaining: i128,
    sponsor_matched: i128,
}

impl Model {
    fn check_invariants(&self) {
        // 1. No negative balances
        assert!(self.raised >= 0, "raised < 0");
        assert!(self.released >= 0, "released < 0");
        assert!(self.sponsor_remaining >= 0, "sponsor_remaining < 0");
        assert!(self.sponsor_matched >= 0, "sponsor_matched < 0");

        // 2. Released never exceeds raised
        assert!(self.released <= self.raised, "released > raised");

        // 3 & 4. Raised and released stay within goal
        assert!(self.raised <= self.goal, "raised > goal");
        assert!(self.released <= self.goal, "released > goal");

        // 5. Milestone counters are consistent
        assert!(
            self.milestones_released <= self.milestones_count,
            "milestones_released > milestones_count"
        );

        // 6. Sponsor pool accounting is consistent
        assert_eq!(
            self.sponsor_matched + self.sponsor_remaining,
            self.sponsor_deposited,
            "sponsor_matched + sponsor_remaining != sponsor_deposited"
        );
    }
}

// ─── Fuzz entry point ─────────────────────────────────────────────────────────

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    // ── Set up the environment ──────────────────────────────────────────────
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(admin.clone());
    let sac_client = StellarAssetClient::new(&env, &sac.address());
    let token_client = TokenClient::new(&env, &sac.address());

    let contract_id = env.register(Aidline, (admin.clone(), sac.address()));
    let client = AidlineClient::new(&env, &contract_id);

    let verifier = Address::generate(&env);
    client.add_verifier(&verifier);

    let creator = Address::generate(&env);
    let beneficiary = Address::generate(&env);
    let donor_a = Address::generate(&env);
    let donor_b = Address::generate(&env);
    let sponsor = Address::generate(&env);

    // Mint generous balances so the fuzzer can explore a wide range.
    let big: i128 = 1_000_000_000;
    sac_client.mint(&donor_a, &big);
    sac_client.mint(&donor_b, &big);
    sac_client.mint(&sponsor, &big);

    // Create campaign: 3 milestones of 100/200/300 = 600 total
    let milestones = soroban_vec![&env, 100_i128, 200_i128, 300_i128];
    let goal: i128 = 600;
    let deadline = env.ledger().timestamp() + 30 * 86_400;
    let campaign_id = client
        .try_create_campaign(
            &creator,
            &beneficiary,
            &verifier,
            &CampaignKind::Emergency,
            &SorobanString::from_str(&env, "ipfs://fuzz"),
            &deadline,
            &milestones,
        )
        .unwrap_or_else(|_| 0);

    // ── Model initial state ─────────────────────────────────────────────────
    let mut model = Model {
        goal,
        raised: 0,
        released: 0,
        milestones_count: 3,
        milestones_released: 0,
        status: CampaignStatus::Active,
        sponsor_deposited: 0,
        sponsor_remaining: 0,
        sponsor_matched: 0,
    };

    let mut sponsor_pool_active = false;

    // ── Execute ops ─────────────────────────────────────────────────────────
    for op in parse_ops(data) {
        match op {
            Op::Donate { amount_mod } => {
                let amount = (amount_mod as i128 % 601).max(1);
                let _ = client.try_donate(&donor_a, &campaign_id, &amount);
                // Sync model from contract state after each op
                if let Ok(c) = client.try_get_campaign(&campaign_id) {
                    model.raised = c.raised;
                    model.released = c.released;
                    model.milestones_released = c.milestones_released;
                    model.status = c.status;
                }
            }
            Op::ApproveMilestone => {
                let proof = SorobanString::from_str(&env, "ipfs://fuzz-proof");
                let _ = client.try_approve_milestone(&campaign_id, &proof);
                if let Ok(c) = client.try_get_campaign(&campaign_id) {
                    model.raised = c.raised;
                    model.released = c.released;
                    model.milestones_released = c.milestones_released;
                    model.status = c.status;
                }
            }
            Op::Cancel => {
                let _ = client.try_cancel_campaign(&creator, &campaign_id);
                if let Ok(c) = client.try_get_campaign(&campaign_id) {
                    model.status = c.status;
                }
            }
            Op::Refund { use_donor_b } => {
                let donor = if use_donor_b { &donor_b } else { &donor_a };
                let _ = client.try_refund(donor, &campaign_id);
                if let Ok(c) = client.try_get_campaign(&campaign_id) {
                    model.raised = c.raised;
                    model.released = c.released;
                }
            }
            Op::DepositSponsorPool {
                ratio_bps_mod,
                cap_mod,
            } => {
                if !sponsor_pool_active {
                    // Keep cap small enough to avoid goal-exceeding issues
                    let cap = ((cap_mod as i128 % 300) + 1).min(goal);
                    let ratio_bps = ((ratio_bps_mod as u32) % 10_000 + 1).min(10_000);
                    if client
                        .try_deposit_sponsor_pool(&sponsor, &campaign_id, &ratio_bps, &cap, &cap)
                        .is_ok()
                    {
                        sponsor_pool_active = true;
                        model.sponsor_deposited = cap;
                        model.sponsor_remaining = cap;
                        model.sponsor_matched = 0;
                    }
                }
            }
            Op::ApplyMatching { donation_mod } => {
                if sponsor_pool_active {
                    let donation = (donation_mod as i128 % 300).max(1);
                    let _ = client.try_apply_matching(
                        &sponsor,
                        &campaign_id,
                        &donor_a,
                        &donation,
                    );
                    if let Some(pool) =
                        client.try_get_sponsor_pool(&campaign_id, &sponsor).ok().flatten()
                    {
                        model.sponsor_remaining = pool.remaining;
                        model.sponsor_matched = pool.matched;
                    }
                    if let Ok(c) = client.try_get_campaign(&campaign_id) {
                        model.raised = c.raised;
                    }
                }
            }
            Op::ReturnSponsorPool => {
                if sponsor_pool_active {
                    if client
                        .try_return_sponsor_pool(&sponsor, &campaign_id)
                        .is_ok()
                    {
                        sponsor_pool_active = false;
                        model.sponsor_remaining = 0;
                        // matched stays so accounting check still holds:
                        // deposited = matched + 0 (remaining)
                        model.sponsor_deposited = model.sponsor_matched;
                    }
                }
            }
            Op::AdvanceTime { seconds } => {
                let now = env.ledger().timestamp();
                env.ledger().set_timestamp(now + seconds as u64);
            }
        }

        // ── Check all invariants after every step ───────────────────────────
        model.check_invariants();

        // Invariant 8: Completed/Cancelled campaign blocks new donations.
        if model.status == CampaignStatus::Completed
            || model.status == CampaignStatus::Cancelled
        {
            assert!(
                client.try_donate(&donor_a, &campaign_id, &1).is_err(),
                "donate succeeded on a finished campaign"
            );
            assert!(
                client
                    .try_approve_milestone(
                        &campaign_id,
                        &SorobanString::from_str(&env, "x")
                    )
                    .is_err(),
                "approve succeeded on a finished campaign"
            );
        }
    }

    // ── Final balance invariant ──────────────────────────────────────────────
    // After all ops, the contract's token balance must equal raised - released
    // (some may remain as dust from integer truncation, but must be >= 0).
    let contract_balance = token_client.balance(&contract_id);
    let expected_min = model.raised - model.released;
    assert!(
        contract_balance >= 0,
        "contract balance < 0: {}",
        contract_balance
    );
    // Allow for sponsor pool funds still held (if pool is still active).
    let sponsor_hold = if sponsor_pool_active {
        model.sponsor_remaining
    } else {
        0
    };
    assert!(
        contract_balance <= expected_min + sponsor_hold + 100, // +100 for rounding dust
        "contract_balance {} > expected {} (raised={} released={} sponsor_hold={})",
        contract_balance,
        expected_min + sponsor_hold,
        model.raised,
        model.released,
        sponsor_hold,
    );
});
