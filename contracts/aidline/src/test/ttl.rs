// ─── Issue #110: persistent keys are extended ────────────────────────────────

use super::*;
use crate::storage::{INSTANCE_THRESHOLD, PERSISTENT_THRESHOLD};
use crate::types::DataKey;
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};

/// Remaining TTL, in ledgers, of a persistent key. Read through the test
/// environment; it panics if the key does not exist.
fn ttl(s: &Setup, key: &DataKey) -> u32 {
    s.env.as_contract(&s.client.address, || {
        s.env.storage().persistent().get_ttl(key)
    })
}

fn assert_extended(s: &Setup, key: DataKey) {
    let remaining = ttl(s, &key);
    assert!(
        remaining >= PERSISTENT_THRESHOLD,
        "{key:?} has only {remaining} ledgers left"
    );
}

/// A fresh entry in the test environment starts with a few thousand ledgers
/// of TTL, so only an explicit `extend_ttl` on the write path can lift it
/// to the threshold. Covers every persistent `DataKey` variant; the instance
/// keys (`Admin`, `Token`, `CampaignCount`, `Paused`, `PlatformFee`,
/// `Treasury`, `PendingAdmin`, `BondRequirement`, `BondWithdrawDelay`,
/// `PledgeCount`) live in the instance entry, checked separately below.
#[test]
fn every_persistent_key_is_extended_by_the_call_that_writes_it() {
    let s = Setup::new();
    let id = s.campaign();
    assert_extended(&s, DataKey::Campaign(id));
    assert_extended(&s, DataKey::Verifier(s.verifier.clone()));
    assert_extended(&s, DataKey::AllowedToken(s.token.address.clone()));

    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &300);
    assert_extended(&s, DataKey::Contribution(id, donor.clone()));
    assert_extended(&s, DataKey::CampaignDonors(id));

    let sponsor = s.donor(1_000_000);
    s.client
        .deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000);
    assert_extended(&s, DataKey::SponsorPool(id, sponsor.clone()));

    let pledger = s.donor(500);
    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&pledger, &s.client.address, &200, &expiry);
    let pledge_id = s.client.create_pledge(&pledger, &id, &200);
    assert_extended(&s, DataKey::Pledge(pledge_id));

    let bonded = s.bonded_verifier(1_000, 100);
    assert_extended(&s, DataKey::VerifierBond(bonded.clone()));
    s.client
        .set_verifier_profile(&bonded, &String::from_str(&s.env, "ipfs://profile"));
    assert_extended(&s, DataKey::VerifierProfile(bonded));

    let due_id = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://due"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
        &vec![
            &s.env,
            s.env.ledger().timestamp() + 10 * DAY,
            s.env.ledger().timestamp() + 20 * DAY,
            s.env.ledger().timestamp() + 25 * DAY,
        ],
    );
    assert_extended(&s, DataKey::MilestoneDueDates(due_id));

    s.client.donate(&donor, &id, &700);
    s.client.approve_milestone(&id, &s.proof());
    assert_extended(&s, DataKey::MilestoneApprovals(id, 0));
}

/// Reading an entry also keeps it alive: advance the ledger until the
/// entry is inside the extension window, read it through a view, and check
/// the TTL went back up. Only calls that do not touch the token contract
/// are made after the jump, because the token's own entries are not ours
/// to extend.
#[test]
fn reads_of_verifier_contribution_and_allowlist_extend_their_entries() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &300);

    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 10 * 17_280);

    let verifier_key = DataKey::Verifier(s.verifier.clone());
    let contribution_key = DataKey::Contribution(id, donor.clone());
    let allowed_key = DataKey::AllowedToken(s.token.address.clone());
    for key in [&verifier_key, &contribution_key, &allowed_key] {
        assert!(ttl(&s, key) < PERSISTENT_THRESHOLD, "{key:?} should be due");
    }

    assert!(s.client.is_verifier(&s.verifier));
    assert_eq!(s.client.contribution_of(&id, &donor), 300);
    assert!(s.client.is_token_allowed(&s.token.address));

    assert_extended(&s, verifier_key);
    assert_extended(&s, contribution_key);
    assert_extended(&s, allowed_key);
}

#[test]
fn is_verifier_extends_ttl_to_persistent_bump_and_ignores_unknown() {
    use crate::storage::PERSISTENT_BUMP;

    let s = Setup::new();
    let unknown = Address::generate(&s.env);

    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 10 * 17_280);

    let verifier_key = DataKey::Verifier(s.verifier.clone());
    let unknown_key = DataKey::Verifier(unknown.clone());
    assert!(ttl(&s, &verifier_key) < PERSISTENT_THRESHOLD);

    assert!(s.client.is_verifier(&s.verifier));
    assert_eq!(ttl(&s, &verifier_key), PERSISTENT_BUMP);

    assert!(!s.client.is_verifier(&unknown));
    let has_unknown = s.env.as_contract(&s.client.address, || {
        s.env.storage().persistent().has(&unknown_key)
    });
    assert!(!has_unknown);
}

#[test]
fn contribution_of_extends_ttl_to_persistent_bump_and_protects_long_running_refund() {
    use crate::storage::PERSISTENT_BUMP;

    let s = Setup::new();
    let day_in_ledgers = 17_280;
    let future = s.env.ledger().timestamp() + 400 * DAY;
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://climate-1yr"),
        &future,
        &vec![&s.env, 1_000],
    );

    let donor = s.donor(500);
    let non_donor = Address::generate(&s.env);
    s.client.donate(&donor, &id, &500);

    let contribution_key = DataKey::Contribution(id, donor.clone());
    let non_donor_key = DataKey::Contribution(id, non_donor.clone());

    // Advance to day 100 (within extension window, before 120-day expiry)
    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 100 * day_in_ledgers);
    assert!(ttl(&s, &contribution_key) < PERSISTENT_THRESHOLD);

    // Querying contribution_of refreshes the contribution TTL to PERSISTENT_BUMP
    assert_eq!(s.client.contribution_of(&id, &donor), 500);
    assert_eq!(ttl(&s, &contribution_key), PERSISTENT_BUMP);

    // Querying for a non-donor does not mutate storage
    assert_eq!(s.client.contribution_of(&id, &non_donor), 0);
    let has_non_donor = s.env.as_contract(&s.client.address, || {
        s.env.storage().persistent().has(&non_donor_key)
    });
    assert!(!has_non_donor);

    // Refresh campaign and advance past 120 days (to day 150) and past deadline
    s.client.extend_campaign_ttl(&id);
    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 50 * day_in_ledgers);
    s.env.ledger().set_timestamp(future + 1);

    // Contribution record survived past the original 120-day TTL
    assert_eq!(s.client.contribution_of(&id, &donor), 500);

    // Refund succeeds on the expired long-running campaign
    let refunded = s.client.refund(&donor, &id);
    assert_eq!(refunded, 500);
    assert_eq!(s.token.balance(&donor), 500);
}

/// The instance entry (admin, token, counters, fees) is extended by every
/// campaign call, not only by the constructor and `create_campaign`.
#[test]
fn campaign_calls_extend_the_instance_entry() {
    let s = Setup::new();
    let id = s.campaign();

    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 5 * 17_280);
    let before = s
        .env
        .as_contract(&s.client.address, || s.env.storage().instance().get_ttl());
    assert!(before < INSTANCE_THRESHOLD);

    s.client.get_campaign(&id);
    let after = s
        .env
        .as_contract(&s.client.address, || s.env.storage().instance().get_ttl());
    assert!(
        after >= INSTANCE_THRESHOLD,
        "instance left with {after} ledgers"
    );
}

const DAY_IN_LEDGERS: u32 = 17_280;

fn instance_ttl(s: &Setup) -> u32 {
    s.env
        .as_contract(&s.client.address, || s.env.storage().instance().get_ttl())
}

/// Moves the ledger forward until the instance entry is due for extension.
fn age_instance(s: &Setup) {
    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 2 * DAY_IN_LEDGERS);
    assert!(instance_ttl(s) < INSTANCE_THRESHOLD);
}

/// Issue #183: every state-changing entry point keeps the instance entry
/// (admin, token, counters, bond settings) alive, so a deployment that
/// keeps running but stops creating campaigns is never bricked by an
/// expired instance.
#[test]
fn state_changing_calls_extend_the_instance_entry() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let id2 = s.campaign();
    let donor2 = s.donor(100);
    s.client.donate(&donor2, &id2, &100);
    let pledger = s.donor(500);
    let expiry = s.env.ledger().sequence() + 100 * DAY_IN_LEDGERS;
    s.token.approve(&pledger, &s.client.address, &200, &expiry);
    let pledge_id = s.client.create_pledge(&pledger, &id, &200);
    let bonded = s.bonded_verifier(1_000, 100);
    let extra = Address::generate(&s.env);

    let steps: [(&str, &dyn Fn()); 18] = [
        ("cancel_pledge", &|| {
            s.client.cancel_pledge(&pledger, &pledge_id)
        }),
        ("donate", &|| {
            s.client.donate(&donor, &id, &1_000);
        }),
        ("approve_milestone", &|| {
            s.client.approve_milestone(&id, &s.proof());
        }),
        ("cancel_campaign", &|| {
            s.client.cancel_campaign(&s.creator, &id2)
        }),
        ("refund", &|| {
            s.client.refund(&donor2, &id2);
        }),
        ("add_verifier", &|| s.client.add_verifier(&s.admin, &extra)),
        ("set_verifier_profile", &|| {
            s.client
                .set_verifier_profile(&extra, &String::from_str(&s.env, "ipfs://p"))
        }),
        ("remove_verifier", &|| {
            s.client.remove_verifier(&s.admin, &extra)
        }),
        ("deregister_verifier", &|| {
            s.client.deregister_verifier(&bonded, &bonded)
        }),
        ("withdraw_bond", &|| {
            s.client.withdraw_bond(&bonded);
        }),
        ("set_bond_requirement", &|| {
            s.client.set_bond_requirement(&0)
        }),
        ("set_bond_withdraw_delay", &|| {
            s.client.set_bond_withdraw_delay(&0)
        }),
        ("set_platform_fee", &|| s.client.set_platform_fee(&0)),
        ("set_treasury", &|| s.client.set_treasury(&extra)),
        ("add_allowed_token", &|| s.client.add_allowed_token(&extra)),
        ("remove_allowed_token", &|| {
            s.client.remove_allowed_token(&extra)
        }),
        ("pause", &|| s.client.pause()),
        ("unpause", &|| s.client.unpause()),
    ];
    for (call, run) in steps {
        age_instance(&s);
        run();
        let remaining = instance_ttl(&s);
        assert!(
            remaining >= INSTANCE_THRESHOLD,
            "{call} left the instance with {remaining} ledgers"
        );
    }

    // Admin hand-over last, since it changes who the admin is.
    age_instance(&s);
    s.client.nominate_admin(&extra);
    assert!(instance_ttl(&s) >= INSTANCE_THRESHOLD, "nominate_admin");
    age_instance(&s);
    s.client.propose_admin(&extra);
    assert!(instance_ttl(&s) >= INSTANCE_THRESHOLD, "propose_admin");
    age_instance(&s);
    s.client.accept_admin();
    assert!(instance_ttl(&s) >= INSTANCE_THRESHOLD, "accept_admin");
    age_instance(&s);
    s.client.set_admin(&s.admin);
    assert!(instance_ttl(&s) >= INSTANCE_THRESHOLD, "set_admin");
}

/// Campaign with milestone due dates spread evenly up to a deadline
/// `days` from now.
fn due_dates_campaign(s: &Setup, days: u64) -> u64 {
    let now = s.env.ledger().timestamp();
    s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://climate"),
        &(now + days * DAY),
        &vec![&s.env, 300, 300, 400],
        &vec![
            &s.env,
            now + days * DAY / 3,
            now + 2 * days * DAY / 3,
            now + days * DAY,
        ],
    )
}

fn ledgers(days: u64) -> u32 {
    days as u32 * DAY_IN_LEDGERS
}

/// Issue #184: due dates outlive the deadline, so a missed milestone still
/// opens refunds on a campaign longer than the default 120-day TTL.
#[test]
fn milestone_due_dates_stay_alive_until_past_the_deadline() {
    let s = Setup::new();
    let short = due_dates_campaign(&s, 90);
    assert!(ttl(&s, &DataKey::MilestoneDueDates(short)) > ledgers(90));

    let long = due_dates_campaign(&s, 200);
    let key = DataKey::MilestoneDueDates(long);
    let remaining = ttl(&s, &key);
    assert!(
        remaining > ledgers(200),
        "due dates expire after {remaining} ledgers, before the 200-day deadline"
    );

    // Reading them on the way to the deadline keeps that guarantee and
    // does not push the expiry any further out.
    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + ledgers(50));
    s.env
        .ledger()
        .set_timestamp(s.env.ledger().timestamp() + 50 * DAY);
    let before = ttl(&s, &key);
    assert_eq!(s.client.milestone_due_dates(&long).len(), 3);
    assert_eq!(ttl(&s, &key), before);
    assert!(before > ledgers(150));

    // Extending the deadline extends the due dates with it.
    let new_deadline = s.client.get_campaign(&long).deadline + 30 * DAY;
    s.client.extend_deadline(&long, &new_deadline);
    assert!(ttl(&s, &key) > ledgers(180));
}

/// The deadline-based TTL never exceeds the network's maximum.
#[test]
fn milestone_due_dates_ttl_is_capped_at_the_network_maximum() {
    let s = Setup::new();
    let id = due_dates_campaign(&s, 3 * 365);
    let max = s
        .env
        .as_contract(&s.client.address, || s.env.storage().max_ttl());
    assert_eq!(ttl(&s, &DataKey::MilestoneDueDates(id)), max);
}

/// Issue #185: anyone can keep a quiet campaign alive without any
/// authorization, and the call renews the campaign, its due dates, its
/// donor list and the instance entry.
#[test]
fn bump_campaign_ttl_is_permissionless_and_renews_the_campaign() {
    let s = Setup::new();
    let id = due_dates_campaign(&s, 200);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &300);

    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + ledgers(10));
    let campaign_key = DataKey::Campaign(id);
    let donors_key = DataKey::CampaignDonors(id);
    for key in [&campaign_key, &donors_key] {
        assert!(ttl(&s, key) < PERSISTENT_THRESHOLD, "{key:?} should be due");
    }
    assert!(instance_ttl(&s) < INSTANCE_THRESHOLD);

    s.client.bump_campaign_ttl(&id);
    assert!(s.env.auths().is_empty(), "no authorization is required");

    assert_extended(&s, campaign_key);
    assert_extended(&s, donors_key);
    assert!(ttl(&s, &DataKey::MilestoneDueDates(id)) > ledgers(190));
    assert!(instance_ttl(&s) >= INSTANCE_THRESHOLD);
}

#[test]
fn bump_campaign_ttl_rejects_an_unknown_campaign() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_bump_campaign_ttl(&42),
        Err(Ok(Error::CampaignNotFound))
    );
}

/// A keeper that keeps calling over two years holds the campaign entry
/// past its original 120-day lifetime.
#[test]
fn repeated_bumps_keep_a_two_year_campaign_alive() {
    let s = Setup::new();
    let id = due_dates_campaign(&s, 2 * 365);
    for _ in 0..8 {
        s.env
            .ledger()
            .set_sequence_number(s.env.ledger().sequence() + ledgers(100));
        s.client.bump_campaign_ttl(&id);
    }
    assert_eq!(s.client.get_campaign(&id).id, id);
}
