use super::*;

// ─── Issue #10 Tests: Per-campaign tokens and allowlist ──────────────────────

#[test]
fn admin_manages_token_allowlist() {
    let s = Setup::new();
    let new_token = Address::generate(&s.env);

    // Initially not allowed
    assert!(!s.client.is_token_allowed(&new_token));

    // Admin adds token
    s.client.add_allowed_token(&new_token);
    assert!(s.client.is_token_allowed(&new_token));

    // Admin removes token
    s.client.remove_allowed_token(&new_token);
    assert!(!s.client.is_token_allowed(&new_token));
}

#[test]
fn unallowed_token_campaign_creation_fails() {
    let s = Setup::new();
    let unallowed_tok = Address::generate(&s.env);

    let res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &unallowed_tok,
        &CampaignKind::Climate,
        &String::from_str(&s.env, "ipfs://fail"),
        &(s.env.ledger().timestamp() + DAY),
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::TokenNotAllowed)));
}

#[test]
fn campaigns_with_different_tokens_operate_side_by_side() {
    let s = Setup::new();
    let env = &s.env;

    // Create a second token: USDC SAC
    let usdc_sac = env.register_stellar_asset_contract_v2(s.admin.clone());
    let usdc = TokenClient::new(env, &usdc_sac.address());
    let usdc_addr = usdc_sac.address();

    // Admin adds USDC to allowlist
    s.client.add_allowed_token(&usdc_addr);
    assert!(s.client.is_token_allowed(&usdc_addr));

    // Campaign 1: raises in default token
    let c1_id = s.campaign();

    // Campaign 2: raises in USDC
    let c2_id = s.client.create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &usdc_addr,
        &CampaignKind::Emergency,
        &String::from_str(env, "ipfs://usdc-campaign"),
        &(env.ledger().timestamp() + 30 * DAY),
        &vec![env, 400, 600],
    );

    let c1 = s.client.get_campaign(&c1_id);
    let c2 = s.client.get_campaign(&c2_id);
    assert_eq!(c1.token, s.token.address);
    assert_eq!(c2.token, usdc_addr);

    // Donors: donor1 has token A, donor2 has USDC
    let donor1 = s.donor(1000);
    let donor2 = Address::generate(env);
    StellarAssetClient::new(env, &usdc_addr).mint(&donor2, &1000);

    // Donate to Campaign 1 with token A
    s.client.donate(&donor1, &c1_id, &500);
    // Donate to Campaign 2 with USDC
    s.client.donate(&donor2, &c2_id, &500);

    // Check balances of escrow contract
    assert_eq!(s.token.balance(&s.client.address), 500);
    assert_eq!(usdc.balance(&s.client.address), 500);
    assert_eq!(s.token.balance(&s.beneficiary), 0);
    assert_eq!(usdc.balance(&s.beneficiary), 0);

    // Approve milestone on Campaign 1 (releases 300 in token A)
    s.client.approve_milestone(&c1_id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(usdc.balance(&s.beneficiary), 0);
    assert_eq!(s.token.balance(&s.client.address), 200);

    // Approve milestone on Campaign 2 (releases 400 in USDC)
    s.client.approve_milestone(&c2_id, &s.proof());
    assert_eq!(s.token.balance(&s.beneficiary), 300);
    assert_eq!(usdc.balance(&s.beneficiary), 400);
    assert_eq!(usdc.balance(&s.client.address), 100);

    // Cancel Campaign 2 and test refund in USDC
    s.client.cancel_campaign(&s.creator, &c2_id);
    let refund_amt = s.client.refund(&donor2, &c2_id);
    assert_eq!(refund_amt, 100);
    assert_eq!(usdc.balance(&donor2), 600);
    assert_eq!(usdc.balance(&s.client.address), 0);
}

// =============================================================================
// Read-only view function tests
// =============================================================================

#[test]
fn test_view_campaign_count() {
    let s = Setup::new();
    assert_eq!(s.client.campaign_count(), 0);

    let c1 = s.campaign();
    assert_eq!(c1, 0);
    assert_eq!(s.client.campaign_count(), 1);

    let c2 = s.campaign();
    assert_eq!(c2, 1);
    assert_eq!(s.client.campaign_count(), 2);
}

#[test]
fn test_view_contribution_of() {
    let s = Setup::new();
    let cid = s.campaign();
    let donor = s.donor(1000);

    // Initial contribution is 0
    assert_eq!(s.client.contribution_of(&cid, &donor), 0);

    // First donation
    s.client.donate(&donor, &cid, &300);
    assert_eq!(s.client.contribution_of(&cid, &donor), 300);

    // Cumulative donation
    s.client.donate(&donor, &cid, &200);
    assert_eq!(s.client.contribution_of(&cid, &donor), 500);

    // Cancel campaign and refund donor -> contribution resets to 0
    s.client.cancel_campaign(&s.creator, &cid);
    s.client.refund(&donor, &cid);
    assert_eq!(s.client.contribution_of(&cid, &donor), 0);
}

#[test]
fn test_view_refundable_amount() {
    let s = Setup::new();
    let id = s.campaign();
    let donor1 = s.donor(1000);
    let donor2 = s.donor(1000);
    let stranger = Address::generate(&s.env);

    // 1. Non-existent campaign returns 0
    assert_eq!(s.client.refundable_amount(&999_999, &donor1), 0);

    // 2. Active campaign within deadline is not refundable -> returns 0
    s.client.donate(&donor1, &id, &500);
    s.client.donate(&donor2, &id, &500);
    assert_eq!(s.client.refundable_amount(&id, &donor1), 0);
    assert_eq!(s.client.refundable_amount(&id, &stranger), 0);

    // 3. Cancelled campaign matches refund amount
    // Release milestone 0 (300). Raised: 1000, Released: 300, Unreleased: 700.
    s.client.approve_milestone(&id, &s.proof());
    let cancel_id = id;
    s.client.cancel_campaign(&s.creator, &cancel_id);

    // donor1 contributed 500: 500 * 700 / 1000 = 350
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor1), 350);
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor2), 350);
    assert_eq!(s.client.refundable_amount(&cancel_id, &stranger), 0);

    // Calling refund pays exactly what refundable_amount reported
    let refunded = s.client.refund(&donor1, &cancel_id);
    assert_eq!(refunded, 350);
    // After refund, donor1 has no recorded contribution -> refundable_amount is 0
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor1), 0);
    // donor2 still has 350 refundable
    assert_eq!(s.client.refundable_amount(&cancel_id, &donor2), 350);

    // 4. Expired campaign matches refund amount
    let s_exp = Setup::new();
    let exp_id = s_exp.campaign();
    let d = s_exp.donor(600);
    s_exp.client.donate(&d, &exp_id, &600);
    assert_eq!(s_exp.client.refundable_amount(&exp_id, &d), 0);

    s_exp.pass_deadline();
    // 600 * 1000 / 600 = 600 (full refund)
    assert_eq!(s_exp.client.refundable_amount(&exp_id, &d), 600);
    assert_eq!(s_exp.client.refund(&d, &exp_id), 600);
    assert_eq!(s_exp.client.refundable_amount(&exp_id, &d), 0);

    // 5. Overdue milestone in active campaign matches refund amount
    let s_overdue = Setup::new();
    let now = s_overdue.env.ledger().timestamp();
    let due_dates = vec![&s_overdue.env, now + 10 * DAY, now + 20 * DAY];
    let milestones = vec![&s_overdue.env, 300, 700];
    let overdue_id = s_overdue.client.create_campaign_with_due_dates(
        &s_overdue.creator,
        &s_overdue.beneficiary,
        &s_overdue.verifier,
        &s_overdue.token.address,
        &CampaignKind::Climate,
        &String::from_str(&s_overdue.env, "ipfs://overdue-view"),
        &(now + 30 * DAY),
        &milestones,
        &due_dates,
    );
    let d_overdue = s_overdue.donor(500);
    s_overdue.client.donate(&d_overdue, &overdue_id, &500);
    assert_eq!(
        s_overdue.client.refundable_amount(&overdue_id, &d_overdue),
        0
    );

    s_overdue.env.ledger().set_timestamp(now + 11 * DAY);
    assert_eq!(
        s_overdue.client.refundable_amount(&overdue_id, &d_overdue),
        500
    );
    assert_eq!(s_overdue.client.refund(&d_overdue, &overdue_id), 500);
    assert_eq!(
        s_overdue.client.refundable_amount(&overdue_id, &d_overdue),
        0
    );

    // 6. Completed campaign returns 0
    let s_comp = Setup::new();
    let comp_id = s_comp.campaign();
    let d_comp = s_comp.donor(1000);
    s_comp.client.donate(&d_comp, &comp_id, &1000);
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    assert_eq!(s_comp.client.refundable_amount(&comp_id, &d_comp), 0);
}

#[test]
fn test_view_effective_status() {
    let s = Setup::new();

    // 1. Unknown campaign returns CampaignNotFound
    assert_eq!(
        s.client.try_effective_status(&999_999),
        Err(Ok(Error::CampaignNotFound))
    );

    // 2. Active campaign within deadline -> Active
    let id = s.campaign();
    assert_eq!(s.client.effective_status(&id), CampaignStatus::Active);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);

    // 3. Active campaign past deadline -> Expired
    s.pass_deadline();
    assert_eq!(s.client.effective_status(&id), CampaignStatus::Expired);
    // Note: stored status remains Active as per ADR-0002
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Active);

    // 4. Cancelled campaign -> Cancelled (before and after deadline)
    let s_cancel = Setup::new();
    let cancel_id = s_cancel.campaign();
    s_cancel
        .client
        .cancel_campaign(&s_cancel.creator, &cancel_id);
    assert_eq!(
        s_cancel.client.effective_status(&cancel_id),
        CampaignStatus::Cancelled
    );
    s_cancel.pass_deadline();
    assert_eq!(
        s_cancel.client.effective_status(&cancel_id),
        CampaignStatus::Cancelled
    );

    // 5. Completed campaign -> Completed (before and after deadline)
    let s_comp = Setup::new();
    let comp_id = s_comp.campaign();
    let d = s_comp.donor(1000);
    s_comp.client.donate(&d, &comp_id, &1000);
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    s_comp.client.approve_milestone(&comp_id, &s_comp.proof());
    assert_eq!(
        s_comp.client.effective_status(&comp_id),
        CampaignStatus::Completed
    );
    s_comp.pass_deadline();
    assert_eq!(
        s_comp.client.effective_status(&comp_id),
        CampaignStatus::Completed
    );
}

#[test]
fn test_view_milestone_due_dates() {
    let s = Setup::new();

    // 1. Unknown campaign returns CampaignNotFound
    assert_eq!(
        s.client.try_milestone_due_dates(&999_999),
        Err(Ok(Error::CampaignNotFound))
    );

    // 2. Campaign created with create_campaign returns empty vector
    let id_standard = s.campaign();
    let due_dates_empty = s.client.milestone_due_dates(&id_standard);
    assert_eq!(due_dates_empty.len(), 0);

    // 3. Campaign created with create_campaign_with_due_dates returns stored due dates
    let now = s.env.ledger().timestamp();
    let expected_dates = vec![&s.env, now + 5 * DAY, now + 15 * DAY, now + 25 * DAY];
    let milestones = vec![&s.env, 300, 300, 400];
    let id_with_dates = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://due-dates-view"),
        &(now + 30 * DAY),
        &milestones,
        &expected_dates,
    );

    let retrieved_dates = s.client.milestone_due_dates(&id_with_dates);
    assert_eq!(retrieved_dates, expected_dates);
}

#[test]
fn create_campaign_with_due_dates_validates_future_and_monotonicity() {
    let s = Setup::new();
    let now = s.env.ledger().timestamp();
    let milestones = vec![&s.env, 300, 700];

    let count_before = s.client.campaign_count();

    // 1. Due date in the past fails with DeadlineInPast
    let past_dates = vec![&s.env, now - 10, now + 100];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://past-due"),
            &(now + 30 * DAY),
            &milestones,
            &past_dates,
        ),
        Err(Ok(Error::DeadlineInPast))
    );

    // 2. Due date equal to current timestamp fails with DeadlineInPast
    let now_dates = vec![&s.env, now, now + 100];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://now-due"),
            &(now + 30 * DAY),
            &milestones,
            &now_dates,
        ),
        Err(Ok(Error::DeadlineInPast))
    );

    // 3. Non-monotonic due dates fail with InvalidMilestones (decreasing)
    let decreasing_dates = vec![&s.env, now + 100, now + 50];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://dec-due"),
            &(now + 30 * DAY),
            &milestones,
            &decreasing_dates,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // 4. Equal consecutive due dates fail with InvalidMilestones
    let equal_dates = vec![&s.env, now + 100, now + 100];
    assert_eq!(
        s.client.try_create_campaign_with_due_dates(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&s.env, "ipfs://equal-due"),
            &(now + 30 * DAY),
            &milestones,
            &equal_dates,
        ),
        Err(Ok(Error::InvalidMilestones))
    );

    // Verify campaign was not created on validation failures
    assert_eq!(s.client.campaign_count(), count_before);

    // 5. Valid future monotonic due dates succeed
    let valid_dates = vec![&s.env, now + 100, now + 200];
    let res = s.client.create_campaign_with_due_dates(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &String::from_str(&s.env, "ipfs://valid-due"),
        &(now + 30 * DAY),
        &milestones,
        &valid_dates,
    );
    assert_eq!(s.client.campaign_count(), count_before + 1);
    assert_eq!(s.client.milestone_due_dates(&res), valid_dates);
}

#[test]
fn test_view_is_verifier() {
    let s = Setup::new();
    let candidate = Address::generate(&s.env);

    // Initially not a verifier
    assert!(!s.client.is_verifier(&candidate));

    // Admin adds verifier
    s.client.add_verifier(&s.admin, &candidate);
    assert!(s.client.is_verifier(&candidate));

    // Admin removes verifier
    s.client.remove_verifier(&s.admin, &candidate);
    assert!(!s.client.is_verifier(&candidate));
}

#[test]
fn test_view_admin() {
    let s = Setup::new();
    assert_eq!(s.client.admin(), s.admin);

    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);
    assert_eq!(s.client.admin(), new_admin);
}

#[test]
fn test_view_token() {
    let s = Setup::new();
    assert_eq!(s.client.token(), s.token.address);
}

#[test]
fn test_view_version() {
    let s = Setup::new();
    assert_eq!(
        s.client.version(),
        String::from_str(&s.env, env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(s.client.version(), String::from_str(&s.env, "0.1.0"));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn prop_refund_invariants_hold_after_cancellation(
        milestone_amounts in prop::collection::vec(100i128..10_000i128, 1..=5),
        donor_donations in prop::collection::vec(10i128..5_000i128, 1..=8),
        num_milestones_to_release in 0usize..5,
    ) {
        let s = Setup::new();
        let mut sdk_milestones = soroban_sdk::Vec::new(&s.env);
        let mut goal: i128 = 0;
        for &m in milestone_amounts.iter() {
            sdk_milestones.push_back(m);
            goal += m;
        }

        let deadline = s.env.ledger().timestamp() + 30 * DAY;
        let id = s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Climate,
            &String::from_str(&s.env, "ipfs://prop-cancel"),
            &deadline,
            &sdk_milestones,
        );

        let mut total_raised: i128 = 0;
        let mut active_donors = soroban_sdk::Vec::new(&s.env);

        for &amount in donor_donations.iter() {
            if total_raised >= goal {
                break;
            }
            let donor = s.donor(amount);
            let accepted = s.client.donate(&donor, &id, &amount);
            if accepted > 0 {
                total_raised += accepted;
                active_donors.push_back(donor);
            }
        }

        let mut cumulative: i128 = 0;
        let mut max_releasable: usize = 0;
        for &m in milestone_amounts.iter() {
            if cumulative + m <= total_raised {
                cumulative += m;
                max_releasable += 1;
            } else {
                break;
            }
        }

        let release_count = num_milestones_to_release.min(max_releasable);
        let mut total_released: i128 = 0;
        for _ in 0..release_count {
            let released = s.client.approve_milestone(&id, &s.proof());
            total_released += released;
        }

        if release_count < milestone_amounts.len() {
            // Campaign remains Active; cancellation succeeds and opens pro-rata refunds.
            s.client.cancel_campaign(&s.creator, &id);

            let mut total_refunded: i128 = 0;
            for donor in active_donors.iter() {
                let refund = s.client.refund(&donor, &id);
                prop_assert!(refund >= 0, "Refund must be non-negative");
                total_refunded += refund;
            }

            // 1. Total refunds plus releases never exceed total raised
            prop_assert!(
                total_refunded + total_released <= total_raised,
                "Total refunded ({}) + total released ({}) exceeded total raised ({})",
                total_refunded, total_released, total_raised
            );

            // 2. Contract balance never goes negative and equals unreleased dust
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(
                contract_balance >= 0,
                "Contract balance is negative: {}",
                contract_balance
            );
            prop_assert_eq!(
                contract_balance,
                total_raised - total_released - total_refunded,
                "Contract balance did not match total_raised - total_released - total_refunded"
            );
            prop_assert_eq!(s.token.balance(&s.beneficiary), total_released);
        } else {
            // All milestones were approved; campaign is Completed and funds delivered.
            let c = s.client.get_campaign(&id);
            prop_assert_eq!(c.status, CampaignStatus::Completed);
            prop_assert!(s.client.try_cancel_campaign(&s.creator, &id).is_err());
            for donor in active_donors.iter() {
                prop_assert!(s.client.try_refund(&donor, &id).is_err());
            }
            prop_assert_eq!(s.token.balance(&s.beneficiary), total_released);
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(contract_balance >= 0);
            prop_assert_eq!(contract_balance, total_raised - total_released);
        }
    }

    #[test]
    fn prop_refund_invariants_hold_after_expiry(
        milestone_amounts in prop::collection::vec(100i128..10_000i128, 1..=5),
        donor_donations in prop::collection::vec(10i128..5_000i128, 1..=8),
        num_milestones_to_release in 0usize..5,
    ) {
        let s = Setup::new();
        let mut sdk_milestones = soroban_sdk::Vec::new(&s.env);
        let mut goal: i128 = 0;
        for &m in milestone_amounts.iter() {
            sdk_milestones.push_back(m);
            goal += m;
        }

        let deadline = s.env.ledger().timestamp() + 30 * DAY;
        let id = s.client.create_campaign(
            &s.creator,
            &s.beneficiary,
            &s.verifier,
            &s.token.address,
            &CampaignKind::Climate,
            &String::from_str(&s.env, "ipfs://prop-expiry"),
            &deadline,
            &sdk_milestones,
        );

        let mut total_raised: i128 = 0;
        let mut active_donors = soroban_sdk::Vec::new(&s.env);

        for &amount in donor_donations.iter() {
            if total_raised >= goal {
                break;
            }
            let donor = s.donor(amount);
            let accepted = s.client.donate(&donor, &id, &amount);
            if accepted > 0 {
                total_raised += accepted;
                active_donors.push_back(donor);
            }
        }

        let mut cumulative: i128 = 0;
        let mut max_releasable: usize = 0;
        for &m in milestone_amounts.iter() {
            if cumulative + m <= total_raised {
                cumulative += m;
                max_releasable += 1;
            } else {
                break;
            }
        }

        let release_count = num_milestones_to_release.min(max_releasable);
        let mut total_released: i128 = 0;
        for _ in 0..release_count {
            let released = s.client.approve_milestone(&id, &s.proof());
            total_released += released;
        }

        // Pass deadline to trigger expiry refunds
        s.env.ledger().set_timestamp(deadline + 1);

        if release_count < milestone_amounts.len() {
            let mut total_refunded: i128 = 0;
            for donor in active_donors.iter() {
                let refund = s.client.refund(&donor, &id);
                prop_assert!(refund >= 0, "Refund must be non-negative");
                total_refunded += refund;
            }

            // Invariants
            prop_assert!(
                total_refunded + total_released <= total_raised,
                "Total refunded ({}) + total released ({}) exceeded total raised ({})",
                total_refunded, total_released, total_raised
            );
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(contract_balance >= 0, "Negative balance");
            prop_assert_eq!(
                contract_balance,
                total_raised - total_released - total_refunded
            );
        } else {
            // Already completed
            let c = s.client.get_campaign(&id);
            prop_assert_eq!(c.status, CampaignStatus::Completed);
            for donor in active_donors.iter() {
                prop_assert!(s.client.try_refund(&donor, &id).is_err());
            }
            let contract_balance = s.token.balance(&s.client.address);
            prop_assert!(contract_balance >= 0);
            prop_assert_eq!(contract_balance, total_raised - total_released);
        }
    }
}
