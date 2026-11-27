use super::*;

// ─── Issue #4 Tests: Verifier reassignment ───────────────────────────────────

#[test]
fn admin_can_reassign_verifier_on_active_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &new_v);

    // reassign_verifier emits VerifierReassigned; events() returns events from
    // the most recent invocation, must be non-empty after the call.
    s.client.reassign_verifier(&id, &new_v);
    assert!(
        !s.env.events().all().events().is_empty(),
        "VerifierReassigned event not emitted"
    );

    let c = s.client.get_campaign(&id);
    assert_eq!(c.verifier, new_v);
}

#[test]
fn reassign_fails_for_unregistered_verifier() {
    let s = Setup::new();
    let id = s.campaign();
    let stranger = Address::generate(&s.env);

    assert_eq!(
        s.client.try_reassign_verifier(&id, &stranger),
        Err(Ok(Error::NotVerifier))
    );
}

#[test]
fn reassign_fails_for_cancelled_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &new_v);

    s.client.cancel_campaign(&s.admin, &id);
    assert_eq!(
        s.client.try_reassign_verifier(&id, &new_v),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn reassign_fails_for_expired_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &new_v);

    s.pass_deadline();
    assert_eq!(
        s.client.try_reassign_verifier(&id, &new_v),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn new_verifier_can_approve_after_reassignment() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Remove old verifier and reassign to a new one.
    s.client.remove_verifier(&s.admin, &s.verifier);
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &new_v);
    s.client.reassign_verifier(&id, &new_v);

    // New verifier controls the campaign; milestone should release.
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);
    assert_eq!(s.client.get_campaign(&id).verifier, new_v);
}

#[test]
fn reassign_verifier_caps_at_maximum() {
    let s = Setup::new();
    let id = s.campaign();

    // Initial reassignment count is 0
    assert_eq!(s.client.verifier_reassignments(&id), 0);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 0);

    let v1 = Address::generate(&s.env);
    let v2 = Address::generate(&s.env);
    let v3 = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &v1);
    s.client.add_verifier(&s.admin, &v2);
    s.client.add_verifier(&s.admin, &v3);

    // Reassignment 1 succeeds
    s.client.reassign_verifier(&id, &v1);
    assert_eq!(s.client.verifier_reassignments(&id), 1);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 1);
    assert_eq!(s.client.get_campaign(&id).verifier, v1);

    // Reassignment 2 succeeds (reaches maximum of 2)
    s.client.reassign_verifier(&id, &v2);
    assert_eq!(s.client.verifier_reassignments(&id), 2);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 2);
    assert_eq!(s.client.get_campaign(&id).verifier, v2);

    // Reassignment 3 rejected with MaxVerifierReassignmentsExceeded
    let res3 = s.client.try_reassign_verifier(&id, &v3);
    assert_eq!(res3, Err(Ok(Error::MaxVerifierReassignmentsExceeded)));
    assert_eq!(s.client.verifier_reassignments(&id), 2);
    assert_eq!(s.client.get_campaign(&id).verifier_reassignments, 2);
    assert_eq!(s.client.get_campaign(&id).verifier, v2);
}

#[test]
fn verifier_reassignments_view_errors_for_nonexistent_campaign() {
    let s = Setup::new();
    let res = s.client.try_verifier_reassignments(&999_999);
    assert_eq!(res, Err(Ok(Error::CampaignNotFound)));
}

#[test]
fn reassign_verifier_rejects_campaign_beneficiary() {
    let s = Setup::new();
    let id = s.campaign();

    // Register beneficiary as a verifier in the system
    s.client.add_verifier(&s.admin, &s.beneficiary);

    // Admin attempts to reassign the campaign's verifier to the beneficiary
    let res = s.client.try_reassign_verifier(&id, &s.beneficiary);
    assert_eq!(res, Err(Ok(Error::VerifierCannotBeBeneficiary)));

    // Legitimate reassignment to a distinct registered verifier succeeds
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &new_v);
    assert!(s.client.try_reassign_verifier(&id, &new_v).is_ok());
    assert_eq!(s.client.get_campaign(&id).verifier, new_v);
}

#[test]
fn reassigning_to_the_beneficiary_or_creator_is_rejected() {
    let s = Setup::new();
    let id = s.campaign();
    s.client.add_verifier(&s.admin, &s.beneficiary);
    s.client.add_verifier(&s.admin, &s.creator);

    assert_eq!(
        s.client.try_reassign_verifier(&id, &s.beneficiary),
        Err(Ok(Error::VerifierCannotBeBeneficiary))
    );
    assert_eq!(
        s.client.try_reassign_verifier(&id, &s.creator),
        Err(Ok(Error::ConflictOfInterest))
    );
    assert_eq!(s.client.get_campaign(&id).verifier, s.verifier);
    assert_eq!(s.client.verifier_reassignments(&id), 0);
}
