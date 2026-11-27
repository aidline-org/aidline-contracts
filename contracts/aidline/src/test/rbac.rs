use super::*;
use crate::Role;
use crate::storage::PERSISTENT_THRESHOLD;
use crate::types::DataKey;
use soroban_sdk::testutils::storage::Persistent;

// ─── Issue #178: Role-based access control ───────────────────────────────────

#[test]
fn verifier_manager_can_add_and_remove_verifiers_but_cannot_slash_or_change_admin() {
    let s = Setup::new();
    let manager = Address::generate(&s.env);
    let candidate = Address::generate(&s.env);

    assert!(!s.client.has_role(&Role::VerifierManager, &manager));
    assert!(s.client.has_role(&Role::Admin, &s.admin));

    s.client.grant_role(&Role::VerifierManager, &manager);
    assert!(s.client.has_role(&Role::VerifierManager, &manager));

    // VerifierManager adds verifier without admin rights
    s.client.add_verifier(&manager, &candidate);
    assert!(s.client.is_verifier(&candidate));

    // VerifierManager cannot slash verifier
    let bonded = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    assert_eq!(
        s.client.try_slash_verifier(&manager, &bonded, &id, &100),
        Err(Ok(Error::Unauthorized))
    );

    // VerifierManager cannot transfer admin (requires primary admin auth)
    let auth = MockAuth {
        address: &manager,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_admin",
            args: (manager.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res = s.client.mock_auths(&[auth]).try_set_admin(&manager);
    assert!(res.is_err());
    assert_eq!(s.client.admin(), s.admin);

    // VerifierManager can remove verifier
    s.client.remove_verifier(&manager, &candidate);
    assert!(!s.client.is_verifier(&candidate));

    // Revocation immediately prevents further privileged execution
    s.client.revoke_role(&Role::VerifierManager, &manager);
    assert!(!s.client.has_role(&Role::VerifierManager, &manager));
    assert_eq!(
        s.client.try_add_verifier(&manager, &candidate),
        Err(Ok(Error::Unauthorized))
    );
    assert_eq!(
        s.client.try_remove_verifier(&manager, &s.verifier),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn slasher_can_slash_but_cannot_change_admin_and_revoke_blocks_slashing() {
    let s = Setup::new();
    let slasher = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    // Non-slasher attempting slash_verifier gets Unauthorized
    assert_eq!(
        s.client.try_slash_verifier(&stranger, &v, &id, &100),
        Err(Ok(Error::Unauthorized))
    );

    // Grant Slasher role to account B
    s.client.grant_role(&Role::Slasher, &slasher);
    assert!(s.client.has_role(&Role::Slasher, &slasher));

    // Account B can slash
    s.client.slash_verifier(&slasher, &v, &id, &150);
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 250);
    assert_eq!(s.client.slashed_recovery(&id), 150);

    // Account B cannot add/remove verifiers
    let candidate = Address::generate(&s.env);
    assert_eq!(
        s.client.try_add_verifier(&slasher, &candidate),
        Err(Ok(Error::Unauthorized))
    );

    // Account B cannot change admin or grant roles
    let auth_admin = MockAuth {
        address: &slasher,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_admin",
            args: (slasher.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    assert!(
        s.client
            .mock_auths(&[auth_admin])
            .try_set_admin(&slasher)
            .is_err()
    );
    assert_eq!(s.client.admin(), s.admin);

    let auth_grant = MockAuth {
        address: &slasher,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "grant_role",
            args: (Role::Slasher, stranger.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    assert!(
        s.client
            .mock_auths(&[auth_grant])
            .try_grant_role(&Role::Slasher, &stranger)
            .is_err()
    );

    // Revoke role -> B can no longer slash
    s.client.revoke_role(&Role::Slasher, &slasher);
    assert!(!s.client.has_role(&Role::Slasher, &slasher));
    assert_eq!(
        s.client.try_slash_verifier(&slasher, &v, &id, &50),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn role_admin_delegation_and_self_rejection() {
    let s = Setup::new();
    let co_admin = Address::generate(&s.env);
    let candidate = Address::generate(&s.env);

    // Contract address cannot be granted a role
    assert_eq!(
        s.client.try_grant_role(&Role::Slasher, &s.client.address),
        Err(Ok(Error::Unauthorized))
    );

    // Delegated Role::Admin can execute verifier and slashing actions
    s.client.grant_role(&Role::Admin, &co_admin);
    assert!(s.client.has_role(&Role::Admin, &co_admin));
    s.client.add_verifier(&co_admin, &candidate);
    assert!(s.client.is_verifier(&candidate));

    let id = s.campaign();
    s.client.cancel_campaign(&co_admin, &id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);

    s.client.revoke_role(&Role::Admin, &co_admin);
    assert!(!s.client.has_role(&Role::Admin, &co_admin));
    assert_eq!(
        s.client
            .try_add_verifier(&co_admin, &Address::generate(&s.env)),
        Err(Ok(Error::Unauthorized))
    );
}

#[test]
fn role_entry_ttl_is_extended_on_write_and_read() {
    let s = Setup::new();
    let manager = Address::generate(&s.env);
    s.client.grant_role(&Role::VerifierManager, &manager);

    let key = DataKey::Role(Role::VerifierManager, manager.clone());
    let initial_ttl = s.env.as_contract(&s.client.address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert!(initial_ttl >= PERSISTENT_THRESHOLD);

    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 10 * 17_280);
    let aged_ttl = s.env.as_contract(&s.client.address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert!(aged_ttl < PERSISTENT_THRESHOLD);

    assert!(s.client.has_role(&Role::VerifierManager, &manager));
    let refreshed_ttl = s.env.as_contract(&s.client.address, || {
        s.env.storage().persistent().get_ttl(&key)
    });
    assert!(refreshed_ttl >= PERSISTENT_THRESHOLD);
}
