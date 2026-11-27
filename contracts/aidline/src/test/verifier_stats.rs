// ─── Issue #119: verifier track record ───────────────────────────────────────

use super::*;

#[test]
fn assigned_approved_and_completed_counters_follow_a_campaign() {
    let s = Setup::new();
    let empty = s.client.verifier_stats(&s.verifier);
    assert_eq!(empty.campaigns_assigned, 0);

    let id = s.campaign();
    assert_eq!(s.client.verifier_stats(&s.verifier).campaigns_assigned, 1);

    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    let mid = s.client.verifier_stats(&s.verifier);
    assert_eq!(mid.milestones_approved, 2);
    assert_eq!(mid.campaigns_completed, 0);

    s.client.approve_milestone(&id, &s.proof());
    let done = s.client.verifier_stats(&s.verifier);
    assert_eq!(done.milestones_approved, 3);
    assert_eq!(done.campaigns_completed, 1);
    assert_eq!(done.campaigns_cancelled, 0);
}

#[test]
fn cancelling_counts_against_the_assigned_verifier() {
    let s = Setup::new();
    let id = s.campaign();
    s.client.cancel_campaign(&s.creator, &id);
    let second = s.campaign();
    s.client.cancel_campaign_with_reason(
        &s.creator,
        &second,
        &String::from_str(&s.env, "ipfs://why"),
    );

    let stats = s.client.verifier_stats(&s.verifier);
    assert_eq!(stats.campaigns_assigned, 2);
    assert_eq!(stats.campaigns_cancelled, 2);
    assert_eq!(stats.campaigns_completed, 0);
}

#[test]
fn a_reassigned_campaign_counts_for_the_verifier_who_held_it() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());

    let second = Address::generate(&s.env);
    s.client.add_verifier(&s.admin, &second);
    s.client.reassign_verifier(&id, &second);

    // The original verifier keeps what they did; the new one is assigned.
    let first = s.client.verifier_stats(&s.verifier);
    assert_eq!(first.campaigns_assigned, 1);
    assert_eq!(first.milestones_approved, 1);
    assert_eq!(s.client.verifier_stats(&second).campaigns_assigned, 1);

    // Events after the handover count for the new verifier only.
    s.client.cancel_campaign(&s.creator, &id);
    assert_eq!(s.client.verifier_stats(&second).campaigns_cancelled, 1);
    assert_eq!(s.client.verifier_stats(&s.verifier).campaigns_cancelled, 0);
}

#[test]
fn slashes_add_up_in_total_slashed() {
    let s = Setup::new();
    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    s.client.slash_verifier(&s.admin, &v, &id, &150);
    s.client.slash_verifier(&s.admin, &v, &id, &50);
    assert_eq!(s.client.verifier_stats(&v).total_slashed, 200);
    assert_eq!(s.client.verifier_stats(&s.verifier).total_slashed, 0);
}
