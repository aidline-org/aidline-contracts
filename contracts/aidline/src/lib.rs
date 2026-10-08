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
pub use types::{BondStatus, Campaign, CampaignKind, CampaignStatus, SponsorPool, VerifierBond};

use events::{
    BondSlashed, BondWithdrawn, CampaignCancelled, CampaignCreated, Donated, MatchingApplied,
    MilestoneReleased, Refunded, SponsorPoolDeposited, SponsorPoolReturned, VerifierBondPosted,
    VerifierDeregistered, VerifierUpdated,
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

    // ─── Admin ────────────────────────────────────────────────────────────────

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

    // ─── Issue #29: Bond configuration (admin only) ───────────────────────────

    /// Set the minimum bond amount a verifier must post when registering.
    /// Set to 0 to disable the bond requirement (default).
    pub fn set_bond_requirement(env: Env, amount: i128) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        storage::set_bond_requirement(&env, amount);
        Ok(())
    }

    /// Set the delay (in seconds) after deregistration before a verifier can
    /// withdraw their bond. Default is 0 (no delay).
    pub fn set_bond_withdraw_delay(env: Env, delay: u64) {
        storage::admin(&env).require_auth();
        storage::set_bond_withdraw_delay(&env, delay);
    }

    // ─── Issue #29: Bonded verifier registration ──────────────────────────────

    /// Register as a verifier by posting the required bond.
    ///
    /// `bond_amount` must be >= the configured `bond_requirement`. The tokens
    /// are transferred from the verifier to the contract and held until the
    /// verifier deregisters and the withdrawal delay expires.
    pub fn register_with_bond(
        env: Env,
        verifier: Address,
        bond_amount: i128,
    ) -> Result<(), Error> {
        verifier.require_auth();

        let required = storage::bond_requirement(&env);
        if bond_amount < required || bond_amount <= 0 {
            return Err(Error::BondRequired);
        }

        // Prevent double-registration
        if storage::is_verifier(&env, &verifier) {
            return Err(Error::Unauthorized);
        }

        // Transfer bond from verifier to contract
        token::Client::new(&env, &storage::token(&env)).transfer(
            &verifier,
            &env.current_contract_address(),
            &bond_amount,
        );

        let bond = VerifierBond {
            verifier: verifier.clone(),
            amount: bond_amount,
            remaining: bond_amount,
            status: BondStatus::Active,
            deregistered_at: 0,
        };
        storage::save_verifier_bond(&env, &bond);
        storage::set_verifier(&env, &verifier, true);

        VerifierBondPosted {
            verifier,
            amount: bond_amount,
        }
        .publish(&env);

        Ok(())
    }

    /// Deregister as a verifier and start the withdrawal delay timer.
    ///
    /// The verifier remains in `PendingWithdrawal` state; their bond cannot be
    /// withdrawn until `bond_withdraw_delay` seconds have elapsed.
    /// The verifier's campaigns are frozen (as with `remove_verifier`).
    ///
    /// Can be called by the verifier themselves or by the admin.
    pub fn deregister_verifier(env: Env, caller: Address, verifier: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = storage::admin(&env);
        if caller != verifier && caller != admin {
            return Err(Error::Unauthorized);
        }

        // Verifier must be currently active
        if !storage::is_verifier(&env, &verifier) {
            return Err(Error::NotVerifier);
        }

        // Remove from active registry
        storage::set_verifier(&env, &verifier, false);

        let now = env.ledger().timestamp();

        // Update bond record if one exists
        if let Some(mut bond) = storage::verifier_bond(&env, &verifier) {
            bond.status = BondStatus::PendingWithdrawal;
            bond.deregistered_at = now;
            storage::save_verifier_bond(&env, &bond);
        }

        VerifierDeregistered {
            verifier,
            deregistered_at: now,
        }
        .publish(&env);

        Ok(())
    }

    /// Withdraw the verifier's bond after the withdrawal delay has passed.
    ///
    /// Only the verifier themselves can withdraw their own bond.
    /// A slashed bond cannot be withdrawn beyond the remaining amount.
    pub fn withdraw_bond(env: Env, verifier: Address) -> Result<i128, Error> {
        verifier.require_auth();

        let mut bond = storage::verifier_bond(&env, &verifier).ok_or(Error::BondNotFound)?;

        // Bond must be in PendingWithdrawal (not Active or Withdrawn or Slashed)
        if bond.status != BondStatus::PendingWithdrawal {
            return Err(Error::BondNotWithdrawable);
        }

        // Enforce withdrawal delay
        let delay = storage::bond_withdraw_delay(&env);
        let elapsed = env.ledger().timestamp() - bond.deregistered_at;
        if elapsed < delay {
            return Err(Error::WithdrawDelayNotMet);
        }

        let withdrawable = bond.remaining;
        if withdrawable <= 0 {
            return Err(Error::BondNotWithdrawable);
        }

        bond.remaining = 0;
        bond.status = BondStatus::Withdrawn;
        storage::save_verifier_bond(&env, &bond);

        token::Client::new(&env, &storage::token(&env)).transfer(
            &env.current_contract_address(),
            &verifier,
            &withdrawable,
        );

        BondWithdrawn {
            verifier,
            amount: withdrawable,
        }
        .publish(&env);

        Ok(withdrawable)
    }

    /// Admin slashes a verifier's bond after a documented dispute.
    ///
    /// The slashed tokens are distributed back to the affected campaign's
    /// donors by adding them to `campaign.raised` and crediting the contract
    /// address as a contributor. This integrates with the existing pro-rata
    /// refund formula: when donors call `refund`, the recovered funds are
    /// returned proportionally to their original contributions.
    ///
    /// * `verifier` — the verifier whose bond is being slashed.
    /// * `campaign_id` — the campaign affected by the fraud.
    /// * `slash_amount` — tokens to slash. Must be <= bond.remaining.
    pub fn slash_verifier(
        env: Env,
        verifier: Address,
        campaign_id: u64,
        slash_amount: i128,
    ) -> Result<(), Error> {
        storage::admin(&env).require_auth();

        if slash_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut bond = storage::verifier_bond(&env, &verifier).ok_or(Error::BondNotFound)?;

        if bond.status == BondStatus::Withdrawn {
            return Err(Error::BondNotWithdrawable);
        }
        if slash_amount > bond.remaining {
            return Err(Error::SlashExceedsBond);
        }

        bond.remaining -= slash_amount;
        if bond.remaining == 0 {
            bond.status = BondStatus::Slashed;
        }
        storage::save_verifier_bond(&env, &bond);

        // Distribute slashed funds to the campaign's donors via the refund
        // mechanism. We add the slashed amount to campaign.raised and record
        // it as a contribution from the contract address so the refund formula
        // distributes it pro-rata when donors call refund.
        //
        // Note: The campaign may be in any state (including completed or
        // cancelled). If it is completed, the slashed funds are held by the
        // contract until the admin manually arranges distribution (a known
        // limitation). For cancelled or expired campaigns, donors can
        // immediately call refund to receive their share.
        let campaign_result = storage::campaign(&env, campaign_id);
        if let Ok(mut campaign) = campaign_result {
            let contract_addr = env.current_contract_address();
            let prev = storage::contribution(&env, campaign_id, &contract_addr);
            // Add to raised so the refund formula treats it as recoverable funds.
            campaign.raised = campaign
                .raised
                .checked_add(slash_amount)
                .unwrap_or(campaign.raised);
            storage::save_campaign(&env, &campaign);
            storage::set_contribution(
                &env,
                campaign_id,
                &contract_addr,
                prev + slash_amount,
            );
        }
        // If the campaign is not found, the tokens remain in the contract.
        // This is a known edge case; a future improvement should track them.

        BondSlashed {
            verifier: verifier.clone(),
            campaign_id,
            slashed_amount: slash_amount,
        }
        .publish(&env);

        Ok(())
    }

    // Views for bond state

    pub fn get_verifier_bond(env: Env, verifier: Address) -> Option<VerifierBond> {
        storage::verifier_bond(&env, &verifier)
    }

    pub fn bond_requirement(env: Env) -> i128 {
        storage::bond_requirement(&env)
    }

    pub fn bond_withdraw_delay(env: Env) -> u64 {
        storage::bond_withdraw_delay(&env)
    }

    // ─── Campaigns ────────────────────────────────────────────────────────────

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

        Ok(())
    }

    // ─── Issue #27: Sponsor matching ─────────────────────────────────────────

    /// A sponsor deposits a matching pool for a campaign.
    pub fn deposit_sponsor_pool(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
        ratio_bps: u32,
        cap: i128,
        amount: i128,
    ) -> Result<(), Error> {
        sponsor.require_auth();

        if ratio_bps == 0 || ratio_bps > MAX_RATIO_BPS {
            return Err(Error::InvalidMatchingConfig);
        }
        if cap <= 0 || amount <= 0 {
            return Err(Error::InvalidMatchingConfig);
        }
        if amount != cap {
            return Err(Error::InvalidMatchingConfig);
        }

        let campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        if storage::sponsor_pool(&env, campaign_id, &sponsor).is_some() {
            return Err(Error::SponsorPoolExists);
        }

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

        let raw_match = donation_amount
            .checked_mul(pool.ratio_bps as i128)
            .ok_or(Error::InvalidAmount)?
            / 10_000_i128;

        if raw_match <= 0 {
            return Ok(0);
        }

        let cap_remaining = pool.cap - pool.matched;
        let pool_limit = pool.remaining;
        let campaign_headroom = campaign.goal - campaign.raised;

        let matched_amount = raw_match
            .min(cap_remaining)
            .min(pool_limit)
            .min(campaign_headroom);

        if matched_amount <= 0 {
            return Ok(0);
        }

        pool.remaining -= matched_amount;
        pool.matched += matched_amount;
        storage::save_sponsor_pool(&env, &pool);

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
    pub fn return_sponsor_pool(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
    ) -> Result<i128, Error> {
        sponsor.require_auth();

        let pool = storage::sponsor_pool(&env, campaign_id, &sponsor)
            .ok_or(Error::SponsorPoolNotFound)?;

        let campaign = storage::campaign(&env, campaign_id)?;

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

    // ─── Views ────────────────────────────────────────────────────────────────

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

    // ─── Internal ─────────────────────────────────────────────────────────────

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
