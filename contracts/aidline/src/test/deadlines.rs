use super::*;

// ─── Deadline extension tests ────────────────────────────────────────────────

#[test]
fn creator_can_extend_deadline_once() {
    let s = Setup::new();
    let id = s.campaign();

    let initial = s.client.get_campaign(&id);
    let original_deadline = initial.deadline;
    assert!(!initial.deadline_extended);

    let new_deadline = original_deadline + 15 * DAY;
    s.client.extend_deadline(&id, &new_deadline);
    assert!(
        !s.env.events().all().events().is_empty(),
        "DeadlineExtended event should be emitted"
    );

    let updated = s.client.get_campaign(&id);
    assert_eq!(updated.deadline, new_deadline);
    assert!(updated.deadline_extended);
}

#[test]
fn extend_deadline_by_accepts_donations_within_extended_window() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(500);
    let original_deadline = s.client.get_campaign(&id).deadline;

    // Extend by the maximum 30 days
    s.client.extend_deadline_by(&id, &(30 * DAY));
    assert!(
        !s.env.events().all().events().is_empty(),
        "DeadlineExtended event should be emitted"
    );

    let updated = s.client.get_campaign(&id);
    assert_eq!(updated.deadline, original_deadline + 30 * DAY);
    assert!(updated.deadline_extended);

    // Second extension via either entry point is rejected
    assert_eq!(
        s.client.try_extend_deadline_by(&id, &DAY),
        Err(Ok(Error::DeadlineAlreadyExtended))
    );

    // Advance past the original deadline but stay inside the extended window
    s.env.ledger().set_timestamp(original_deadline + 5 * DAY);
    let accepted = s.client.donate(&donor, &id, &250);
    assert_eq!(accepted, 250);
    assert_eq!(s.client.get_campaign(&id).raised, 250);

    // Advance past the extended deadline: donations are now rejected
    s.env
        .ledger()
        .set_timestamp(original_deadline + 30 * DAY + 1);
    assert_eq!(
        s.client.try_donate(&donor, &id, &100),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn extend_deadline_by_validates_bounds_and_status() {
    let s = Setup::new();
    let id = s.campaign();

    // Zero or > 30 days rejected
    assert_eq!(
        s.client.try_extend_deadline_by(&id, &0),
        Err(Ok(Error::InvalidDeadlineExtension))
    );
    assert_eq!(
        s.client.try_extend_deadline_by(&id, &(31 * DAY)),
        Err(Ok(Error::InvalidDeadlineExtension))
    );

    // Requires creator auth
    assert!(
        s.client
            .mock_auths(&[])
            .try_extend_deadline_by(&id, &(10 * DAY))
            .is_err()
    );

    // Completed campaign cannot extend deadline
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(
        s.client.try_extend_deadline_by(&id, &(10 * DAY)),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn second_deadline_extension_is_rejected() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    // First extension succeeds
    s.client
        .extend_deadline(&id, &(original_deadline + 10 * DAY));

    // Second extension fails
    let res = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 20 * DAY));
    assert_eq!(res, Err(Ok(Error::DeadlineAlreadyExtended)));
}

#[test]
fn deadline_extension_rules_enforced() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    // Extension > 30 days rejected
    let res_too_long = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 31 * DAY));
    assert_eq!(res_too_long, Err(Ok(Error::InvalidDeadlineExtension)));

    // Extension <= current deadline rejected
    let res_past = s.client.try_extend_deadline(&id, &original_deadline);
    assert_eq!(res_past, Err(Ok(Error::InvalidDeadlineExtension)));

    let res_earlier = s
        .client
        .try_extend_deadline(&id, &(original_deadline - DAY));
    assert_eq!(res_earlier, Err(Ok(Error::InvalidDeadlineExtension)));
}

#[test]
fn deadline_extension_fails_after_deadline_expired() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    s.pass_deadline();

    let res = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 10 * DAY));
    assert_eq!(res, Err(Ok(Error::CampaignExpired)));
}

#[test]
fn deadline_extension_fails_on_cancelled_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    s.client.cancel_campaign(&s.creator, &id);

    let res = s
        .client
        .try_extend_deadline(&id, &(original_deadline + 10 * DAY));
    assert_eq!(res, Err(Ok(Error::CampaignNotActive)));
}

#[test]
fn deadline_extension_requires_creator_auth() {
    let s = Setup::new();
    let id = s.campaign();
    let original_deadline = s.client.get_campaign(&id).deadline;

    let res = s
        .client
        .mock_auths(&[])
        .try_extend_deadline(&id, &(original_deadline + 10 * DAY));
    assert!(res.is_err());
}

#[test]
fn verifier_can_reject_milestone_with_reason() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &500);

    let details_uri = String::from_str(&s.env, "ipfs://rejection-reason");
    for reason in [
        crate::RejectionReason::IncompleteWork as u32,
        crate::RejectionReason::InsufficientProof as u32,
        crate::RejectionReason::BudgetExceeded as u32,
    ] {
        let res = s.client.try_reject_milestone(&id, &reason, &details_uri);
        assert_eq!(res, Ok(Ok(())));

        // Verify event was emitted
        assert!(
            !s.env.events().all().events().is_empty(),
            "MilestoneRejected event not emitted"
        );
    }

    // Verify no state change to funds, milestone progress, or Active status for non-fraud reasons
    let c = s.client.get_campaign(&id);
    assert_eq!(c.raised, 500);
    assert_eq!(c.released, 0);
    assert_eq!(c.milestones_released, 0);
    assert_eq!(c.status, CampaignStatus::Active);
    assert_eq!(s.token.balance(&s.beneficiary), 0);
}

#[test]
fn reject_milestone_with_fraud_detected_cancels_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &500);

    let details_uri = String::from_str(&s.env, "ipfs://fraud-evidence");
    s.client.reject_milestone(
        &id,
        &(crate::RejectionReason::FraudDetected as u32),
        &details_uri,
    );
    assert!(
        !s.env.events().all().events().is_empty(),
        "MilestoneRejected and CampaignCancelled events should be emitted"
    );

    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Cancelled);

    // Donor can immediately claim a refund after fraud cancellation
    let refunded = s.client.refund(&donor, &id);
    assert_eq!(refunded, 500);
    assert_eq!(s.token.balance(&donor), 1000);

    // Verifier stats reflect cancellation
    let stats = s.client.verifier_stats(&s.verifier);
    assert_eq!(stats.campaigns_cancelled, 1);
}

#[test]
fn reject_milestone_rejects_invalid_reason_code() {
    let s = Setup::new();
    let id = s.campaign();
    let details_uri = String::from_str(&s.env, "ipfs://rejection-reason");

    assert_eq!(
        s.client.try_reject_milestone(&id, &0, &details_uri),
        Err(Ok(Error::InvalidRejectionReason))
    );
    assert_eq!(
        s.client.try_reject_milestone(&id, &99, &details_uri),
        Err(Ok(Error::InvalidRejectionReason))
    );
}

#[test]
fn reject_milestone_requires_verifier_auth() {
    let s = Setup::new();
    let id = s.campaign();
    let details_uri = String::from_str(&s.env, "ipfs://rejection-reason");

    let res = s.client.mock_auths(&[]).try_reject_milestone(
        &id,
        &(crate::RejectionReason::IncompleteWork as u32),
        &details_uri,
    );
    assert!(res.is_err());
}

#[test]
fn reject_milestone_rejects_unregistered_verifier() {
    let s = Setup::new();
    let id = s.campaign();
    s.client.remove_verifier(&s.admin, &s.verifier);

    let details_uri = String::from_str(&s.env, "ipfs://rejection-reason");
    let res = s.client.try_reject_milestone(
        &id,
        &(crate::RejectionReason::InsufficientProof as u32),
        &details_uri,
    );
    assert_eq!(res, Err(Ok(Error::NotVerifier)));
}

#[test]
fn reject_milestone_rejects_long_reason_uri() {
    let s = Setup::new();
    let id = s.campaign();

    // 257-byte string exceeds MAX_URI_LENGTH (256)
    let long_uri = String::from_str(
        &s.env,
        "https://example.com/very/long/rejection/path/that/exceeds/the/maximum/allowed/length/of/two/hundred/and/fifty/six/bytes/---------------------------------------------------------------------------------------------------------------------------------------------",
    );
    let res = s.client.try_reject_milestone(
        &id,
        &(crate::RejectionReason::BudgetExceeded as u32),
        &long_uri,
    );
    assert_eq!(res, Err(Ok(Error::UriTooLong)));
}

#[test]
fn reject_milestone_fails_for_expired_or_cancelled_campaign() {
    let s = Setup::new();
    let id = s.campaign();
    let details_uri = String::from_str(&s.env, "ipfs://rejection-reason");
    let reason = crate::RejectionReason::IncompleteWork as u32;

    // Cancelled campaign
    s.client.cancel_campaign(&s.creator, &id);
    assert_eq!(
        s.client.try_reject_milestone(&id, &reason, &details_uri),
        Err(Ok(Error::CampaignNotActive))
    );

    // Expired campaign
    let s2 = Setup::new();
    let id2 = s2.campaign();
    let details_uri2 = String::from_str(&s2.env, "ipfs://rejection-reason");
    s2.pass_deadline();
    assert_eq!(
        s2.client.try_reject_milestone(&id2, &reason, &details_uri2),
        Err(Ok(Error::CampaignExpired))
    );
}

#[test]
fn reject_milestone_fails_when_all_milestones_released() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Release all 3 milestones
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());

    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Completed);

    let details_uri = String::from_str(&s.env, "ipfs://rejection-reason");
    assert_eq!(
        s.client.try_reject_milestone(
            &id,
            &(crate::RejectionReason::IncompleteWork as u32),
            &details_uri
        ),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn platform_fee_defaults_to_zero_and_treasury_to_none() {
    let s = Setup::new();
    assert_eq!(s.client.platform_fee(), 0);
    assert_eq!(s.client.treasury(), None);
}

#[test]
fn admin_can_set_platform_fee_and_treasury() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);

    s.client.set_platform_fee(&150);
    assert!(
        !s.env.events().all().events().is_empty(),
        "PlatformFeeUpdated event should be emitted"
    );
    assert_eq!(s.client.platform_fee(), 150);

    s.client.set_treasury(&treasury);
    assert!(
        !s.env.events().all().events().is_empty(),
        "TreasuryUpdated event should be emitted"
    );
    assert_eq!(s.client.treasury(), Some(treasury));
}

#[test]
fn platform_fee_capped_at_two_percent() {
    let s = Setup::new();

    // 200 bps (2%) succeeds
    let res_valid = s.client.try_set_platform_fee(&200);
    assert_eq!(res_valid, Ok(Ok(())));
    assert_eq!(s.client.platform_fee(), 200);

    // 201 bps (>2%) fails with InvalidAmount
    let res_invalid = s.client.try_set_platform_fee(&201);
    assert_eq!(res_invalid, Err(Ok(Error::InvalidAmount)));
}

#[test]
fn non_admin_cannot_set_fee_or_treasury() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);

    let res_fee = s.client.mock_auths(&[]).try_set_platform_fee(&100);
    assert!(res_fee.is_err());

    let res_tres = s.client.mock_auths(&[]).try_set_treasury(&treasury);
    assert!(res_tres.is_err());
}

#[test]
fn milestone_release_deducts_fee_and_sends_to_treasury() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);
    s.client.set_platform_fee(&200); // 2% fee
    s.client.set_treasury(&treasury);

    let id = s.campaign(); // milestones: 300, 300, 400
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // Approve milestone 0 (amount = 300)
    // Fee = 300 * 200 / 10000 = 6
    // Beneficiary receives 300 - 6 = 294
    // Treasury receives 6
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);

    assert_eq!(s.token.balance(&s.beneficiary), 294);
    assert_eq!(s.token.balance(&treasury), 6);
    assert_eq!(s.token.balance(&s.client.address), 700);

    let c = s.client.get_campaign(&id);
    assert_eq!(c.released, 300);
}

#[test]
fn refund_math_unaffected_by_platform_fee() {
    let s = Setup::new();
    let treasury = Address::generate(&s.env);
    s.client.set_platform_fee(&200); // 2%
    s.client.set_treasury(&treasury);

    let id = s.campaign(); // 300, 300, 400
    let donor_a = s.donor(600);
    let donor_b = s.donor(400);

    s.client.donate(&donor_a, &id, &600);
    s.client.donate(&donor_b, &id, &400);

    // Release milestone 0: 300 total leaves escrow (294 to beneficiary, 6 to treasury)
    s.client.approve_milestone(&id, &s.proof());

    // Cancel campaign so remaining 700 is refundable
    s.client.cancel_campaign(&s.creator, &id);

    // Donor A refund: 600 * (1000 - 300) / 1000 = 420
    let refund_a = s.client.refund(&donor_a, &id);
    assert_eq!(refund_a, 420);
    assert_eq!(s.token.balance(&donor_a), 420);

    // Donor B refund: 400 * (1000 - 300) / 1000 = 280
    let refund_b = s.client.refund(&donor_b, &id);
    assert_eq!(refund_b, 280);
    assert_eq!(s.token.balance(&donor_b), 280);

    // Total refunded = 420 + 280 = 700. Remaining in contract = 0.
    assert_eq!(s.token.balance(&s.client.address), 0);
}

#[test]
fn remaining_to_goal_returns_correct_amount_and_zero_when_funded() {
    let s = Setup::new();
    let id = s.campaign(); // goal is 1000

    // Initial state: goal - raised = 1000 - 0 = 1000
    assert_eq!(s.client.remaining_to_goal(&id), 1000);

    // Donate 400: remaining is 600
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &400);
    assert_eq!(s.client.remaining_to_goal(&id), 600);

    // Donate remaining 600: goal reached, remaining is 0
    s.client.donate(&donor, &id, &600);
    assert_eq!(s.client.remaining_to_goal(&id), 0);
}

#[test]
fn remaining_to_goal_returns_campaign_not_found_for_unknown_id() {
    let s = Setup::new();
    let res = s.client.try_remaining_to_goal(&99_999);
    assert_eq!(res, Err(Ok(Error::CampaignNotFound)));
}

#[test]
fn next_milestone_returns_correct_milestone_across_approvals() {
    let s = Setup::new();
    let id = s.campaign(); // milestones: [300, 300, 400]
    let donor = s.donor(1000);
    s.client.donate(&donor, &id, &1000);

    // After 0 approvals: next milestone is index 0 with amount 300
    assert_eq!(s.client.next_milestone(&id), Some((0, 300)));

    // After 1 approval: next milestone is index 1 with amount 300
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.client.next_milestone(&id), Some((1, 300)));

    // After 2 approvals: next milestone is index 2 with amount 400
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.client.next_milestone(&id), Some((2, 400)));

    // After all 3 approvals: campaign is completed, returns None
    s.client.approve_milestone(&id, &s.proof());
    let c = s.client.get_campaign(&id);
    assert_eq!(c.status, CampaignStatus::Completed);
    assert_eq!(s.client.next_milestone(&id), None);
}

#[test]
fn next_milestone_returns_campaign_not_found_for_unknown_id() {
    let s = Setup::new();
    let res = s.client.try_next_milestone(&99_999);
    assert_eq!(res, Err(Ok(Error::CampaignNotFound)));
}
