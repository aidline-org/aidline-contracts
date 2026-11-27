use super::*;

use crate::{TIMELOCK_DELAY, TIMELOCK_GRACE_PERIOD, TimelockedAction};

// ─── Timelock for sensitive admin actions (issue #113) ───────────────────────

#[test]
fn queued_action_waits_for_the_delay_then_runs() {
    let s = Setup::new();
    let id = s
        .client
        .queue_action(&TimelockedAction::SetBondRequirement(500));
    assert!(!s.env.events().all().events().is_empty(), "queue event");

    let eta = s.client.queued_action(&id).unwrap().eta;
    assert_eq!(eta, s.env.ledger().timestamp() + TIMELOCK_DELAY);
    assert_eq!(
        s.client.try_execute_action(&id),
        Err(Ok(Error::TimelockNotReady))
    );

    s.env.ledger().set_timestamp(eta - 1);
    assert_eq!(
        s.client.try_execute_action(&id),
        Err(Ok(Error::TimelockNotReady))
    );
    assert_eq!(s.client.bond_requirement(), 0);

    s.env.ledger().set_timestamp(eta);
    s.client.execute_action(&id);
    assert!(!s.env.events().all().events().is_empty(), "execute event");
    assert_eq!(s.client.bond_requirement(), 500);
    assert!(s.client.queued_action(&id).is_none());
    assert_eq!(
        s.client.try_execute_action(&id),
        Err(Ok(Error::ActionNotFound))
    );
}

#[test]
fn queued_action_expires_after_the_grace_period() {
    let s = Setup::new();
    let id = s
        .client
        .queue_action(&TimelockedAction::SetBondWithdrawDelay(DAY));
    let eta = s.client.queued_action(&id).unwrap().eta;

    s.env.ledger().set_timestamp(eta + TIMELOCK_GRACE_PERIOD);
    s.client.execute_action(&id);
    assert_eq!(s.client.bond_withdraw_delay(), DAY);

    let late = s
        .client
        .queue_action(&TimelockedAction::SetBondWithdrawDelay(2 * DAY));
    let late_eta = s.client.queued_action(&late).unwrap().eta;
    s.env
        .ledger()
        .set_timestamp(late_eta + TIMELOCK_GRACE_PERIOD + 1);
    assert_eq!(
        s.client.try_execute_action(&late),
        Err(Ok(Error::ActionExpired))
    );
    assert_eq!(s.client.bond_withdraw_delay(), DAY);
}

#[test]
fn admin_can_cancel_a_queued_action() {
    let s = Setup::new();
    let id = s
        .client
        .queue_action(&TimelockedAction::SetBondRequirement(500));

    s.client.cancel_action(&id);
    assert!(!s.env.events().all().events().is_empty(), "cancel event");
    assert!(s.client.queued_action(&id).is_none());

    let eta_passed = s.env.ledger().timestamp() + TIMELOCK_DELAY;
    s.env.ledger().set_timestamp(eta_passed);
    assert_eq!(
        s.client.try_execute_action(&id),
        Err(Ok(Error::ActionNotFound))
    );
    assert_eq!(
        s.client.try_cancel_action(&id),
        Err(Ok(Error::ActionNotFound))
    );
    assert_eq!(s.client.bond_requirement(), 0);
}

#[test]
fn queued_slash_runs_the_normal_slash_logic_after_the_delay() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    let action = s
        .client
        .queue_action(&TimelockedAction::SlashVerifier(v.clone(), id, 150));
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 400);

    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + TIMELOCK_DELAY);
    s.client.execute_action(&action);

    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 250);
    assert_eq!(s.client.slashed_recovery(&id), 150);
}

#[test]
fn queued_reassignment_runs_after_the_delay() {
    let s = Setup::new();
    let id = s.campaign();
    let new_verifier = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &new_verifier);

    let action = s.client.queue_action(&TimelockedAction::ReassignVerifier(
        id,
        new_verifier.clone(),
    ));
    assert_eq!(s.client.get_campaign(&id).verifier, s.verifier);

    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + TIMELOCK_DELAY);
    s.client.execute_action(&action);
    assert_eq!(s.client.get_campaign(&id).verifier, new_verifier);
}
