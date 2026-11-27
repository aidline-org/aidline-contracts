use super::*;

// ─── Issue #2 Tests: AdminChanged event ──────────────────────────────────────

#[test]
fn set_admin_emits_admin_changed_event() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);

    // set_admin emits AdminChanged. env.events().all() returns events from the
    // most recent contract invocation; after set_admin it must be non-empty.
    s.client.set_admin(&new_admin);
    assert!(
        !s.env.events().all().events().is_empty(),
        "AdminChanged event not emitted"
    );

    // The contract's admin should now be new_admin.
    assert_eq!(s.client.admin(), new_admin);
}

#[test]
fn set_admin_updates_stored_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);

    let old_admin = s.client.admin();
    s.client.set_admin(&new_admin);

    assert_ne!(s.client.admin(), old_admin);
    assert_eq!(s.client.admin(), new_admin);
}

// ─── Two-step admin transfer tests ───────────────────────────────────────────

#[test]
fn propose_admin_does_nothing_until_accepted() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);

    assert_eq!(s.client.pending_admin(), None);
    s.client.propose_admin(&candidate);

    // Pending admin is recorded, but active admin remains unchanged.
    assert_eq!(s.client.pending_admin(), Some(candidate.clone()));
    assert_eq!(s.client.admin(), s.admin);

    // Accept finalizes the transfer
    s.client.accept_admin();
    assert_eq!(s.client.admin(), candidate);
    assert_eq!(s.client.pending_admin(), None);
}

#[test]
fn accept_admin_fails_when_no_pending_proposal() {
    let s = Setup::new();
    assert_eq!(s.client.pending_admin(), None);
    let res = s.client.try_accept_admin();
    assert_eq!(res, Err(Ok(Error::NoPendingAdmin)));
}

#[test]
fn new_proposal_replaces_pending_proposal() {
    let s = Setup::new();
    let candidate_a = Address::generate(&s.env);
    let candidate_b = Address::generate(&s.env);

    s.client.propose_admin(&candidate_a);
    assert_eq!(s.client.pending_admin(), Some(candidate_a.clone()));

    // Replace proposal with candidate_b
    s.client.propose_admin(&candidate_b);
    assert_eq!(s.client.pending_admin(), Some(candidate_b.clone()));

    // Candidate A cannot accept because auth for candidate B is required
    let auth_a = MockAuth {
        address: &candidate_a,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "accept_admin",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res_a = s.client.mock_auths(&[auth_a]).try_accept_admin();
    assert_eq!(res_a.unwrap_err(), Err(InvokeError::Abort));

    // Candidate B can accept
    let auth_b = MockAuth {
        address: &candidate_b,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "accept_admin",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    s.client.mock_auths(&[auth_b]).accept_admin();
    assert_eq!(s.client.admin(), candidate_b);
    assert_eq!(s.client.pending_admin(), None);
}

#[test]
fn only_pending_admin_can_accept() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    s.client.propose_admin(&candidate);

    let auth_stranger = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "accept_admin",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res = s.client.mock_auths(&[auth_stranger]).try_accept_admin();
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.admin(), s.admin);
}

#[test]
fn events_emitted_for_propose_and_accept() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);

    s.client.propose_admin(&candidate);
    assert!(
        !s.env.events().all().events().is_empty(),
        "AdminProposed event not emitted"
    );

    s.client.accept_admin();
    assert!(
        !s.env.events().all().events().is_empty(),
        "AdminAccepted event not emitted"
    );
}

#[test]
fn set_admin_clears_pending_proposal() {
    let s = Setup::new();
    let candidate_a = Address::generate(&s.env);
    let candidate_b = Address::generate(&s.env);

    s.client.propose_admin(&candidate_a);
    assert_eq!(s.client.pending_admin(), Some(candidate_a));

    s.client.set_admin(&candidate_b);
    assert_eq!(s.client.admin(), candidate_b);
    assert_eq!(s.client.pending_admin(), None);

    let res = s.client.try_accept_admin();
    assert_eq!(res, Err(Ok(Error::NoPendingAdmin)));
}

#[test]
fn non_admin_cannot_propose_admin() {
    let s = Setup::new();
    let stranger = Address::generate(&s.env);
    let candidate = Address::generate(&s.env);

    let auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "propose_admin",
            args: (candidate.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let expected_auth_err = soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context,
        soroban_sdk::xdr::ScErrorCode::InvalidAction,
    );
    let res = s.client.mock_auths(&[auth]).try_propose_admin(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), expected_auth_err);
    assert_eq!(s.client.pending_admin(), None);
}

#[test]
fn nominate_admin_and_accept_flow_works() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);
    let candidate2 = Address::generate(&s.env);
    let extra_verifier = Address::generate(&s.env);

    // Nominate first candidate
    s.client.nominate_admin(&candidate);
    assert!(
        !s.env.events().all().events().is_empty(),
        "AdminNominated event not emitted"
    );
    assert_eq!(s.client.pending_admin(), Some(candidate.clone()));
    assert_eq!(s.client.admin(), s.admin);

    // Candidate cannot execute admin functions before accept_admin
    let auth_candidate_before = MockAuth {
        address: &candidate,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "add_verifier",
            args: (candidate.clone(), extra_verifier.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res = s
        .client
        .mock_auths(&[auth_candidate_before])
        .try_add_verifier(&candidate, &extra_verifier);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
    assert!(!s.client.is_verifier(&extra_verifier));

    // Re-nominating replaces the pending candidate
    s.client.nominate_admin(&candidate2);
    assert_eq!(s.client.pending_admin(), Some(candidate2.clone()));

    let auth_accept_c2 = MockAuth {
        address: &candidate2,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "accept_admin",
            args: ().into_val(&s.env),
            sub_invokes: &[],
        },
    };
    s.client.mock_auths(&[auth_accept_c2]).accept_admin();
    assert_eq!(s.client.admin(), candidate2);
    assert_eq!(s.client.pending_admin(), None);
}

#[test]
fn non_admin_cannot_nominate_admin() {
    let s = Setup::new();
    let stranger = Address::generate(&s.env);
    let candidate = Address::generate(&s.env);

    let auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "nominate_admin",
            args: (candidate.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let expected_auth_err = soroban_sdk::Error::from_type_and_code(
        soroban_sdk::xdr::ScErrorType::Context,
        soroban_sdk::xdr::ScErrorCode::InvalidAction,
    );
    let res = s.client.mock_auths(&[auth]).try_nominate_admin(&candidate);
    assert_eq!(res.unwrap_err().unwrap(), expected_auth_err);
    assert_eq!(s.client.pending_admin(), None);
}
