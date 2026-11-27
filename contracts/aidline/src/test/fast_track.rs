use super::*;

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
fn emergency_fast_track_capped_by_milestone_zero() {
    let s = Setup::new();
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://flood-relief"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 100, 400, 500],
    );
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    // 20% of goal is 200, but milestone 0 is only 100.
    assert_eq!(s.client.emergency_fast_track(&id), 100);
    assert_eq!(s.client.get_campaign(&id).released, 100);

    // Approving milestone 0 pays nothing more and keeps `released` intact.
    s.client.approve_milestone(&id, &s.proof());
    let c = s.client.get_campaign(&id);
    assert_eq!(c.released, 100);
    assert_eq!(c.milestones_released, 1);
    assert_eq!(s.token.balance(&s.beneficiary), 100);
    assert_eq!(s.token.balance(&s.client.address), 900);
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
        &s.token.address,
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

    s.client.remove_verifier(&s.admin, &s.verifier);
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

#[test]
fn emergency_fast_track_cap_overflow_returns_invalid_amount() {
    let s = Setup::new();
    let large_goal = i128::MAX / 1000;
    let milestones = vec![&s.env, large_goal];
    let id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://overflow-goal"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &milestones,
    );

    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &500);

    // Calling emergency_fast_track with an extreme goal overflows cap calculation and returns InvalidAmount
    assert_eq!(
        s.client.try_emergency_fast_track(&id),
        Err(Ok(Error::InvalidAmount))
    );
}
