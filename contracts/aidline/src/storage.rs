use soroban_sdk::{Address, Env};

use crate::errors::Error;
use crate::types::{Campaign, DataKey, Pledge, SponsorPool, VerifierBond};

const DAY_IN_LEDGERS: u32 = 17_280;
const INSTANCE_BUMP: u32 = 30 * DAY_IN_LEDGERS;
const INSTANCE_THRESHOLD: u32 = INSTANCE_BUMP - DAY_IN_LEDGERS;
const PERSISTENT_BUMP: u32 = 120 * DAY_IN_LEDGERS;
const PERSISTENT_THRESHOLD: u32 = PERSISTENT_BUMP - 7 * DAY_IN_LEDGERS;

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

pub fn admin(env: &Env) -> Address {
    env.storage().instance().get(&DataKey::Admin).unwrap()
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
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
    env.storage().persistent().get(&key).unwrap_or(false)
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

pub fn campaign(env: &Env, id: u64) -> Result<Campaign, Error> {
    let key = DataKey::Campaign(id);
    let campaign = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::CampaignNotFound)?;
    bump(env, &key);
    Ok(campaign)
}

pub fn save_campaign(env: &Env, campaign: &Campaign) {
    let key = DataKey::Campaign(campaign.id);
    env.storage().persistent().set(&key, campaign);
    bump(env, &key);
}

pub fn contribution(env: &Env, id: u64, donor: &Address) -> i128 {
    let key = DataKey::Contribution(id, donor.clone());
    env.storage().persistent().get(&key).unwrap_or(0)
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

pub fn next_pledge_id_peek(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::PledgeCount)
        .unwrap_or(0)
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

pub fn milestone_due_dates(env: &Env, campaign_id: u64) -> Option<soroban_sdk::Vec<u64>> {
    let key = DataKey::MilestoneDueDates(campaign_id);
    let dates = env.storage().persistent().get(&key)?;
    bump(env, &key);
    Some(dates)
}

pub fn set_milestone_due_dates(env: &Env, campaign_id: u64, due_dates: &soroban_sdk::Vec<u64>) {
    let key = DataKey::MilestoneDueDates(campaign_id);
    env.storage().persistent().set(&key, due_dates);
    bump(env, &key);
}
