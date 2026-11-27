use super::*;

// ─── Issue #9 Tests: Multisig verification ───────────────────────────────────

#[test]
fn multisig_campaign_creation_validation() {
    let s = Setup::new();
    let v1 = s.verifier.clone();
    let v2 = Address::generate(&s.env);
    let v3 = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &v2);
    s.client.add_verifier(&s.admin, &v3);

    let verifiers = vec![&s.env, v1.clone(), v2.clone(), v3.clone()];
    let milestones = vec![&s.env, 500, 500];
    let deadline = s.env.ledger().timestamp() + 30 * DAY;
    let uri = String::from_str(&s.env, "ipfs://multisig-trees");

    // Valid 2 of 3 creation
    let id = s.client.create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    let c = s.client.get_campaign(&id);
    assert_eq!(c.threshold, 2);
    assert_eq!(c.verifiers.len(), 3);
    assert_eq!(c.goal, 1000);

    // Invalid threshold: 0
    let err_zero = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &0,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_zero, Err(Ok(Error::InvalidThreshold)));

    // Invalid threshold: exceeds verifiers count (4 > 3)
    let err_exceed = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &4,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_exceed, Err(Ok(Error::InvalidThreshold)));

    // Empty verifiers list
    let empty_verifiers = vec![&s.env];
    let err_empty = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &empty_verifiers,
        &1,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_empty, Err(Ok(Error::InvalidThreshold)));

    // Duplicate verifiers
    let dup_verifiers = vec![&s.env, v1.clone(), v2.clone(), v1.clone()];
    let err_dup = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &dup_verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_dup, Err(Ok(Error::DuplicateVerifier)));

    // Unregistered verifier in list
    let stranger = Address::generate(&s.env);
    let unreg_verifiers = vec![&s.env, v1, v2, stranger];
    let err_unreg = s.client.try_create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &unreg_verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    assert_eq!(err_unreg, Err(Ok(Error::NotVerifier)));
}

#[test]
fn multisig_milestone_release_requires_threshold() {
    let s = Setup::new();
    let v1 = s.verifier.clone();
    let v2 = Address::generate(&s.env);
    let v3 = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &v2);
    s.client.add_verifier(&s.admin, &v3);

    let verifiers = vec![&s.env, v1.clone(), v2.clone(), v3.clone()];
    let milestones = vec![&s.env, 300, 700];
    let deadline = s.env.ledger().timestamp() + 30 * DAY;
    let uri = String::from_str(&s.env, "ipfs://multisig-trees");

    let id = s.client.create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );

    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Single verifier approval (1 of 2 threshold) -> returns 0, no funds released yet
    let first_res = s.client.approve_milestone_multisig(&id, &v1, &s.proof());
    assert_eq!(first_res, 0);
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    let c = s.client.get_campaign(&id);
    assert_eq!(c.milestones_released, 0);
    assert_eq!(c.released, 0);

    // Approvals recorded for milestone 0
    let approvals = s.client.get_milestone_approvals(&id, &0);
    assert_eq!(approvals.len(), 1);
    assert_eq!(approvals.get(0).unwrap(), v1);

    // Duplicate approval by same verifier (v1) -> rejected
    let dup_res = s
        .client
        .try_approve_milestone_multisig(&id, &v1, &s.proof());
    assert_eq!(dup_res, Err(Ok(Error::DuplicateApproval)));

    // Approval by non-assigned verifier -> rejected
    let v4 = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &v4);
    let non_assigned = s
        .client
        .try_approve_milestone_multisig(&id, &v4, &s.proof());
    assert_eq!(non_assigned, Err(Ok(Error::NotVerifier)));

    // Second approval by v2 reaches threshold (2 of 2) -> releases milestone 0 (300)
    let second_res = s.client.approve_milestone_multisig(&id, &v2, &s.proof());
    assert_eq!(second_res, 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    let c_after = s.client.get_campaign(&id);
    assert_eq!(c_after.milestones_released, 1);
    assert_eq!(c_after.released, 300);

    // Second milestone: v3 approves first (1 of 2) -> returns 0
    let m2_first = s.client.approve_milestone_multisig(&id, &v3, &s.proof());
    assert_eq!(m2_first, 0);
    assert_eq!(s.token.balance(&s.beneficiary), 300);

    // v1 approves second (2 of 2) -> releases milestone 1 (700)
    let m2_second = s.client.approve_milestone_multisig(&id, &v1, &s.proof());
    assert_eq!(m2_second, 700);
    assert_eq!(s.token.balance(&s.beneficiary), 1000);
    let c_final = s.client.get_campaign(&id);
    assert_eq!(c_final.status, CampaignStatus::Completed);
}

#[test]
fn single_verifier_campaign_works_unchanged() {
    let s = Setup::new();
    let id = s.campaign(); // 3 milestones: 300, 300, 400
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // approve_milestone with original 2-arg signature works for single verifier campaign
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);
    assert_eq!(s.token.balance(&s.beneficiary), 300);
}

#[test]
fn multisig_campaign_blocks_anonymous_approve_milestone() {
    let s = Setup::new();
    let v1 = s.verifier.clone();
    let v2 = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &v2);

    let verifiers = vec![&s.env, v1, v2];
    let milestones = vec![&s.env, 500];
    let deadline = s.env.ledger().timestamp() + 30 * DAY;
    let uri = String::from_str(&s.env, "ipfs://trees");

    let id = s.client.create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &uri,
        &deadline,
        &milestones,
    );
    let donor = s.donor(500);
    s.client.donate(&donor, &id, &500);

    // Calling approve_milestone without specifying verifier on a multisig campaign returns Unauthorized
    let res = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
}

// ─── Issue #172: Multisig milestone proof URI collection ─────────────────────

#[test]
fn multisig_collects_proof_uris_from_all_approving_verifiers() {
    let s = Setup::new();
    let v1 = s.verifier.clone();
    let v2 = Address::generate(&s.env);
    let v3 = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &v2);
    s.client.add_verifier(&s.admin, &v3);

    let verifiers = vec![&s.env, v1.clone(), v2.clone(), v3.clone()];
    let milestones = vec![&s.env, 400, 600];
    let deadline = s.env.ledger().timestamp() + 30 * DAY;
    let id = s.client.create_multisig_campaign(
        &s.creator,
        &s.beneficiary,
        &verifiers,
        &2,
        &s.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://multisig-proofs"),
        &deadline,
        &milestones,
    );

    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    let proof1 = String::from_str(&s.env, "ipfs://proof-v1");
    let proof2 = String::from_str(&s.env, "ipfs://proof-v2");

    // Verifier 1 approves -> MilestoneApproved event emitted, no token transfer yet
    assert_eq!(s.client.approve_milestone_multisig(&id, &v1, &proof1), 0);
    assert!(!s.env.events().all().events().is_empty());
    assert_eq!(s.token.balance(&s.beneficiary), 0);

    // Duplicate approval by v1 fails and does not duplicate proof URI
    assert_eq!(
        s.client.try_approve_milestone_multisig(&id, &v1, &proof2),
        Err(Ok(Error::DuplicateApproval))
    );

    // Verifier 2 approves -> milestone 0 released and tokens transferred
    assert_eq!(s.client.approve_milestone_multisig(&id, &v2, &proof2), 400);
    assert_eq!(s.token.balance(&s.beneficiary), 400);

    let collected_proofs = s.client.get_milestone_approval_proofs(&id, &0);
    assert_eq!(collected_proofs.len(), 2);
    assert_eq!(collected_proofs.get(0).unwrap(), proof1);
    assert_eq!(collected_proofs.get(1).unwrap(), proof2);

    let approvers = s.client.get_milestone_approvals(&id, &0);
    assert_eq!(approvers, vec![&s.env, v1, v2]);
}
