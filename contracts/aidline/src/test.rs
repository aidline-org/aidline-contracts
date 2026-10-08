#![cfg(test)]

use soroban_sdk::{
    Address, Env, String,
    testutils::{Address as _, Ledger},
    token::{StellarAssetClient, TokenClient},
    vec,
};

use crate::{Aidline, AidlineClient, CampaignKind, CampaignStatus, Error};

const DAY: u64 = 86_400;

struct Setup<'a> {
    env: Env,
    client: AidlineClient<'a>,
    token: TokenClient<'a>,
    admin: Address,
    verifier: Address,
    creator: Address,
    beneficiary: Address,
}

impl<'a> Setup<'a> {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let token = TokenClient::new(&env, &sac.address());

        let contract_id = env.register(Aidline, (admin.clone(), sac.address()));
        let client = AidlineClient::new(&env, &contract_id);

        let verifier = Address::generate(&env);
        client.add_verifier(&verifier);

        Setup {
            creator: Address::generate(&env),
            beneficiary: Address::generate(&env),
            env,
            client,
            token,
            admin,
            verifier,
        }
    }

    fn donor(&self, balance: i128) -> Address {
        let donor = Address::generate(&self.env);
        StellarAssetClient::new(&self.env, &self.token.address).mint(&donor, &balance);
        donor
    }

    /// Campaign with three milestones of 300, 300 and 400 (goal 1000).
    fn campaign(&self) -> u64 {
        self.client.create_campaign(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://flood-relief"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
        )
    }

    fn proof(&self) -> String {
        String::from_str(&self.env, "ipfs://proof")
    }

    fn pass_deadline(&self) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + 31 * DAY);
    }
}

#[test]
fn creates_campaign_with_goal_from_milestones() {
    let s = Setup::new();
    let id = s.campaign();

    let c = s.client.get_campaign(&id);
    assert_eq!(id, 0);
    assert_eq!(c.goal, 1000);
    assert_eq!(c.status, CampaignStatus::Active);
    assert_eq!(c.kind, CampaignKind::Emergency);
    assert_eq!(s.client.campaign_count(), 1);
}

#[test]
fn rejects_unknown_verifier() {
    let s = Setup::new();
    let stranger = Address::generate(&s.env);
    let res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &stranger,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://trees"),
        &(s.env.ledger().timestamp() + DAY),
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::NotVerifier)));
}

#[test]
fn rejects_bad_milestones_and_deadline() {
    let s = Setup::new();
    let uri = String::from_str(&s.env, "ipfs://x");
    let future = s.env.ledger().timestamp() + DAY;

    let empty = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env],
    );
    assert_eq!(empty, Err(Ok(Error::InvalidMilestones)));

    let negative = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &CampaignKind::Climate,
        &uri,
        &future,
        &vec![&s.env, 100, -5],
    );
    assert_eq!(negative, Err(Ok(Error::InvalidMilestones)));

    let past = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &CampaignKind::Climate,
        &uri,
        &s.env.ledger().timestamp(),
        &vec![&s.env, 100],
    );
    assert_eq!(past, Err(Ok(Error::DeadlineInPast)));
}

#[test]
fn donations_are_held_in_escrow() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);

    s.client.donate(&donor, &id, &200);
    s.client.donate(&donor, &id, &100);

    assert_eq!(s.token.balance(&donor), 200);
    assert_eq!(s.token.balance(&s.client.address), 300);
    assert_eq!(s.client.contribution_of(&id, &donor), 300);
    assert_eq!(s.client.get_campaign(&id).raised, 300);
}

#[test]
fn donation_cannot_exceed_goal() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(2000);

    assert_eq!(
        s.client.try_donate(&donor, &id, &1001),
        Err(Ok(Error::GoalExceeded))
    );
    assert_eq!(
        s.client.try_donate(&donor, &id, &0),
        Err(Ok(Error::InvalidAmount))
    );
}

#[test]
fn milestones_release_in_order_and_complete() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 300);
    assert_eq!(s.client.approve_milestone(&id, &s.proof()), 400);

    let c = s.client.get_campaign(&id);
    assert_eq!(s.token.balance(&s.beneficiary), 1000);
    assert_eq!(c.released, 1000);
    assert_eq!(c.status, CampaignStatus::Completed);
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn milestone_needs_enough_funds() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &299);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::MilestoneNotFunded))
    );
}

#[test]
fn removed_verifier_cannot_approve() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);
    s.client.remove_verifier(&s.verifier);

    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn cancelled_campaign_refunds_unreleased_pro_rata() {
    let s = Setup::new();
    let id = s.campaign();
    let alice = s.donor(1000);
    let bob = s.donor(1000);
    s.client.donate(&alice, &id, &600);
    s.client.donate(&bob, &id, &400);
    s.client.approve_milestone(&id, &s.proof()); // 300 released, 700 left

    s.client.cancel_campaign(&s.creator, &id);

    assert_eq!(s.client.refund(&alice, &id), 420);
    assert_eq!(s.client.refund(&bob, &id), 280);
    assert_eq!(s.token.balance(&s.client.address), 0);
    assert_eq!(
        s.client.try_refund(&alice, &id),
        Err(Ok(Error::NothingToRefund))
    );
}

#[test]
fn expired_campaign_allows_refund_and_blocks_activity() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &500);

    assert_eq!(
        s.client.try_refund(&donor, &id),
        Err(Ok(Error::RefundNotAvailable))
    );

    s.pass_deadline();
    assert_eq!(
        s.client.try_donate(&donor, &id, &10),
        Err(Ok(Error::CampaignExpired))
    );
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignExpired))
    );
    assert_eq!(s.client.refund(&donor, &id), 500);
    assert_eq!(s.token.balance(&donor), 1000);
}

#[test]
fn only_creator_or_admin_can_cancel() {
    let s = Setup::new();
    let id = s.campaign();
    let stranger = Address::generate(&s.env);

    assert_eq!(
        s.client.try_cancel_campaign(&stranger, &id),
        Err(Ok(Error::Unauthorized))
    );
    s.client.cancel_campaign(&s.admin, &id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);
}

#[test]
fn missing_campaign_errors() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_get_campaign(&42),
        Err(Ok(Error::CampaignNotFound))
    );
}

// ─── Issue #23 Tests: Emergency fast track ────────────────────────────────────

#[test]
fn emergency_fast_track_works_for_emergency_campaign() {
    let s = Setup::new();
    let id = s.campaign(); // Goal 1000
    let donor = s.donor(500);
    
    // Donate 300. Cap is 20% of 1000 = 200.
    s.client.donate(&donor, &id, &300);

    assert_eq!(s.client.emergency_fast_track(&id), 200);
    assert_eq!(s.token.balance(&s.beneficiary), 200);

    let c = s.client.get_campaign(&id);
    assert_eq!(c.emergency_advance, 200);
    assert_eq!(c.released, 200);
}

#[test]
fn emergency_fast_track_limited_by_escrow() {
    let s = Setup::new();
    let id = s.campaign(); // Goal 1000
    let donor = s.donor(500);
    
    // Donate 100. Cap is 200, but only 100 escrowed.
    s.client.donate(&donor, &id, &100);

    assert_eq!(s.client.emergency_fast_track(&id), 100);
    assert_eq!(s.token.balance(&s.beneficiary), 100);
}

#[test]
fn emergency_fast_track_cannot_exceed_cap() {
    let s = Setup::new();
    let id = s.campaign(); // Goal 1000
    let donor = s.donor(1000);
    
    // Donate 1000. Cap is 200.
    s.client.donate(&donor, &id, &1000);

    assert_eq!(s.client.emergency_fast_track(&id), 200);
    
    // Try again -> already taken
    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::AdvanceAlreadyTaken))
    );
}

#[test]
fn emergency_fast_track_fails_for_non_emergency() {
    let s = Setup::new();
    let donor = s.donor(1000);

    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://trees"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 1000],
    );

    s.client.donate(&donor, &id, &1000);

    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::NotEmergencyCampaign))
    );
}

#[test]
fn emergency_fast_track_authorization() {
    let s = Setup::new();
    let id = s.campaign();
    let stranger = s.donor(1000);

    s.client.remove_verifier(&s.verifier);
    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn emergency_fast_track_accounting_with_milestone_one() {
    let s = Setup::new();
    let id = s.campaign(); // milestones: 300, 300, 400
    let donor = s.donor(1000);
    
    s.client.donate(&donor, &id, &1000);

    // Fast track takes 200
    s.client.emergency_fast_track(&id);
    assert_eq!(s.token.balance(&s.beneficiary), 200);

    // Approving milestone one (scheduled 300) should only release remaining 100
    let amt = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(amt, 300); // returns scheduled amount
    assert_eq!(s.token.balance(&s.beneficiary), 300); // 200 + 100
    
    let c = s.client.get_campaign(&id);
    assert_eq!(c.released, 300); // total released so far
    assert_eq!(c.emergency_advance, 200);
    assert_eq!(c.milestones_released, 1);
}

#[test]
fn emergency_fast_track_requires_escrow() {
    let s = Setup::new();
    let id = s.campaign();
    
    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::AdvanceExceedsEscrow))
    );
}

// ─── Issue #24 Tests: Resource Cost Measurements ──────────────────────────────

#[test]
fn test_measure_resource_costs() {
    let s = Setup::new();
    let env = &s.env;
    let donor = s.donor(5000);
    
    env.budget().reset_default();
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://cost-test"),
        &(env.ledger().timestamp() + 30 * DAY),
        &vec![env, 1000, 1000],
    );
    
    env.budget().reset_default();
    s.client.donate(&donor, &id, &500);
    
    env.budget().reset_default();
    s.client.emergency_fast_track(&id);
    
    env.budget().reset_default();
    s.client.approve_milestone(&id, &s.proof());
    
    env.budget().reset_default();
    s.client.cancel_campaign(&s.creator, &id);
    
    env.budget().reset_default();
    s.client.refund(&donor, &id);
}

// ─── Issue #25 Tests: Assert Exact Events ─────────────────────────────────────

#[test]
fn test_exact_events_emitted() {
    let s = Setup::new();
    let env = &s.env;
    let donor = s.donor(1000);
    
    let deadline = env.ledger().timestamp() + 30 * DAY;
    let uri = String::from_str(env, "ipfs://events-test");
    let milestones = vec![env, 300, 700];

    s.client.add_verifier(&donor);
    let events = env.events().all();
    let verifier_updated_event = events.last().unwrap();
    assert_eq!(
        verifier_updated_event,
        (
            s.client.address.clone(),
            soroban_sdk::vec![
                env,
                soroban_sdk::Symbol::new(env, "VerifierUpdated").into_val(env),
                donor.into_val(env)
            ],
            true.into_val(env) // active: bool
        )
    );

    env.events().all().clear();

    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &CampaignKind::Emergency,
        &uri,
        &deadline,
        &milestones,
    );
    let events = env.events().all();
    let campaign_created_event = events.last().unwrap();
    assert_eq!(
        campaign_created_event,
        (
            s.client.address.clone(),
            soroban_sdk::vec![
                env,
                soroban_sdk::Symbol::new(env, "CampaignCreated").into_val(env),
                id.into_val(env)
            ],
            (s.creator.clone(), CampaignKind::Emergency, 1000_i128, deadline).into_val(env)
        )
    );

    s.client.donate(&donor, &id, &500);
    let events = env.events().all();
    let donated_event = events.last().unwrap();
    assert_eq!(
        donated_event,
        (
            s.client.address.clone(),
            soroban_sdk::vec![
                env,
                soroban_sdk::Symbol::new(env, "Donated").into_val(env),
                id.into_val(env),
                donor.into_val(env)
            ],
            500_i128.into_val(env)
        )
    );

    s.client.emergency_fast_track(&id);
    let events = env.events().all();
    let emergency_event = events.last().unwrap();
    assert_eq!(
        emergency_event,
        (
            s.client.address.clone(),
            soroban_sdk::vec![
                env,
                soroban_sdk::Symbol::new(env, "EmergencyAdvanceReleased").into_val(env),
                id.into_val(env),
                s.verifier.into_val(env)
            ],
            (0_u32, 200_i128).into_val(env) // milestone_index, amount
        )
    );

    s.client.approve_milestone(&id, &s.proof());
    let events = env.events().all();
    let milestone_event = events.last().unwrap();
    assert_eq!(
        milestone_event,
        (
            s.client.address.clone(),
            soroban_sdk::vec![
                env,
                soroban_sdk::Symbol::new(env, "MilestoneReleased").into_val(env),
                id.into_val(env)
            ],
            (0_u32, 300_i128, s.proof()).into_val(env) // index, amount, proof_uri
        )
    );

    s.client.cancel_campaign(&s.creator, &id);
    let events = env.events().all();
    let cancelled_event = events.last().unwrap();
    assert_eq!(
        cancelled_event,
        (
            s.client.address.clone(),
            soroban_sdk::vec![
                env,
                soroban_sdk::Symbol::new(env, "CampaignCancelled").into_val(env),
                id.into_val(env)
            ],
            ().into_val(env) // no data fields
        )
    );

    s.client.refund(&donor, &id);
    let events = env.events().all();
    let refunded_event = events.last().unwrap();
    assert_eq!(
        refunded_event,
        (
            s.client.address.clone(),
            soroban_sdk::vec![
                env,
                soroban_sdk::Symbol::new(env, "Refunded").into_val(env),
                id.into_val(env),
                donor.into_val(env)
            ],
            350_i128.into_val(env) // amount refunded
        )
    );
}

// ─── Issue #26 Tests: Per-milestone due dates ────────────────────────────────

#[test]
fn due_date_refunds_do_not_cancel_campaign() {
    let s = Setup::new();
    let env = &s.env;
    
    let now = env.ledger().timestamp();
    let due_dates = vec![env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![env, 300, 700];
    
    let id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://due-dates"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );
    
    let donor_a = s.donor(500);
    let donor_b = s.donor(500);
    
    s.client.donate(&donor_a, &id, &500);
    s.client.donate(&donor_b, &id, &500);
    
    s.client.approve_milestone(&id, &s.proof()); // 300 released
    
    env.ledger().set_timestamp(now + 21 * DAY);
    
    let refund_a = s.client.refund(&donor_a, &id);
    assert_eq!(refund_a, 350);
    
    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Active);
    assert_eq!(c.raised, 500);
    assert_eq!(c.released, 150);
    
    let refund_b = s.client.refund(&donor_b, &id);
    assert_eq!(refund_b, 350);
    
    let c2 = s.client.get_campaign(&id);
    assert_eq!(c2.raised, 0);
    assert_eq!(c2.released, 0);
}
