// ─── Issue #129: bond withdrawal waits for the verifier's campaigns ──────────

use super::*;

fn campaign_for(s: &Setup, verifier: &Address) -> u64 {
    s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://bond-lock"),
        &(s.env.ledger().timestamp() + 30 * DAY),
        &vec![&s.env, 300, 300, 400],
    )
}

#[test]
fn bond_is_locked_while_a_campaign_is_open_and_free_after_cancel() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = campaign_for(&s, &v);
    s.client.deregister_verifier(&v, &v);

    assert_eq!(
        s.client.try_withdraw_bond(&v),
        Err(Ok(Error::VerifierHasActiveCampaigns))
    );
    // Still slashable while the campaign is open.
    s.client.slash_verifier(&s.admin, &v, &id, &100);
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 300);

    s.client.cancel_campaign(&s.creator, &id);
    assert_eq!(s.client.withdraw_bond(&v), 300);
}

#[test]
fn bond_is_free_once_every_campaign_completes() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let first = campaign_for(&s, &v);
    let second = campaign_for(&s, &v);
    let donor = s.donor(2_000);

    for id in [first, second] {
        s.client.donate(&donor, &id, &1_000);
        for _ in 0..3 {
            s.client.approve_milestone(&id, &s.proof());
        }
        // The count only reaches zero after the last campaign.
        if id == first {
            s.client.deregister_verifier(&v, &v);
            assert_eq!(
                s.client.try_withdraw_bond(&v),
                Err(Ok(Error::VerifierHasActiveCampaigns))
            );
            s.client.add_verifier(&s.admin, &v);
        }
    }
    s.client.deregister_verifier(&v, &v);
    assert_eq!(s.client.withdraw_bond(&v), 400);
}

#[test]
fn reassigning_a_campaign_moves_the_lock_to_the_new_verifier() {
    let s = Setup::new();
    let old = s.bonded_verifier(1_000, 400);
    let new = s.bonded_verifier(1_000, 400);
    let id = campaign_for(&s, &old);
    s.client.deregister_verifier(&old, &old);
    s.client.deregister_verifier(&new, &new);
    s.client.add_verifier(&s.admin, &new);

    s.client.reassign_verifier(&id, &new);

    assert_eq!(s.client.withdraw_bond(&old), 400);
    assert_eq!(
        s.client.try_withdraw_bond(&new),
        Err(Ok(Error::VerifierHasActiveCampaigns))
    );
}

#[test]
fn verifiers_without_campaigns_withdraw_as_before() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    s.client.deregister_verifier(&v, &v);
    assert_eq!(s.client.withdraw_bond(&v), 400);
}
