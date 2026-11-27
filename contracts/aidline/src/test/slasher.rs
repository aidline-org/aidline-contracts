// ─── Issue #111: slasher role ────────────────────────────────────────────────

use super::*;

fn only_signed_by(s: &Setup, who: &Address, v: &Address, id: u64) -> Result<(), ()> {
    let auth = MockAuth {
        address: who,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "slash_verifier",
            args: (who.clone(), v.clone(), id, 100i128).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    match s
        .client
        .mock_auths(&[auth])
        .try_slash_verifier(who, v, &id, &100)
    {
        Ok(Ok(())) => Ok(()),
        _ => Err(()),
    }
}

#[test]
fn the_admin_is_the_slasher_until_another_is_set() {
    let s = Setup::new();
    assert_eq!(s.client.get_slasher(), s.admin);

    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    assert!(only_signed_by(&s, &s.admin, &v, id).is_ok());
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 300);
}

#[test]
fn only_the_new_slasher_can_slash_and_the_admin_cannot() {
    let s = Setup::new();
    let slasher = Address::generate(&s.env);
    s.client.set_slasher(&slasher);
    assert!(
        !s.env.events().all().events().is_empty(),
        "SlasherChanged event"
    );
    assert_eq!(s.client.get_slasher(), slasher);

    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    assert!(only_signed_by(&s, &s.admin, &v, id).is_err());
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 400);

    assert!(only_signed_by(&s, &slasher, &v, id).is_ok());
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 300);
}

#[test]
fn only_the_admin_can_change_the_slasher() {
    let s = Setup::new();
    let slasher = Address::generate(&s.env);
    let outsider = Address::generate(&s.env);
    let auth = MockAuth {
        address: &outsider,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_slasher",
            args: (slasher.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    assert!(
        s.client
            .mock_auths(&[auth])
            .try_set_slasher(&slasher)
            .is_err()
    );
    assert_eq!(s.client.get_slasher(), s.admin);
}

#[test]
fn the_default_slasher_follows_a_change_of_admin() {
    let s = Setup::new();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);
    assert_eq!(s.client.get_slasher(), new_admin);
}

#[test]
fn queueing_a_slash_needs_the_slasher_not_the_admin() {
    let s = Setup::new();
    let slasher = Address::generate(&s.env);
    s.client.set_slasher(&slasher);
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();
    let action = crate::TimelockedAction::SlashVerifier(v.clone(), id, 100);

    // The admin alone cannot queue it.
    let admin_only = MockAuth {
        address: &s.admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "queue_action",
            args: (action.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    assert!(
        s.client
            .mock_auths(&[admin_only])
            .try_queue_action(&action)
            .is_err()
    );

    // The slasher can.
    let by_slasher = MockAuth {
        address: &slasher,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "queue_action",
            args: (action.clone(),).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    s.client.mock_auths(&[by_slasher]).queue_action(&action);

    // Other admin actions are still queued by the admin.
    s.client
        .queue_action(&crate::TimelockedAction::SetBondRequirement(500));
}
