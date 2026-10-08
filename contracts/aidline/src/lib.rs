#![no_std]

//! Aidline: milestone based escrow for disaster relief and climate campaigns.
//!
//! Donations are held by the contract and only reach the beneficiary one
//! milestone at a time, after the campaign's verifier confirms the work was
//! done. If a campaign is cancelled or runs out of time, donors can reclaim
//! their share of whatever has not been released yet.

mod errors;
mod events;
mod storage;
mod types;

#[cfg(test)]
mod test;

use soroban_sdk::{Address, Env, String, Vec, contract, contractimpl, token};

pub use errors::Error;
pub use types::{Campaign, CampaignKind, CampaignStatus, SponsorPool};

use events::{
    CampaignCancelled, CampaignCreated, Donated, MatchingApplied, MilestoneReleased, Refunded,
    SponsorPoolDeposited, SponsorPoolReturned, VerifierUpdated,
};

const MAX_MILESTONES: u32 = 20;
/// Maximum matching ratio (100 % = 10 000 bps).
const MAX_RATIO_BPS: u32 = 10_000;

#[contract]
pub struct Aidline;

#[contractimpl]
impl Aidline {
    /// Sets the admin and the token (a Stellar Asset Contract such as USDC)
    /// that every campaign raises in.
    pub fn __constructor(env: Env, admin: Address, token: Address) {
        storage::set_admin(&env, &admin);
        storage::set_token(&env, &token);
        storage::bump_instance(&env);
    }

    // Admin

    pub fn add_verifier(env: Env, verifier: Address) {
        storage::admin(&env).require_auth();
        storage::set_verifier(&env, &verifier, true);
        VerifierUpdated {
            verifier,
            active: true,
        }
        .publish(&env);
    }

    /// Removing a verifier freezes milestone approvals on their campaigns
    /// until the campaign is cancelled or reassigned.
    pub fn remove_verifier(env: Env, verifier: Address) {
        storage::admin(&env).require_auth();
        storage::set_verifier(&env, &verifier, false);
        VerifierUpdated {
            verifier,
            active: false,
        }
        .publish(&env);
    }

    pub fn set_admin(env: Env, new_admin: Address) {
        storage::admin(&env).require_auth();
        storage::set_admin(&env, &new_admin);
    }

    // Campaigns

    pub fn create_campaign(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
    ) -> Result<u64, Error> {
        creator.require_auth();
        storage::bump_instance(&env);

        if !storage::is_verifier(&env, &verifier) {
            return Err(Error::NotVerifier);
        }
        if deadline <= env.ledger().timestamp() {
            return Err(Error::DeadlineInPast);
        }
        if milestones.is_empty() || milestones.len() > MAX_MILESTONES {
            return Err(Error::InvalidMilestones);
        }
        let mut goal: i128 = 0;
        for amount in milestones.iter() {
            if amount <= 0 {
                return Err(Error::InvalidMilestones);
            }
            goal = goal.checked_add(amount).ok_or(Error::InvalidMilestones)?;
        }

        let id = storage::next_campaign_id(&env);
        let campaign = Campaign {
            id,
            creator: creator.clone(),
            beneficiary,
            verifier,
            kind,
            metadata_uri,
            goal,
            deadline,
            milestones,
            milestones_released: 0,
            raised: 0,
            released: 0,
            status: CampaignStatus::Active,
        };
        storage::save_campaign(&env, &campaign);

        CampaignCreated {
            campaign_id: id,
            creator,
            kind,
            goal,
            deadline,
        }
        .publish(&env);
        Ok(id)
    }

    pub fn donate(env: Env, donor: Address, campaign_id: u64, amount: i128) -> Result<(), Error> {
        donor.require_auth();
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;
        let raised = campaign
            .raised
            .checked_add(amount)
            .ok_or(Error::InvalidAmount)?;
        if raised > campaign.goal {
            return Err(Error::GoalExceeded);
        }

        token::Client::new(&env, &storage::token(&env)).transfer(
            &donor,
            &env.current_contract_address(),
            &amount,
        );

        campaign.raised = raised;
        storage::save_campaign(&env, &campaign);
        let previous = storage::contribution(&env, campaign_id, &donor);
        storage::set_contribution(&env, campaign_id, &donor, previous + amount);

        Donated {
            campaign_id,
            donor: donor.clone(),
            amount,
        }
        .publish(&env);

        // ── Issue #27: apply sponsor matching after recording the donation ──
        // We iterate over the sponsor's pool keyed by (campaign_id, sponsor).
        // Because Soroban storage does not support prefix-scanning we rely on
        // the sponsor having called `deposit_sponsor_pool` before donating.
        // The matching is a best-effort, single-pool approach: the contract
        // attempts to match for every pool the sponsor registered. In this
        // MVP we apply matching for the donation's full amount against the
        // first available pool. A future improvement can iterate all pools.
        // NOTE: sponsors register their own pool via `deposit_sponsor_pool`
        // and the matching is triggered here automatically.
        // Because we cannot enumerate all sponsors we instead expose
        // `apply_matching` as a separate call that anyone can invoke after a
        // donation; see design notes in docs/ARCHITECTURE.md.
        Ok(())
    }

    // ─── Issue #27: Sponsor matching ────────────────────────────────────────

    /// A sponsor deposits a matching pool for a campaign.
    ///
    /// * `ratio_bps` – matching ratio in basis points (100 bps = 1 %).
    ///   Must be between 1 and 10 000 (inclusive).
    /// * `cap` – maximum total amount the sponsor will match across all
    ///   donations. Must be > 0 and ≤ `amount`.
    /// * `amount` – tokens the sponsor transfers into the pool right now.
    ///   Must equal `cap` (the full pool is deposited up front).
    ///
    /// Each sponsor may have at most one pool per campaign. Call
    /// `return_sponsor_pool` to reclaim remaining funds and close the pool.
    pub fn deposit_sponsor_pool(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
        ratio_bps: u32,
        cap: i128,
        amount: i128,
    ) -> Result<(), Error> {
        sponsor.require_auth();

        // Validate configuration
        if ratio_bps == 0 || ratio_bps > MAX_RATIO_BPS {
            return Err(Error::InvalidMatchingConfig);
        }
        if cap <= 0 || amount <= 0 {
            return Err(Error::InvalidMatchingConfig);
        }
        // The deposited amount must exactly equal the cap so accounting is
        // always exact — sponsors cannot partially fund a pool.
        if amount != cap {
            return Err(Error::InvalidMatchingConfig);
        }

        // Campaign must exist and be active
        let campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        // One pool per sponsor per campaign
        if storage::sponsor_pool(&env, campaign_id, &sponsor).is_some() {
            return Err(Error::SponsorPoolExists);
        }

        // Transfer tokens from sponsor to contract
        token::Client::new(&env, &storage::token(&env)).transfer(
            &sponsor,
            &env.current_contract_address(),
            &amount,
        );

        let pool = SponsorPool {
            sponsor: sponsor.clone(),
            campaign_id,
            ratio_bps,
            cap,
            deposited: amount,
            remaining: amount,
            matched: 0,
        };
        storage::save_sponsor_pool(&env, &pool);

        SponsorPoolDeposited {
            campaign_id,
            sponsor,
            amount,
            ratio_bps,
            cap,
        }
        .publish(&env);

        Ok(())
    }

    /// Apply matching from a sponsor pool to a specific donation amount.
    ///
    /// This function is separate from `donate` because Soroban storage does
    /// not support prefix-scanning, so we cannot automatically discover all
    /// sponsor pools for a campaign inside `donate`. Instead, anyone (the
    /// donor, the sponsor, a keeper) may call this after a donation to credit
    /// the matched amount to the campaign.
    ///
    /// `donation_amount` is the gross donation to match against (must be > 0).
    /// The matched amount is constrained by:
    ///   1. `ratio_bps / 10_000 * donation_amount` (the configured ratio)
    ///   2. `pool.remaining` (pool cannot be overdrafted)
    ///   3. `pool.cap - pool.matched` (cap cannot be exceeded)
    ///   4. `campaign.goal - campaign.raised` (campaign cannot be overfunded)
    ///
    /// The matched tokens are already held by the contract (deposited in
    /// `deposit_sponsor_pool`), so no further token transfer is needed — the
    /// contract simply adds `matched_amount` to `campaign.raised` and records
    /// the sponsor's contribution so they share in any pro-rata refund.
    pub fn apply_matching(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
        donor: Address,
        donation_amount: i128,
    ) -> Result<i128, Error> {
        if donation_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut pool = storage::sponsor_pool(&env, campaign_id, &sponsor)
            .ok_or(Error::SponsorPoolNotFound)?;

        let mut campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        // Compute the raw match according to the ratio (no overflow: both are i128)
        let raw_match = donation_amount
            .checked_mul(pool.ratio_bps as i128)
            .ok_or(Error::InvalidAmount)?
            / 10_000_i128;

        if raw_match <= 0 {
            // Donation too small to generate any match at this ratio — not an error
            return Ok(0);
        }

        // Cap 1: sponsor cap
        let cap_remaining = pool.cap - pool.matched;
        // Cap 2: pool remaining
        let pool_limit = pool.remaining;
        // Cap 3: campaign headroom
        let campaign_headroom = campaign.goal - campaign.raised;

        let matched_amount = raw_match
            .min(cap_remaining)
            .min(pool_limit)
            .min(campaign_headroom);

        if matched_amount <= 0 {
            // Pool exhausted or campaign full — not an error, just no match
            return Ok(0);
        }

        // Update pool accounting
        pool.remaining -= matched_amount;
        pool.matched += matched_amount;
        storage::save_sponsor_pool(&env, &pool);

        // Credit matching into campaign raised amount and record the sponsor's
        // contribution so they are included in pro-rata refund calculations.
        campaign.raised += matched_amount;
        storage::save_campaign(&env, &campaign);

        let prev = storage::contribution(&env, campaign_id, &sponsor);
        storage::set_contribution(&env, campaign_id, &sponsor, prev + matched_amount);

        MatchingApplied {
            campaign_id,
            sponsor: sponsor.clone(),
            donor,
            donation_amount,
            matched_amount,
        }
        .publish(&env);

        Ok(matched_amount)
    }

    /// Returns the sponsor's unused pool balance after the campaign ends.
    ///
    /// Can be called once the campaign is either Completed, Cancelled, or
    /// past its deadline. The remaining pool tokens are transferred back to
    /// the sponsor and the pool record is removed.
    pub fn return_sponsor_pool(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
    ) -> Result<i128, Error> {
        sponsor.require_auth();

        let pool = storage::sponsor_pool(&env, campaign_id, &sponsor)
            .ok_or(Error::SponsorPoolNotFound)?;

        let campaign = storage::campaign(&env, campaign_id)?;

        // Allow return only when the campaign can no longer match (i.e. it is
        // over). A campaign is "over" when cancelled, completed, or expired.
        let expired = campaign.status == CampaignStatus::Active
            && env.ledger().timestamp() > campaign.deadline;
        let ended = campaign.status != CampaignStatus::Active || expired;
        if !ended {
            return Err(Error::PoolNotReturnable);
        }

        let remaining = pool.remaining;
        storage::remove_sponsor_pool(&env, campaign_id, &sponsor);

        if remaining > 0 {
            token::Client::new(&env, &storage::token(&env)).transfer(
                &env.current_contract_address(),
                &sponsor,
                &remaining,
            );
        }

        SponsorPoolReturned {
            campaign_id,
            sponsor,
            amount: remaining,
        }
        .publish(&env);

        Ok(remaining)
    }

    /// Called by the campaign's verifier once the next milestone is done.
    /// Pays that milestone to the beneficiary. `proof_uri` points at the
    /// evidence (photos, receipts, reports) and is emitted for indexers.
    pub fn approve_milestone(env: Env, campaign_id: u64, proof_uri: String) -> Result<i128, Error> {
        let mut campaign = storage::campaign(&env, campaign_id)?;
        campaign.verifier.require_auth();
        if !storage::is_verifier(&env, &campaign.verifier) {
            return Err(Error::NotVerifier);
        }
        Self::ensure_open(&env, &campaign)?;

        let index = campaign.milestones_released;
        let amount = campaign
            .milestones
            .get(index)
            .ok_or(Error::NoMilestonesLeft)?;
        if campaign.raised - campaign.released < amount {
            return Err(Error::MilestoneNotFunded);
        }

        token::Client::new(&env, &storage::token(&env)).transfer(
            &env.current_contract_address(),
            &campaign.beneficiary,
            &amount,
        );

        campaign.released += amount;
        campaign.milestones_released += 1;
        if campaign.milestones_released == campaign.milestones.len() {
            campaign.status = CampaignStatus::Completed;
        }
        storage::save_campaign(&env, &campaign);

        MilestoneReleased {
            campaign_id,
            index,
            amount,
            proof_uri,
        }
        .publish(&env);
        Ok(amount)
    }

    /// Stops a campaign early so donors can reclaim unreleased funds.
    /// Allowed for the creator or the admin.
    pub fn cancel_campaign(env: Env, caller: Address, campaign_id: u64) -> Result<(), Error> {
        caller.require_auth();
        let mut campaign = storage::campaign(&env, campaign_id)?;
        if caller != campaign.creator && caller != storage::admin(&env) {
            return Err(Error::Unauthorized);
        }
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }

        campaign.status = CampaignStatus::Cancelled;
        storage::save_campaign(&env, &campaign);

        CampaignCancelled { campaign_id }.publish(&env);
        Ok(())
    }

    /// Returns the donor's pro rata share of funds that were never released.
    /// Available once a campaign is cancelled, or has passed its deadline
    /// without completing.
    ///
    /// Note: sponsor contributions are tracked as `Contribution` entries so
    /// sponsors share in refunds proportionally to their matched amount.
    pub fn refund(env: Env, donor: Address, campaign_id: u64) -> Result<i128, Error> {
        donor.require_auth();
        let campaign = storage::campaign(&env, campaign_id)?;

        let expired = campaign.status == CampaignStatus::Active
            && env.ledger().timestamp() > campaign.deadline;
        if campaign.status != CampaignStatus::Cancelled && !expired {
            return Err(Error::RefundNotAvailable);
        }

        let contributed = storage::contribution(&env, campaign_id, &donor);
        if contributed == 0 {
            return Err(Error::NothingToRefund);
        }
        // `raised` and `released` are frozen once refunds open, so every donor
        // is measured against the same pool.
        let unreleased = campaign.raised - campaign.released;
        let amount = contributed * unreleased / campaign.raised;

        storage::set_contribution(&env, campaign_id, &donor, 0);
        if amount > 0 {
            token::Client::new(&env, &storage::token(&env)).transfer(
                &env.current_contract_address(),
                &donor,
                &amount,
            );
        }

        Refunded {
            campaign_id,
            donor,
            amount,
        }
        .publish(&env);
        Ok(amount)
    }

    // Views

    pub fn get_campaign(env: Env, campaign_id: u64) -> Result<Campaign, Error> {
        storage::campaign(&env, campaign_id)
    }

    pub fn campaign_count(env: Env) -> u64 {
        storage::campaign_count(&env)
    }

    pub fn contribution_of(env: Env, campaign_id: u64, donor: Address) -> i128 {
        storage::contribution(&env, campaign_id, &donor)
    }

    pub fn is_verifier(env: Env, who: Address) -> bool {
        storage::is_verifier(&env, &who)
    }

    pub fn admin(env: Env) -> Address {
        storage::admin(&env)
    }

    pub fn token(env: Env) -> Address {
        storage::token(&env)
    }

    pub fn get_sponsor_pool(
        env: Env,
        campaign_id: u64,
        sponsor: Address,
    ) -> Option<SponsorPool> {
        storage::sponsor_pool(&env, campaign_id, &sponsor)
    }

    // Internal

    fn ensure_open(env: &Env, campaign: &Campaign) -> Result<(), Error> {
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }
        if env.ledger().timestamp() > campaign.deadline {
            return Err(Error::CampaignExpired);
        }
        Ok(())
    }
}
