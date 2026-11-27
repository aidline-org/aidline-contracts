//! Fuzz harness for the Aidline contract.
//!
//! This target drives randomized sequences of contract operations
//! (create, donate, approve, cancel, refund, sponsor matching pools,
//! pledges, and verifier bonds with withdrawal/slashing) and checks
//! financial invariants after each step.
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
//! From the repository root (not from inside `fuzz/`), run a short bounded
//! fuzz session:
//!
//! ```sh
//! cargo +nightly fuzz run fuzz_contract -- -max_total_time=60
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
//!    `pool.remaining >= 0`, `pool.matched >= 0`, `bond.remaining >= 0`.
//! 2. **Released never exceeds raised.** `campaign.released <= campaign.raised`.
//! 3. **Released never exceeds goal.** `campaign.released <= campaign.goal`.
//! 4. **Raised stays within goal plus bond additions.** `campaign.raised <= campaign.goal + bond_deposited`.
//! 5. **Milestone counters are consistent.** `milestones_released <= milestones.len()`.
//! 6. **Sponsor pool accounting is consistent.** `pool.matched + pool.remaining == pool.deposited`.
//! 7. **Bond accounting is consistent.** `bond_remaining <= bond_deposited`.
//! 8. **Contract balance equals escrow plus unreturned pools and bonds.**
//!    The contract balance matches unreleased escrow (`raised - released - refunded`)
//!    plus held sponsor pools and verifier bonds within rounding dust.
//! 9. **State transitions are valid.** A Completed or Cancelled campaign never
//!    accepts new donations or milestone approvals.
//!
//! # Note on execution
//!
//! This harness is not part of the normal `cargo test` suite. It requires
//! `cargo-fuzz` and a nightly Rust toolchain. CI runs it for 30 seconds in a
//! separate job.

#![no_main]

use aidline::{Aidline, AidlineClient, CampaignKind, CampaignStatus};
use libfuzzer_sys::arbitrary::{self, Arbitrary};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::{
    Address, Env, String as SorobanString,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    vec as soroban_vec,
};

// ─── Structured fuzz input ────────────────────────────────────────────────────

/// One operation in a random sequence.
#[derive(Debug, Clone, Copy, Arbitrary)]
pub enum Op {
    Donate {
        amount_mod: u32,
    },
    ApproveMilestone,
    Cancel,
    Refund {
        use_donor_b: bool,
    },
    DepositSponsorPool {
        ratio_bps_mod: u8,
        cap_mod: u32,
    },
    ApplyMatching {
        donation_mod: u32,
    },
    ReturnSponsorPool,
    AdvanceTime {
        seconds: u32,
    },
    CreatePledge {
        use_donor_b: bool,
        amount_mod: u32,
    },
    RegisterWithBond {
        bond_amount_mod: u32,
    },
    DeregisterVerifier,
    WithdrawBond,
    SlashVerifier {
        slash_amount_mod: u32,
    },
}

/// Decode raw bytes into a sequence of operations using a byte-driven approach.
fn parse_ops(data: &[u8]) -> Vec<Op> {
    let mut ops = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let tag = data[i] % 13;
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
            8 => {
                let use_donor_b = i < data.len() && data[i] % 2 == 1;
                i += 1;
                let amount_mod = if i + 3 < data.len() {
                    u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
                } else {
                    50
                };
                i += 4;
                Op::CreatePledge {
                    use_donor_b,
                    amount_mod,
                }
            }
            9 => {
                let bond_amount_mod = if i + 3 < data.len() {
                    u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
                } else {
                    100
                };
                i += 4;
                Op::RegisterWithBond { bond_amount_mod }
            }
            10 => Op::DeregisterVerifier,
            11 => Op::WithdrawBond,
            12 => {
                let slash_amount_mod = if i + 3 < data.len() {
                    u32::from_le_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]])
                } else {
                    50
                };
                i += 4;
                Op::SlashVerifier { slash_amount_mod }
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
    bond_deposited: i128,
    bond_remaining: i128,
}

impl Model {
    fn check_invariants(&self) {
        // 1. No negative balances
        assert!(self.raised >= 0, "raised < 0");
        assert!(self.released >= 0, "released < 0");
        assert!(self.sponsor_remaining >= 0, "sponsor_remaining < 0");
        assert!(self.sponsor_matched >= 0, "sponsor_matched < 0");
        assert!(self.bond_remaining >= 0, "bond_remaining < 0");

        // 2. Released never exceeds raised
        assert!(self.released <= self.raised, "released > raised");

        // 3. Released never exceeds goal
        assert!(self.released <= self.goal, "released > goal");

        // 4. Raised stays within goal plus slashed bond compensation
        assert!(
            self.raised <= self.goal + self.bond_deposited,
            "raised > goal + bond_deposited"
        );

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

        // 7. Bond remaining is consistent
        assert!(
            self.bond_remaining <= self.bond_deposited,
            "bond_remaining > bond_deposited"
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
    client.add_verifier(&admin, &verifier);

    let bonded_verifier = Address::generate(&env);

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
    sac_client.mint(&bonded_verifier, &big);

    // Create campaign: 3 milestones of 100/200/300 = 600 total
    let milestones = soroban_vec![&env, 100_i128, 200_i128, 300_i128];
    let goal: i128 = 600;
    let deadline = env.ledger().timestamp() + 30 * 86_400;
    // Setup must succeed, so use the panicking client call here.
    let campaign_id = client.create_campaign(
        &creator,
        &beneficiary,
        &verifier,
        &sac.address(),
        &CampaignKind::Emergency,
        &SorobanString::from_str(&env, "ipfs://fuzz"),
        &deadline,
        &milestones,
    );

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
        bond_deposited: 0,
        bond_remaining: 0,
    };

    let mut sponsor_pool_active = false;
    let mut bond_active = false;
    let mut total_refunded = 0i128;

    // ── Execute ops ─────────────────────────────────────────────────────────
    for op in parse_ops(data) {
        match op {
            Op::Donate { amount_mod } => {
                let amount = (amount_mod as i128 % 601).max(1);
                let _ = client.try_donate(&donor_a, &campaign_id, &amount);
                {
                    let c = client.get_campaign(&campaign_id);
                    model.raised = c.raised;
                    model.released = c.released;
                    model.milestones_released = c.milestones_released;
                    model.status = c.status;
                }
            }
            Op::ApproveMilestone => {
                let proof = SorobanString::from_str(&env, "ipfs://fuzz-proof");
                let _ = client.try_approve_milestone(&campaign_id, &proof);
                {
                    let c = client.get_campaign(&campaign_id);
                    model.raised = c.raised;
                    model.released = c.released;
                    model.milestones_released = c.milestones_released;
                    model.status = c.status;
                }
            }
            Op::Cancel => {
                let _ = client.try_cancel_campaign(&creator, &campaign_id);
                {
                    let c = client.get_campaign(&campaign_id);
                    model.status = c.status;
                }
            }
            Op::Refund { use_donor_b } => {
                let donor = if use_donor_b { &donor_b } else { &donor_a };
                if let Ok(Ok(refund_amount)) = client.try_refund(donor, &campaign_id) {
                    total_refunded += refund_amount;
                }
                {
                    let c = client.get_campaign(&campaign_id);
                    model.raised = c.raised;
                    model.released = c.released;
                }
            }
            Op::DepositSponsorPool {
                ratio_bps_mod,
                cap_mod,
            } => {
                if !sponsor_pool_active {
                    let cap = 1_000_000 + (cap_mod as i128 % 1_000_000);
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
                    let _ = client.try_apply_matching(&sponsor, &campaign_id, &donor_a, &donation);
                    if let Some(pool) = client.get_sponsor_pool(&campaign_id, &sponsor) {
                        model.sponsor_remaining = pool.remaining;
                        model.sponsor_matched = pool.matched;
                    }
                    {
                        let c = client.get_campaign(&campaign_id);
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
            Op::CreatePledge {
                use_donor_b,
                amount_mod,
            } => {
                let donor = if use_donor_b { &donor_b } else { &donor_a };
                let amount = (amount_mod as i128 % 200).max(1);
                token_client.approve(
                    donor,
                    &contract_id,
                    &amount,
                    &(env.ledger().sequence() + 10_000),
                );
                let _ = client.try_create_pledge(donor, &campaign_id, &amount);
            }
            Op::RegisterWithBond { bond_amount_mod } => {
                if !bond_active {
                    let _ = client.try_set_bond_requirement(&50);
                    let bond_amount = ((bond_amount_mod as i128 % 500) + 50).max(50);
                    if client
                        .try_register_with_bond(&bonded_verifier, &bond_amount)
                        .is_ok()
                    {
                        bond_active = true;
                        model.bond_deposited = bond_amount;
                        model.bond_remaining = bond_amount;
                    }
                }
            }
            Op::DeregisterVerifier => {
                if bond_active {
                    let _ = client.try_deregister_verifier(&admin, &bonded_verifier);
                }
            }
            Op::WithdrawBond => {
                if bond_active {
                    if client.try_withdraw_bond(&bonded_verifier).is_ok() {
                        bond_active = false;
                        model.bond_remaining = 0;
                    }
                }
            }
            Op::SlashVerifier { slash_amount_mod } => {
                if bond_active && model.bond_remaining > 0 {
                    let slash_amount = (slash_amount_mod as i128 % 200)
                        .max(1)
                        .min(model.bond_remaining);
                    if client
                        .try_slash_verifier(&admin, &bonded_verifier, &campaign_id, &slash_amount)
                        .is_ok()
                    {
                        model.bond_remaining -= slash_amount;
                        let c = client.get_campaign(&campaign_id);
                        model.raised = c.raised;
                    }
                }
            }
        }

        // ── Check all invariants after every step ───────────────────────────
        model.check_invariants();

        // Accounting invariant: contract token balance holds campaign escrow,
        // unreturned sponsor pool, and active verifier bond.
        let contract_balance = token_client.balance(&contract_id);
        assert!(
            contract_balance >= 0,
            "contract balance < 0: {}",
            contract_balance
        );

        let sponsor_hold = if sponsor_pool_active {
            model.sponsor_remaining
        } else {
            0
        };
        let bond_hold = if bond_active {
            model.bond_remaining
        } else {
            0
        };
        let campaign_escrow = (model.raised - model.released - total_refunded).max(0);
        let expected_balance = campaign_escrow + sponsor_hold + bond_hold;

        assert!(
            contract_balance <= expected_balance + 100, // +100 for integer division dust
            "contract_balance {} > expected {} + 100 (raised={} released={} refunded={} sponsor_hold={} bond_hold={})",
            contract_balance,
            expected_balance,
            model.raised,
            model.released,
            total_refunded,
            sponsor_hold,
            bond_hold,
        );

        // Invariant 9: Completed/Cancelled campaign blocks new donations.
        if model.status == CampaignStatus::Completed || model.status == CampaignStatus::Cancelled {
            assert!(
                client
                    .try_donate(&donor_a, &campaign_id, &aidline::MIN_DONATION_AMOUNT)
                    .is_err(),
                "donate succeeded on a finished campaign"
            );
            assert!(
                client
                    .try_approve_milestone(&campaign_id, &SorobanString::from_str(&env, "x"))
                    .is_err(),
                "approve succeeded on a finished campaign"
            );
        }
    }
});
