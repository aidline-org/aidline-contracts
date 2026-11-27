use super::*;

// ─── Admin transfer tests: old admin access revoked ─────────────────────────

#[test]
fn old_admin_cannot_add_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let candidate = Address::generate(&s.env);
    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "add_verifier",
            args: (old_admin.clone(), candidate.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s
        .client
        .mock_auths(&[auth])
        .try_add_verifier(&old_admin, &candidate);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
    assert!(!s.client.is_verifier(&candidate));

    // New admin can add verifier
    s.client.add_verifier(&new_admin, &candidate);
    assert!(s.client.is_verifier(&candidate));
}

#[test]
fn old_admin_cannot_remove_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let verifier = Address::generate(&s.env);
    s.client.add_verifier(&new_admin, &verifier);
    assert!(s.client.is_verifier(&verifier));

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "remove_verifier",
            args: (old_admin.clone(), verifier.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s
        .client
        .mock_auths(&[auth])
        .try_remove_verifier(&old_admin, &verifier);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
    assert!(s.client.is_verifier(&verifier));

    // New admin can remove verifier
    s.client.remove_verifier(&new_admin, &verifier);
    assert!(!s.client.is_verifier(&verifier));
}

#[test]
fn old_admin_cannot_set_bond_requirement_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_bond_requirement",
            args: (500i128,).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s.client.mock_auths(&[auth]).try_set_bond_requirement(&500);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.bond_requirement(), 0);

    // New admin can set bond requirement
    s.client.set_bond_requirement(&500);
    assert_eq!(s.client.bond_requirement(), 500);
}

#[test]
fn old_admin_cannot_set_bond_withdraw_delay_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_bond_withdraw_delay",
            args: (86400u64,).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s
        .client
        .mock_auths(&[auth])
        .try_set_bond_withdraw_delay(&86400);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.bond_withdraw_delay(), 0);

    // New admin can set bond withdraw delay
    s.client.set_bond_withdraw_delay(&86400);
    assert_eq!(s.client.bond_withdraw_delay(), 86400);
}

#[test]
fn old_admin_cannot_slash_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let v = s.bonded_verifier(1_000, 400);
    let id = s.campaign();

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "slash_verifier",
            args: (old_admin.clone(), v.clone(), id, 150i128).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s
        .client
        .mock_auths(&[auth])
        .try_slash_verifier(&old_admin, &v, &id, &150);
    assert_eq!(res, Err(Ok(Error::Unauthorized)));
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 400);

    // New admin can slash verifier
    s.client.slash_verifier(&new_admin, &v, &id, &150);
    assert_eq!(s.client.get_verifier_bond(&v).unwrap().remaining, 250);
    assert_eq!(s.client.slashed_recovery(&id), 150);
}

#[test]
fn old_admin_cannot_reassign_verifier_after_set_admin() {
    let s = Setup::new();
    let old_admin = s.admin.clone();
    let new_admin = Address::generate(&s.env);
    s.client.set_admin(&new_admin);

    let id = s.campaign();
    let new_v = Address::generate(&s.env);
    s.client.add_verifier(&new_admin, &new_v);

    let auth = MockAuth {
        address: &old_admin,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "reassign_verifier",
            args: (id, new_v.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };

    let res = s
        .client
        .mock_auths(&[auth])
        .try_reassign_verifier(&id, &new_v);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));
    assert_eq!(s.client.get_campaign(&id).verifier, s.verifier);

    // New admin can reassign verifier
    s.client.reassign_verifier(&id, &new_v);
    assert_eq!(s.client.get_campaign(&id).verifier, new_v);
}

#[test]
fn cancelled_campaign_rejects_new_activity() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let sponsor = s.donor(10_000_000);

    s.client.cancel_campaign(&s.creator, &id);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);

    assert_eq!(
        s.client.try_donate(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );
    assert_eq!(
        s.client.try_approve_milestone(&id, &s.proof()),
        Err(Ok(Error::CampaignNotActive))
    );

    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &100, &expiry);
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );

    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000),
        Err(Ok(Error::CampaignNotActive))
    );
}

#[test]
fn completed_campaign_rejects_new_activity() {
    let s = Setup::new();
    let id = s.campaign();
    let donor = s.donor(1_000);
    let sponsor = s.donor(10_000_000);

    // Fully fund and release all milestones
    s.client.donate(&donor, &id, &1_000);
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    s.client.approve_milestone(&id, &s.proof());
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Completed);

    assert_eq!(
        s.client.try_donate(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );

    let expiry = s.env.ledger().sequence() + 1_000;
    s.token.approve(&donor, &s.client.address, &100, &expiry);
    assert_eq!(
        s.client.try_create_pledge(&donor, &id, &100),
        Err(Ok(Error::CampaignNotActive))
    );

    assert_eq!(
        s.client
            .try_deposit_sponsor_pool(&sponsor, &id, &5_000, &1_000_000, &1_000_000),
        Err(Ok(Error::CampaignNotActive))
    );
    // Note: try_approve_milestone against a Completed campaign is already tested in milestones_release_in_order_and_complete.
}

#[test]
fn approval_before_cooldown_gap_fails_with_error() {
    let s = Setup::new();
    let cooldown_secs = 3_600; // 1 hour
    let id = s.campaign_with_cooldown(cooldown_secs);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    // Initial last approval time should be 0
    assert_eq!(s.client.last_approval_time(&id), 0);
    assert_eq!(s.client.approval_cooldown(&id), cooldown_secs);

    // First milestone approval succeeds immediately
    let initial_time = s.env.ledger().timestamp();
    let released = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released, 300);

    // Last approval time is stored and visible
    assert_eq!(s.client.last_approval_time(&id), initial_time);
    assert_eq!(s.client.get_campaign(&id).last_approval_time, initial_time);

    // Immediate second milestone approval fails with ApprovalCooldownNotMet
    let res = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res, Err(Ok(Error::ApprovalCooldownNotMet)));

    // Approval before cooldown gap has passed (after 1800s) still fails
    s.env.ledger().set_timestamp(initial_time + 1_800);
    let res_halfway = s.client.try_approve_milestone(&id, &s.proof());
    assert_eq!(res_halfway, Err(Ok(Error::ApprovalCooldownNotMet)));

    // Timestamp jump to exactly the cooldown time allows approval
    s.env.ledger().set_timestamp(initial_time + cooldown_secs);
    let released_second = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released_second, 300);

    // Last approval time updated
    let second_time = initial_time + cooldown_secs;
    assert_eq!(s.client.last_approval_time(&id), second_time);
    assert_eq!(s.client.get_campaign(&id).last_approval_time, second_time);
}

#[test]
fn approval_cooldown_of_zero_behaves_as_normal() {
    let s = Setup::new();
    let id = s.campaign_with_cooldown(0);
    let donor = s.donor(1_000);
    s.client.donate(&donor, &id, &1_000);

    // Milestone 0 and 1 approved in the very same timestamp without error
    let released_first = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released_first, 300);

    let released_second = s.client.approve_milestone(&id, &s.proof());
    assert_eq!(released_second, 300);
}

#[test]
fn approval_cooldown_and_last_approval_time_return_campaign_not_found() {
    let s = Setup::new();
    assert_eq!(
        s.client.try_approval_cooldown(&999),
        Err(Ok(Error::CampaignNotFound))
    );
    assert_eq!(
        s.client.try_last_approval_time(&999),
        Err(Ok(Error::CampaignNotFound))
    );
}

#[test]
fn cancel_campaign_with_reason_by_creator_or_admin_succeeds() {
    let s = Setup::new();
    let id = s.campaign();
    let reason = String::from_str(&s.env, "ipfs://reason-severe-weather");

    // Stranger cannot cancel with reason
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&stranger, &id, &reason),
        Err(Ok(Error::Unauthorized))
    );

    // Creator can cancel with reason
    s.client
        .cancel_campaign_with_reason(&s.creator, &id, &reason);
    assert_eq!(s.client.get_campaign(&id).status, CampaignStatus::Cancelled);

    // Second cancel fails because campaign is already cancelled
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&s.creator, &id, &reason),
        Err(Ok(Error::CampaignNotActive))
    );

    // Admin can cancel another campaign with reason
    let id2 = s.campaign();
    let admin_reason = String::from_str(&s.env, "ipfs://safety-intervention");
    s.client
        .cancel_campaign_with_reason(&s.admin, &id2, &admin_reason);
    assert_eq!(
        s.client.get_campaign(&id2).status,
        CampaignStatus::Cancelled
    );

    // Existing cancel_campaign without reason still works
    let id3 = s.campaign();
    s.client.cancel_campaign(&s.creator, &id3);
    assert_eq!(
        s.client.get_campaign(&id3).status,
        CampaignStatus::Cancelled
    );
}

#[test]
fn cancel_campaign_with_reason_validates_uri_length_and_existence() {
    let s = Setup::new();
    let id = s.campaign();

    // 257-byte reason URI exceeds MAX_URI_LENGTH (256)
    let long_reason = String::from_str(
        &s.env,
        "https://example.com/long-cancel-reason-uri-exceeding-two-hundred-and-fifty-six-bytes-limit-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding",
    );
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&s.creator, &id, &long_reason),
        Err(Ok(Error::UriTooLong))
    );

    // Non-existent campaign
    let valid_reason = String::from_str(&s.env, "ipfs://reason");
    assert_eq!(
        s.client
            .try_cancel_campaign_with_reason(&s.creator, &999, &valid_reason),
        Err(Ok(Error::CampaignNotFound))
    );
}

#[test]
fn verifier_profile_uri_recorded_and_queried() {
    let s = Setup::new();
    let profile = String::from_str(&s.env, "https://redcross.org/verifier-profile");

    // Initially, profile is None
    assert_eq!(s.client.verifier_profile(&s.verifier), None);

    // Admin sets profile URI for registered verifier
    s.client.set_verifier_profile(&s.verifier, &profile);
    assert_eq!(
        s.client.verifier_profile(&s.verifier),
        Some(profile.clone())
    );

    // Admin can update profile URI
    let updated_profile = String::from_str(&s.env, "https://redcross.org/updated-profile");
    s.client.set_verifier_profile(&s.verifier, &updated_profile);
    assert_eq!(
        s.client.verifier_profile(&s.verifier),
        Some(updated_profile)
    );

    // Unset profile returns None for an unregistered address
    let stranger = Address::generate(&s.env);
    assert_eq!(s.client.verifier_profile(&stranger), None);
}

#[test]
fn verifier_profile_only_admin_and_only_registered_verifiers() {
    let s = Setup::new();
    let profile = String::from_str(&s.env, "https://example.org/profile");
    let stranger = Address::generate(&s.env);

    // Fails for unregistered verifier address
    assert_eq!(
        s.client.try_set_verifier_profile(&stranger, &profile),
        Err(Ok(Error::NotVerifier))
    );

    // Non-admin cannot set profile
    let auth = MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.client.address,
            fn_name: "set_verifier_profile",
            args: (s.verifier.clone(), profile.clone()).into_val(&s.env),
            sub_invokes: &[],
        },
    };
    let res = s
        .client
        .mock_auths(&[auth])
        .try_set_verifier_profile(&s.verifier, &profile);
    assert_eq!(res.unwrap_err(), Err(InvokeError::Abort));

    // Reject URI exceeding MAX_URI_LENGTH (256)
    let long_uri = String::from_str(
        &s.env,
        "https://example.com/very-long-verifier-profile-uri-exceeding-two-hundred-and-fifty-six-bytes-limit-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding-padding",
    );
    assert_eq!(
        s.client.try_set_verifier_profile(&s.verifier, &long_uri),
        Err(Ok(Error::UriTooLong))
    );
}

#[test]
fn donors_pagination_returns_stable_order_and_no_duplicates() {
    let s = Setup::new();
    let id = s.campaign();

    let d1 = s.donor(100);
    let d2 = s.donor(100);
    let d3 = s.donor(100);
    let d4 = s.donor(100);
    let d5 = s.donor(100);

    // Donate in order: d1, d2, d3, d4, d5
    s.client.donate(&d1, &id, &100);
    s.client.donate(&d2, &id, &100);
    s.client.donate(&d3, &id, &100);
    s.client.donate(&d4, &id, &100);
    s.client.donate(&d5, &id, &100);

    // Page 1: start 0, limit 2
    let page1 = s.client.donors(&id, &0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap(), d1);
    assert_eq!(page1.get(1).unwrap(), d2);

    // Page 2: start 2, limit 2
    let page2 = s.client.donors(&id, &2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap(), d3);
    assert_eq!(page2.get(1).unwrap(), d4);

    // Page 3: start 4, limit 2
    let page3 = s.client.donors(&id, &4, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap(), d5);

    // Out of bounds start
    let empty_page = s.client.donors(&id, &5, &2);
    assert_eq!(empty_page.len(), 0);

    // Limit of zero
    let zero_page = s.client.donors(&id, &0, &0);
    assert_eq!(zero_page.len(), 0);
}

#[test]
fn repeat_donors_are_not_added_twice() {
    let s = Setup::new();
    let id = s.campaign();

    let d1 = s.donor(300);
    let d2 = s.donor(300);
    let d3 = s.donor(300);

    // d1 donates first time
    s.client.donate(&d1, &id, &100);
    // d2 donates first time
    s.client.donate(&d2, &id, &100);
    // d1 repeat donation
    s.client.donate(&d1, &id, &100);
    // d2 repeat donation
    s.client.donate(&d2, &id, &100);
    // d3 donates first time
    s.client.donate(&d3, &id, &100);

    let all = s.client.donors(&id, &0, &50);
    assert_eq!(all.len(), 3);
    assert_eq!(all.get(0).unwrap(), d1);
    assert_eq!(all.get(1).unwrap(), d2);
    assert_eq!(all.get(2).unwrap(), d3);
}

#[test]
fn donors_limit_above_cap_is_clamped() {
    let s = Setup::new();
    let id = s.campaign();

    let d1 = s.donor(100);
    let d2 = s.donor(100);
    s.client.donate(&d1, &id, &50);
    s.client.donate(&d2, &id, &50);

    // Request limit = 100 which exceeds MAX_DONORS_PAGE_LIMIT (50).
    // Should be clamped gracefully and return all 2 donors without failure.
    let res = s.client.donors(&id, &0, &100);
    assert_eq!(res.len(), 2);
    assert_eq!(res.get(0).unwrap(), d1);
    assert_eq!(res.get(1).unwrap(), d2);

    // Non-existent campaign returns empty vector
    let none_res = s.client.donors(&999, &0, &50);
    assert_eq!(none_res.len(), 0);
}
