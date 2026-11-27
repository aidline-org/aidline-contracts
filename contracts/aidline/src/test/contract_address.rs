// ─── Issues #179 & #180: reject contract address as beneficiary or verifier ───

use super::*;

#[test]
fn create_campaign_rejects_contract_address_as_beneficiary() {
    let s = Setup::new();
    let contract_addr = s.client.address.clone();
    let future = s.env.ledger().timestamp() + DAY;
    let uri = String::from_str(&s.env, "ipfs://relief");

    let res = s.client.try_create_campaign(
        &s.creator,
        &contract_addr,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::Unauthorized)));

    // Split-payout campaign also rejects contract address in any beneficiary slot
    let split = vec![
        &s.env,
        crate::Beneficiary {
            to: s.beneficiary.clone(),
            bps: 5_000,
        },
        crate::Beneficiary {
            to: contract_addr,
            bps: 5_000,
        },
    ];
    let split_res = s.client.try_create_campaign_beneficiaries(
        &s.creator,
        &split,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(split_res, Err(Ok(Error::Unauthorized)));

    // External beneficiary succeeds
    let ok_res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &s.verifier,
        &s.token.address,
        &CampaignKind::Emergency,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert!(ok_res.is_ok());
}

#[test]
fn rejects_contract_address_as_verifier() {
    let s = Setup::new();
    let contract_addr = s.client.address.clone();
    let future = s.env.ledger().timestamp() + DAY;
    let uri = String::from_str(&s.env, "ipfs://relief");

    // Admin cannot register the contract itself as a verifier
    assert_eq!(
        s.client.try_add_verifier(&s.admin, &contract_addr),
        Err(Ok(Error::Unauthorized))
    );
    assert!(!s.client.is_verifier(&contract_addr));

    // Contract cannot register itself with a bond
    s.client.set_bond_requirement(&100);
    assert_eq!(
        s.client.try_register_with_bond(&contract_addr, &100),
        Err(Ok(Error::Unauthorized))
    );

    // Campaign creation rejects contract address as verifier
    let res = s.client.try_create_campaign(
        &s.creator,
        &s.beneficiary,
        &contract_addr,
        &s.token.address,
        &CampaignKind::Emergency,
        &uri,
        &future,
        &vec![&s.env, 100],
    );
    assert_eq!(res, Err(Ok(Error::NotVerifier)));

    // Reassigning verifier to contract address is also rejected
    let id = s.campaign();
    assert_eq!(
        s.client.try_reassign_verifier(&id, &contract_addr),
        Err(Ok(Error::NotVerifier))
    );
}
