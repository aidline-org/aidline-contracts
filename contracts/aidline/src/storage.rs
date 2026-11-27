use soroban_sdk::{Address, Env, String, Vec};

use crate::errors::Error;
use crate::types::{
    Beneficiary, Campaign, DataKey, PendingApproval, Pledge, QueuedAction, Role, SponsorPool,
    Stream, VerifierBond, VerifierStats,
};

const DAY_IN_LEDGERS: u32 = 17_280;
/// Ledger close time `DAY_IN_LEDGERS` assumes. Real ledgers close slightly
/// slower, so converting seconds with it overestimates the ledgers needed and
/// errs on the side of keeping data alive longer.
const LEDGER_SECONDS: u64 = 86_400 / DAY_IN_LEDGERS as u64;
pub(crate) const INSTANCE_BUMP: u32 = 30 * DAY_IN_LEDGERS;
pub(crate) const INSTANCE_THRESHOLD: u32 = INSTANCE_BUMP - DAY_IN_LEDGERS;
pub(crate) const PERSISTENT_BUMP: u32 = 120 * DAY_IN_LEDGERS;
pub(crate) const PERSISTENT_THRESHOLD: u32 = PERSISTENT_BUMP - 7 * DAY_IN_LEDGERS;

pub fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_THRESHOLD, INSTANCE_BUMP);
}

fn bump(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_THRESHOLD, PERSISTENT_BUMP);
}

/// Extends `key` only if it exists, for optional per-campaign entries.
fn bump_if_present(env: &Env, key: &DataKey) {
    if env.storage().persistent().has(key) {
        bump(env, key);
    }
}

/// Ledgers between now and `timestamp`, rounded up (0 once it has passed).
fn ledgers_until(env: &Env, timestamp: u64) -> u32 {
    let seconds = timestamp.saturating_sub(env.ledger().timestamp());
    u32::try_from(seconds.div_ceil(LEDGER_SECONDS)).unwrap_or(u32::MAX)
}

/// Like [`bump`], but keeps `key` alive until at least a week past
/// `timestamp` when that is further away than the usual 120 days. The result
/// is capped at the network's maximum TTL. Repeated calls before `timestamp`
/// do nothing, because the entry already expires at the same ledger.
fn bump_until(env: &Env, key: &DataKey, timestamp: u64) {
    let grace = PERSISTENT_BUMP - PERSISTENT_THRESHOLD;
    let extend_to = ledgers_until(env, timestamp)
        .saturating_add(grace)
        .max(PERSISTENT_BUMP)
        .min(env.storage().max_ttl());
    let threshold = extend_to.saturating_sub(grace);
    env.storage()
        .persistent()
        .extend_ttl(key, threshold, extend_to);
}

pub fn admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, pending: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::PendingAdmin, pending);
}

pub fn remove_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

pub fn has_role(env: &Env, role: Role, who: &Address) -> bool {
    if role == Role::Admin && *who == admin(env) {
        return true;
    }
    let key = DataKey::Role(role, who.clone());
    let granted: Option<bool> = env.storage().persistent().get(&key);
    if granted == Some(true) {
        bump(env, &key);
    }
    granted.unwrap_or(false)
}

pub fn set_role(env: &Env, role: Role, who: &Address, granted: bool) {
    let key = DataKey::Role(role, who.clone());
    if granted {
        env.storage().persistent().set(&key, &true);
        bump(env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }
}

/// The address that may slash verifier bonds. Defaults to the current admin,
/// so a deployment that never calls `set_slasher` behaves as before.
pub fn slasher(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Slasher)
        .unwrap_or_else(|| admin(env))
}

pub fn set_slasher(env: &Env, slasher: &Address) {
    env.storage().instance().set(&DataKey::Slasher, slasher);
}

pub fn challenge_window(env: &Env, campaign_id: u64) -> u64 {
    let key = DataKey::ChallengeWindow(campaign_id);
    let window: Option<u64> = env.storage().persistent().get(&key);
    if window.is_some() {
        bump(env, &key);
    }
    window.unwrap_or(0)
}

pub fn set_challenge_window(env: &Env, campaign_id: u64, window: u64) {
    let key = DataKey::ChallengeWindow(campaign_id);
    env.storage().persistent().set(&key, &window);
    bump(env, &key);
}

pub fn pending_approval(env: &Env, campaign_id: u64) -> Option<PendingApproval> {
    let key = DataKey::PendingApproval(campaign_id);
    let pending = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(pending)
}

pub fn save_pending_approval(env: &Env, campaign_id: u64, pending: &PendingApproval) {
    let key = DataKey::PendingApproval(campaign_id);
    env.storage().persistent().set(&key, pending);
    bump(env, &key);
}

pub fn remove_pending_approval(env: &Env, campaign_id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::PendingApproval(campaign_id));
}

/// Returns the next proposal number for the campaign, starting at 1.
pub fn next_proposal_nonce(env: &Env, campaign_id: u64) -> u32 {
    let key = DataKey::ProposalNonce(campaign_id);
    let nonce: u32 = env.storage().persistent().get(&key).unwrap_or(0) + 1;
    env.storage().persistent().set(&key, &nonce);
    bump(env, &key);
    nonce
}

pub fn has_block_vote(env: &Env, campaign_id: u64, nonce: u32, donor: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::BlockVote(campaign_id, nonce, donor.clone()))
}

pub fn set_block_vote(env: &Env, campaign_id: u64, nonce: u32, donor: &Address) {
    let key = DataKey::BlockVote(campaign_id, nonce, donor.clone());
    env.storage().persistent().set(&key, &true);
    bump(env, &key);
}

pub fn token(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Token).unwrap()
}

pub fn set_token(env: &Env, token: &Address) {
    env.storage().instance().set(&DataKey::Token, token);
}

/// Returns the next campaign id and advances the counter.
pub fn next_campaign_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::CampaignCount)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::CampaignCount, &(id + 1));
    id
}

pub fn campaign_count(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::CampaignCount)
        .unwrap_or(0)
}

pub fn is_verifier(env: &Env, who: &Address) -> bool {
    let key = DataKey::Verifier(who.clone());
    let registered: Option<bool> = env.storage().persistent().get(&key);
    // Reads keep the registration alive, so a verifier that is still in use is
    // never silently dropped when its entry would otherwise expire (#110, #181).
    if registered == Some(true) {
        bump(env, &key);
    }
    registered.unwrap_or(false)
}

pub fn set_verifier(env: &Env, who: &Address, active: bool) {
    let key = DataKey::Verifier(who.clone());
    if active {
        env.storage().persistent().set(&key, &true);
        bump(env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }
}

pub fn verifier_profile(env: &Env, who: &Address) -> Option<String> {
    let key = DataKey::VerifierProfile(who.clone());
    let profile = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(profile)
}

pub fn set_verifier_profile(env: &Env, who: &Address, uri: &String) {
    let key = DataKey::VerifierProfile(who.clone());
    env.storage().persistent().set(&key, uri);
    bump(env, &key);
}

pub fn campaign(env: &Env, id: u64) -> Result<Campaign, Error> {
    // Every campaign call passes through here, so this is what keeps the
    // instance entry (admin, token, counters, fees) alive on a deployment that
    // is busy but creates no new campaigns (#110).
    bump_instance(env);
    let key = DataKey::Campaign(id);
    let campaign = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::CampaignNotFound)?;
    bump(env, &key);
    Ok(campaign)
}

/// Extends the TTL of the instance entry, campaign `id` and the
/// campaign-wide entries that exist for it: due dates (kept until the
/// deadline), split beneficiaries, stream schedule, donor and pledge lists,
/// start time, slashed recovery and the cancel-vote total. Per-donor entries
/// (contributions, cancel votes) are extended whenever that donor's entry is
/// read or written.
pub fn bump_campaign_ttl(env: &Env, id: u64) -> Result<(), Error> {
    let campaign = campaign(env, id)?;
    extend_milestone_due_dates_ttl(env, id, campaign.deadline);
    for key in [
        DataKey::Beneficiaries(id),
        DataKey::MilestoneStreamDays(id),
        DataKey::CampaignDonors(id),
        DataKey::CampaignPledges(id),
        DataKey::CampaignStart(id),
        DataKey::SlashedRecovery(id),
        DataKey::CancelVoteTotal(id),
    ] {
        bump_if_present(env, &key);
    }
    Ok(())
}

/// Stores a newly created campaign and extends its TTL.
pub fn create_campaign(env: &Env, campaign: &Campaign) {
    let key = DataKey::Campaign(campaign.id);
    env.storage().persistent().set(&key, campaign);
    bump(env, &key);
}

/// Writes back a campaign loaded with [`campaign`] earlier in the same call.
/// That read already extended the entry's TTL and a write does not shorten
/// it, so extending again here would only cost another host call (#186).
pub fn save_campaign(env: &Env, campaign: &Campaign) {
    env.storage()
        .persistent()
        .set(&DataKey::Campaign(campaign.id), campaign);
}

pub fn verifier_stats(env: &Env, who: &Address) -> VerifierStats {
    let key = DataKey::VerifierStats(who.clone());
    match env.storage().persistent().get(&key) {
        Some(stats) => {
            bump(env, &key);
            stats
        }
        None => VerifierStats {
            campaigns_assigned: 0,
            milestones_approved: 0,
            campaigns_completed: 0,
            campaigns_cancelled: 0,
            total_slashed: 0,
        },
    }
}

pub fn save_verifier_stats(env: &Env, who: &Address, stats: &VerifierStats) {
    let key = DataKey::VerifierStats(who.clone());
    env.storage().persistent().set(&key, stats);
    bump(env, &key);
}

/// Version of the stored data layout. A deployment that predates versioning
/// has no entry and reads as 0, so it stays pending until `migrate` runs.
pub fn schema_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::SchemaVersion)
        .unwrap_or(0)
}

pub fn set_schema_version(env: &Env, version: u32) {
    env.storage()
        .instance()
        .set(&DataKey::SchemaVersion, &version);
}

pub fn beneficiaries(env: &Env, campaign_id: u64) -> Option<Vec<Beneficiary>> {
    let key = DataKey::Beneficiaries(campaign_id);
    let list = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(list)
}

pub fn set_beneficiaries(env: &Env, campaign_id: u64, list: &Vec<Beneficiary>) {
    let key = DataKey::Beneficiaries(campaign_id);
    env.storage().persistent().set(&key, list);
    bump(env, &key);
}

pub fn contribution(env: &Env, id: u64, donor: &Address) -> i128 {
    let key = DataKey::Contribution(id, donor.clone());
    let amount: Option<i128> = env.storage().persistent().get(&key);
    if amount.is_some_and(|a| a > 0) {
        bump(env, &key);
    }
    amount.unwrap_or(0)
}

pub fn set_contribution(env: &Env, id: u64, donor: &Address, amount: i128) {
    let key = DataKey::Contribution(id, donor.clone());
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        bump(env, &key);
    }
}

pub fn unmatched_donation(env: &Env, id: u64, donor: &Address) -> i128 {
    let key = DataKey::UnmatchedDonation(id, donor.clone());
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn set_unmatched_donation(env: &Env, id: u64, donor: &Address, amount: i128) {
    let key = DataKey::UnmatchedDonation(id, donor.clone());
    if amount <= 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &amount);
        bump(env, &key);
    }
}

pub fn stretch_count(env: &Env, id: u64) -> u32 {
    let key = DataKey::StretchCount(id);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn set_stretch_count(env: &Env, id: u64, count: u32) {
    let key = DataKey::StretchCount(id);
    env.storage().persistent().set(&key, &count);
    bump(env, &key);
}

pub fn next_action_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::NextActionId)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::NextActionId, &(id + 1));
    id
}

pub fn queued_action(env: &Env, id: u64) -> Option<QueuedAction> {
    let key = DataKey::QueuedAction(id);
    let queued = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(queued)
}

pub fn save_queued_action(env: &Env, id: u64, queued: &QueuedAction) {
    let key = DataKey::QueuedAction(id);
    env.storage().persistent().set(&key, queued);
    bump(env, &key);
}

pub fn remove_queued_action(env: &Env, id: u64) {
    env.storage()
        .persistent()
        .remove(&DataKey::QueuedAction(id));
}

pub fn slashed_recovery(env: &Env, id: u64) -> i128 {
    let key = DataKey::SlashedRecovery(id);
    env.storage().persistent().get(&key).unwrap_or(0)
}

pub fn set_slashed_recovery(env: &Env, id: u64, amount: i128) {
    let key = DataKey::SlashedRecovery(id);
    env.storage().persistent().set(&key, &amount);
    bump(env, &key);
}

pub fn campaign_donors(env: &Env, campaign_id: u64) -> Vec<Address> {
    let key = DataKey::CampaignDonors(campaign_id);
    if let Some(donors) = env.storage().persistent().get(&key) {
        bump(env, &key);
        donors
    } else {
        Vec::new(env)
    }
}

pub fn add_campaign_donor(env: &Env, campaign_id: u64, donor: &Address) {
    let key = DataKey::CampaignDonors(campaign_id);
    let mut donors: Vec<Address> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    if !donors.contains(donor) {
        donors.push_back(donor.clone());
        env.storage().persistent().set(&key, &donors);
        bump(env, &key);
    }
}

// ─── Issue #27: Sponsor matching pool storage ────────────────────────────────

pub fn sponsor_pool(env: &Env, campaign_id: u64, sponsor: &Address) -> Option<SponsorPool> {
    let key = DataKey::SponsorPool(campaign_id, sponsor.clone());
    let pool = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(pool)
}

pub fn save_sponsor_pool(env: &Env, pool: &SponsorPool) {
    let key = DataKey::SponsorPool(pool.campaign_id, pool.sponsor.clone());
    env.storage().persistent().set(&key, pool);
    bump(env, &key);
}

pub fn remove_sponsor_pool(env: &Env, campaign_id: u64, sponsor: &Address) {
    let key = DataKey::SponsorPool(campaign_id, sponsor.clone());
    env.storage().persistent().remove(&key);
}

// ─── Issue #29: Verifier bond storage ────────────────────────────────────────

/// Required bond amount. 0 = no bond required (default).
pub fn bond_requirement(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::BondRequirement)
        .unwrap_or(0)
}

pub fn set_bond_requirement(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&DataKey::BondRequirement, &amount);
}

/// Withdrawal delay in seconds after deregistration. 0 = no delay (default).
pub fn bond_withdraw_delay(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::BondWithdrawDelay)
        .unwrap_or(0)
}

pub fn set_bond_withdraw_delay(env: &Env, delay: u64) {
    env.storage()
        .instance()
        .set(&DataKey::BondWithdrawDelay, &delay);
}

pub fn verifier_bond(env: &Env, verifier: &Address) -> Option<VerifierBond> {
    let key = DataKey::VerifierBond(verifier.clone());
    let bond = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(bond)
}

pub fn save_verifier_bond(env: &Env, bond: &VerifierBond) {
    let key = DataKey::VerifierBond(bond.verifier.clone());
    env.storage().persistent().set(&key, bond);
    bump(env, &key);
}

pub fn verifier_bond_nonce(env: &Env, verifier: &Address) -> u32 {
    let key = DataKey::VerifierBondNonce(verifier.clone());
    let nonce: Option<u32> = env.storage().persistent().get(&key);
    if nonce.is_some() {
        bump(env, &key);
    }
    nonce.unwrap_or(0)
}

pub fn archive_verifier_bond(env: &Env, verifier: &Address, bond: &VerifierBond) {
    let nonce = verifier_bond_nonce(env, verifier);
    let hist_key = DataKey::HistoricalVerifierBond(verifier.clone(), nonce);
    env.storage().persistent().set(&hist_key, bond);
    bump(env, &hist_key);

    let nonce_key = DataKey::VerifierBondNonce(verifier.clone());
    env.storage().persistent().set(&nonce_key, &(nonce + 1));
    bump(env, &nonce_key);
}

pub fn historical_verifier_bond(env: &Env, verifier: &Address, nonce: u32) -> Option<VerifierBond> {
    let key = DataKey::HistoricalVerifierBond(verifier.clone(), nonce);
    let bond = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(bond)
}

// ─── Issue #30: Pledge storage ────────────────────────────────────────────────

pub fn next_pledge_id(env: &Env) -> u64 {
    let id: u64 = env
        .storage()
        .instance()
        .get(&DataKey::PledgeCount)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey::PledgeCount, &(id + 1));
    id
}

/// Ids of the pledges made to `campaign_id`, so settling one campaign never
/// reads another campaign's pledges (Issue #125).
pub fn active_campaigns(env: &Env, who: &Address) -> u32 {
    let key = DataKey::VerifierActiveCampaigns(who.clone());
    let count: Option<u32> = env.storage().persistent().get(&key);
    if count.is_some() {
        bump(env, &key);
    }
    count.unwrap_or(0)
}

pub fn set_active_campaigns(env: &Env, who: &Address, count: u32) {
    let key = DataKey::VerifierActiveCampaigns(who.clone());
    if count == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &count);
        bump(env, &key);
    }
}

pub fn milestone_stream_days(env: &Env, campaign_id: u64) -> Option<Vec<u32>> {
    let key = DataKey::MilestoneStreamDays(campaign_id);
    let days = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(days)
}

pub fn set_milestone_stream_days(env: &Env, campaign_id: u64, days: &Vec<u32>) {
    let key = DataKey::MilestoneStreamDays(campaign_id);
    env.storage().persistent().set(&key, days);
    bump(env, &key);
}

pub fn stream(env: &Env, campaign_id: u64, milestone_index: u32) -> Option<Stream> {
    let key = DataKey::Stream(campaign_id, milestone_index);
    let stream = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(stream)
}

pub fn save_stream(env: &Env, campaign_id: u64, milestone_index: u32, stream: &Stream) {
    let key = DataKey::Stream(campaign_id, milestone_index);
    env.storage().persistent().set(&key, stream);
    bump(env, &key);
}

pub fn campaign_start(env: &Env, campaign_id: u64) -> u64 {
    let key = DataKey::CampaignStart(campaign_id);
    let start: Option<u64> = env.storage().persistent().get(&key);
    if start.is_some() {
        bump(env, &key);
    }
    start.unwrap_or(0)
}

pub fn set_campaign_start(env: &Env, campaign_id: u64, timestamp: u64) {
    let key = DataKey::CampaignStart(campaign_id);
    env.storage().persistent().set(&key, &timestamp);
    bump(env, &key);
}

pub fn cancel_vote(env: &Env, campaign_id: u64, donor: &Address) -> i128 {
    let key = DataKey::CancelVote(campaign_id, donor.clone());
    let weight: Option<i128> = env.storage().persistent().get(&key);
    if weight.is_some() {
        bump(env, &key);
    }
    weight.unwrap_or(0)
}

pub fn set_cancel_vote(env: &Env, campaign_id: u64, donor: &Address, weight: i128) {
    let key = DataKey::CancelVote(campaign_id, donor.clone());
    if weight == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &weight);
        bump(env, &key);
    }
}

pub fn cancel_vote_total(env: &Env, campaign_id: u64) -> i128 {
    let key = DataKey::CancelVoteTotal(campaign_id);
    let total: Option<i128> = env.storage().persistent().get(&key);
    if total.is_some() {
        bump(env, &key);
    }
    total.unwrap_or(0)
}

pub fn set_cancel_vote_total(env: &Env, campaign_id: u64, total: i128) {
    let key = DataKey::CancelVoteTotal(campaign_id);
    env.storage().persistent().set(&key, &total);
    bump(env, &key);
}

pub fn campaign_pledges(env: &Env, campaign_id: u64) -> Vec<u64> {
    let key = DataKey::CampaignPledges(campaign_id);
    match env.storage().persistent().get(&key) {
        Some(ids) => {
            bump(env, &key);
            ids
        }
        None => Vec::new(env),
    }
}

pub fn save_campaign_pledges(env: &Env, campaign_id: u64, ids: &Vec<u64>) {
    let key = DataKey::CampaignPledges(campaign_id);
    env.storage().persistent().set(&key, ids);
    bump(env, &key);
}

pub fn pledge(env: &Env, pledge_id: u64) -> Option<Pledge> {
    let key = DataKey::Pledge(pledge_id);
    let p = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(p)
}

pub fn save_pledge(env: &Env, p: &Pledge) {
    let key = DataKey::Pledge(p.pledge_id);
    env.storage().persistent().set(&key, p);
    bump(env, &key);
}

// ─── Issue #26: Per-milestone due dates storage ──────────────────────────────

/// Due dates of `campaign_id`. Reading them keeps them alive until past
/// `deadline`, so an overdue milestone is still detected, and refunds still
/// open, on a campaign that runs longer than the usual 120-day TTL (#184).
pub fn milestone_due_dates(
    env: &Env,
    campaign_id: u64,
    deadline: u64,
) -> Option<soroban_sdk::Vec<u64>> {
    let key = DataKey::MilestoneDueDates(campaign_id);
    let dates = env.storage().persistent().get(&key)?;
    bump_until(env, &key, deadline);
    Some(dates)
}

/// Stores the due dates of `campaign_id` with a TTL that lasts past `deadline`.
pub fn set_milestone_due_dates(
    env: &Env,
    campaign_id: u64,
    due_dates: &soroban_sdk::Vec<u64>,
    deadline: u64,
) {
    let key = DataKey::MilestoneDueDates(campaign_id);
    env.storage().persistent().set(&key, due_dates);
    bump_until(env, &key, deadline);
}

/// Keeps the due dates of `campaign_id`, if it has any, alive past `deadline`.
pub fn extend_milestone_due_dates_ttl(env: &Env, campaign_id: u64, deadline: u64) {
    let key = DataKey::MilestoneDueDates(campaign_id);
    if env.storage().persistent().has(&key) {
        bump_until(env, &key, deadline);
    }
}

pub fn remove_milestone_due_dates(env: &Env, campaign_id: u64) {
    let key = DataKey::MilestoneDueDates(campaign_id);
    env.storage().persistent().remove(&key);
}

/// Removes persistent auxiliary records for an unfunded campaign on cancellation (Issue #169).
pub fn cleanup_unfunded_campaign(env: &Env, campaign_id: u64) {
    remove_milestone_due_dates(env, campaign_id);
    env.storage()
        .persistent()
        .remove(&DataKey::MilestoneStreamDays(campaign_id));
    env.storage()
        .persistent()
        .remove(&DataKey::CampaignStart(campaign_id));
}

// ─── Issue #9: Milestone approvals storage ───────────────────────────────────

pub fn milestone_approvals(
    env: &Env,
    campaign_id: u64,
    milestone_index: u32,
) -> soroban_sdk::Vec<Address> {
    let key = DataKey::MilestoneApprovals(campaign_id, milestone_index);
    if let Some(approvals) = env.storage().persistent().get(&key) {
        bump(env, &key);
        approvals
    } else {
        soroban_sdk::Vec::new(env)
    }
}

pub fn save_milestone_approvals(
    env: &Env,
    campaign_id: u64,
    milestone_index: u32,
    approvals: &soroban_sdk::Vec<Address>,
) {
    let key = DataKey::MilestoneApprovals(campaign_id, milestone_index);
    env.storage().persistent().set(&key, approvals);
    bump(env, &key);
}

pub fn milestone_approval_proofs(
    env: &Env,
    campaign_id: u64,
    milestone_index: u32,
) -> soroban_sdk::Vec<String> {
    let key = DataKey::MilestoneApprovalProofs(campaign_id, milestone_index);
    if let Some(proofs) = env.storage().persistent().get(&key) {
        bump(env, &key);
        proofs
    } else {
        soroban_sdk::Vec::new(env)
    }
}

pub fn save_milestone_approval_proofs(
    env: &Env,
    campaign_id: u64,
    milestone_index: u32,
    proofs: &soroban_sdk::Vec<String>,
) {
    let key = DataKey::MilestoneApprovalProofs(campaign_id, milestone_index);
    env.storage().persistent().set(&key, proofs);
    bump(env, &key);
}

// ─── Issue #10: Token allowlist storage ──────────────────────────────────────

pub fn is_token_allowed(env: &Env, token: &Address) -> bool {
    let key = DataKey::AllowedToken(token.clone());
    let allowed: Option<bool> = env.storage().persistent().get(&key);
    if allowed.is_some() {
        bump(env, &key);
    }
    allowed.unwrap_or(false)
}

pub fn set_token_allowed(env: &Env, token: &Address, allowed: bool) {
    let key = DataKey::AllowedToken(token.clone());
    if allowed {
        env.storage().persistent().set(&key, &true);
        bump(env, &key);
    } else {
        env.storage().persistent().remove(&key);
    }
}

// ─── Emergency pause switch ──────────────────────────────────────────────────

pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

pub fn set_paused(env: &Env, paused: bool) {
    env.storage().instance().set(&DataKey::Paused, &paused);
}

// ─── Platform fee and treasury ───────────────────────────────────────────────

pub fn platform_fee(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::PlatformFee)
        .unwrap_or(0)
}

pub fn set_platform_fee(env: &Env, fee_bps: u32) {
    env.storage()
        .instance()
        .set(&DataKey::PlatformFee, &fee_bps);
}

pub fn treasury(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::Treasury)
}

pub fn set_treasury(env: &Env, treasury: &Address) {
    env.storage().instance().set(&DataKey::Treasury, treasury);
}
