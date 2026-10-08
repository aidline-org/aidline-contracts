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
pub use types::{
    BondStatus, Campaign, CampaignKind, CampaignStatus, Pledge, SponsorPool, VerifierBond,
};

use events::{
    BondSlashed, BondWithdrawn, CampaignCancelled, CampaignCreated, Donated, MatchingApplied,
    MilestoneReleased, PledgeCreated, PledgePulled, PledgeSettlement, PledgeSkipped, Refunded,
    SponsorPoolDeposited, SponsorPoolReturned, VerifierBondPosted, VerifierDeregistered,
    VerifierUpdated,
};

const MAX_MILESTONES: u32 = 20;
/// Maximum matching ratio (100 % = 10 000 bps).
const MAX_RATIO_BPS: u32 = 10_000;
/// Reason code emitted in PledgeSkipped when the donor has no allowance.
const SKIP_REASON_NO_ALLOWANCE: u32 = 1;
/// Reason code emitted in PledgeSkipped when the pledge is fully consumed.
const SKIP_REASON_EXHAUSTED: u32 = 2;

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

    pub fn set_bond_requirement(env: Env, amount: i128) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        storage::set_bond_requirement(&env, amount);
        Ok(())
    }

    pub fn set_bond_withdraw_delay(env: Env, delay: u64) {
        storage::admin(&env).require_auth();
        storage::set_bond_withdraw_delay(&env, delay);
    }

    // ─── Issue #29: Bonded verifier registration ──────────────────────────────

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

        if storage::is_verifier(&env, &verifier) {
            return Err(Error::Unauthorized);
        }

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

    pub fn deregister_verifier(env: Env, caller: Address, verifier: Address) -> Result<(), Error> {
        caller.require_auth();
        let admin = storage::admin(&env);
        if caller != verifier && caller != admin {
            return Err(Error::Unauthorized);
        }

        if !storage::is_verifier(&env, &verifier) {
            return Err(Error::NotVerifier);
        }

        storage::set_verifier(&env, &verifier, false);

        let now = env.ledger().timestamp();

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

    pub fn withdraw_bond(env: Env, verifier: Address) -> Result<i128, Error> {
        verifier.require_auth();

        let mut bond = storage::verifier_bond(&env, &verifier).ok_or(Error::BondNotFound)?;

        if bond.status != BondStatus::PendingWithdrawal {
            return Err(Error::BondNotWithdrawable);
        }

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

        let campaign_result = storage::campaign(&env, campaign_id);
        if let Ok(mut campaign) = campaign_result {
            let contract_addr = env.current_contract_address();
            let prev = storage::contribution(&env, campaign_id, &contract_addr);
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

    // ─── Issue #30: Pledges ───────────────────────────────────────────────────

    /// A donor creates a pledge for a campaign.
    ///
    /// The donor must have granted the Aidline contract a token allowance of
    /// at least `pledged_amount` before calling this. The pledge is recorded
    /// persistently and funds are pulled proportionally when milestones are
    /// approved.
    ///
    /// `pledged_amount` must be > 0. The campaign must be active.
    ///
    /// Pledges are supplementary to direct donations: the milestone funding
    /// check (`raised - released >= milestone_amount`) uses only direct
    /// donations and matched funds. Pledges are pulled on top of that to
    /// pre-fill future milestones.
    pub fn create_pledge(
        env: Env,
        donor: Address,
        campaign_id: u64,
        pledged_amount: i128,
    ) -> Result<u64, Error> {
        donor.require_auth();

        if pledged_amount <= 0 {
            return Err(Error::InvalidPledge);
        }

        let campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        // Verify the donor has granted sufficient allowance.
        // We do not transfer tokens here — they are pulled at milestone approval.
        let tok = token::Client::new(&env, &storage::token(&env));
        let allowance = tok.allowance(&donor, &env.current_contract_address());
        if allowance < pledged_amount {
            return Err(Error::InvalidPledge);
        }

        let pledge_id = storage::next_pledge_id(&env);
        let pledge = Pledge {
            pledge_id,
            donor: donor.clone(),
            campaign_id,
            pledged_amount,
            pulled_amount: 0,
            active: true,
        };
        storage::save_pledge(&env, &pledge);

        PledgeCreated {
            pledge_id,
            campaign_id,
            donor,
            pledged_amount,
        }
        .publish(&env);

        Ok(pledge_id)
    }

    /// Called by the campaign's verifier once the next milestone is done.
    ///
    /// In addition to releasing the milestone from direct donations (the
    /// original behavior), this function also pulls from active pledges
    /// proportionally to cover the milestone amount. Pledge pulling is
    /// best-effort: if a pledge's allowance has lapsed, the pledge is skipped
    /// and a `PledgeSkipped` event is emitted.
    ///
    /// Pledge funds pulled are added to `campaign.raised` so they are included
    /// in the milestone release. If the total pledged pull is insufficient to
    /// cover the milestone (because allowances lapsed), the milestone approval
    /// may still fail with `MilestoneNotFunded` unless direct donations cover
    /// the shortfall.
    ///
    /// `proof_uri` points at the evidence and is emitted for indexers.
    pub fn approve_milestone(env: Env, campaign_id: u64, proof_uri: String) -> Result<i128, Error> {
        let mut campaign = storage::campaign(&env, campaign_id)?;
        campaign.verifier.require_auth();
        if !storage::is_verifier(&env, &campaign.verifier) {
            return Err(Error::NotVerifier);
        }
        Self::ensure_open(&env, &campaign)?;

        let index = campaign.milestones_released;
        let milestone_amount = campaign
            .milestones
            .get(index)
            .ok_or(Error::NoMilestonesLeft)?;

        // ── Issue #30: Pull from pledges before checking funding ─────────────
        // We iterate over pledges by pledge_id from 0 to the current count.
        // Pledges not belonging to this campaign are skipped.
        // This approach avoids the need for a campaign→pledges index.
        let pledge_count = storage::next_pledge_id_peek(&env);
        let total_pledged_pulled =
            Self::pull_pledges(&env, campaign_id, milestone_amount, pledge_count, &mut campaign);

        // Emit settlement event
        PledgeSettlement {
            campaign_id,
            milestone_index: index,
            total_pledged_pulled,
        }
        .publish(&env);

        // ── Original milestone release logic ─────────────────────────────────
        if campaign.raised - campaign.released < milestone_amount {
            return Err(Error::MilestoneNotFunded);
        }

        token::Client::new(&env, &storage::token(&env)).transfer(
            &env.current_contract_address(),
            &campaign.beneficiary,
            &milestone_amount,
        );

        campaign.released += milestone_amount;
        campaign.milestones_released += 1;
        if campaign.milestones_released == campaign.milestones.len() {
            campaign.status = CampaignStatus::Completed;
        }
        storage::save_campaign(&env, &campaign);

        MilestoneReleased {
            campaign_id,
            index,
            amount: milestone_amount,
            proof_uri,
        }
        .publish(&env);
        Ok(milestone_amount)
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

    pub fn get_pledge(env: Env, pledge_id: u64) -> Option<Pledge> {
        storage::pledge(&env, pledge_id)
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

    /// Pull pledge funds proportionally toward `milestone_amount`.
    ///
    /// Returns the total amount actually pulled from pledges.
    ///
    /// Algorithm:
    /// 1. Collect all active pledges for the campaign.
    /// 2. Compute total remaining pledge capacity.
    /// 3. For each pledge compute its proportional share of `milestone_amount`.
    /// 4. Clamp to: min(share, pledge.remaining, allowance).
    /// 5. If pull > 0, transfer and record contribution.
    /// 6. If allowance == 0 or pull == 0, emit PledgeSkipped.
    ///
    /// Rounding: each share is `milestone_amount * pledge.remaining / total_remaining`,
    /// using integer division truncated toward zero. Any unallocated remainder
    /// stays in the milestone's direct-donation funding.
    fn pull_pledges(
        env: &Env,
        campaign_id: u64,
        milestone_amount: i128,
        pledge_count: u64,
        campaign: &mut Campaign,
    ) -> i128 {
        let tok = token::Client::new(env, &storage::token(env));
        let contract_addr = env.current_contract_address();

        // Phase 1: gather active pledges for this campaign
        // We scan all pledge IDs. In production the pledge count is bounded by
        // the number of create_pledge calls across all campaigns; for a mature
        // system an off-chain index (or a per-campaign pledge list stored
        // separately) would be more efficient. This MVP scans linearly.
        let mut active_pledges: Vec<Pledge> = Vec::new(env);
        let mut total_remaining: i128 = 0;
        let mut i: u64 = 0;
        while i < pledge_count {
            if let Some(p) = storage::pledge(env, i) {
                if p.campaign_id == campaign_id && p.active && p.pulled_amount < p.pledged_amount {
                    total_remaining += p.pledged_amount - p.pulled_amount;
                    active_pledges.push_back(p);
                }
            }
            i += 1;
        }

        if total_remaining == 0 || active_pledges.is_empty() {
            return 0;
        }

        // Phase 2: pull proportionally
        let mut total_pulled: i128 = 0;

        for mut pledge in active_pledges.iter() {
            let pledge_remaining = pledge.pledged_amount - pledge.pulled_amount;

            // Proportional share: milestone_amount * pledge_remaining / total_remaining
            let share = milestone_amount
                .checked_mul(pledge_remaining)
                .unwrap_or(i128::MAX)
                / total_remaining;

            if share <= 0 {
                PledgeSkipped {
                    pledge_id: pledge.pledge_id,
                    campaign_id,
                    donor: pledge.donor.clone(),
                    reason: SKIP_REASON_EXHAUSTED,
                }
                .publish(env);
                continue;
            }

            // Check allowance
            let allowance = tok.allowance(&pledge.donor, &contract_addr);
            if allowance <= 0 {
                PledgeSkipped {
                    pledge_id: pledge.pledge_id,
                    campaign_id,
                    donor: pledge.donor.clone(),
                    reason: SKIP_REASON_NO_ALLOWANCE,
                }
                .publish(env);
                continue;
            }

            // Clamp to what is actually available
            let pull = share.min(pledge_remaining).min(allowance);
            if pull <= 0 {
                PledgeSkipped {
                    pledge_id: pledge.pledge_id,
                    campaign_id,
                    donor: pledge.donor.clone(),
                    reason: SKIP_REASON_NO_ALLOWANCE,
                }
                .publish(env);
                continue;
            }

            // Transfer tokens from donor to contract
            tok.transfer_from(
                &contract_addr,
                &pledge.donor,
                &contract_addr,
                &pull,
            );

            // Update pledge record
            pledge.pulled_amount += pull;
            if pledge.pulled_amount >= pledge.pledged_amount {
                pledge.active = false;
            }
            storage::save_pledge(env, &pledge);

            // Record as contribution so donor shares in refunds
            let prev = storage::contribution(env, campaign_id, &pledge.donor);
            storage::set_contribution(env, campaign_id, &pledge.donor, prev + pull);

            // Increase campaign.raised
            campaign.raised = campaign.raised.checked_add(pull).unwrap_or(campaign.raised);
            total_pulled += pull;

            PledgePulled {
                pledge_id: pledge.pledge_id,
                campaign_id,
                donor: pledge.donor.clone(),
                pulled_amount: pull,
            }
            .publish(env);
        }

        total_pulled
    }
}
