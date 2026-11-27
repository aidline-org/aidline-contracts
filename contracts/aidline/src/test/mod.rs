#![cfg(test)]

//! Unit tests for the Aidline contract, split into one module per area.
//!
//! Shared fixtures live here: [`Setup`] builds a fresh environment with a funded
//! token, an admin, a registered verifier and a creator/beneficiary pair.
//! Each area module pulls them in with `use super::*;`.

use proptest::prelude::*;
use soroban_sdk::{
    Address, Env, IntoVal, InvokeError, String, contract, contractimpl,
    testutils::{Address as _, Events as _, Ledger, MockAuth, MockAuthInvoke},
    token::{StellarAssetClient, TokenClient},
    vec,
};

use crate::{
    Aidline, AidlineClient, BondStatus, CampaignKind, CampaignStatus, Error, MIN_DONATION_AMOUNT,
};

/// Seconds in a day, used to build deadlines and bond delays.
const DAY: u64 = 86_400;

/// Test fixture: a deployed contract, its token client and the main actors.
struct Setup<'a> {
    env: Env,
    client: AidlineClient<'a>,
    token: TokenClient<'a>,
    admin: Address,
    verifier: Address,
    creator: Address,
    beneficiary: Address,
}

impl<'a> Setup<'a> {
    /// Deploys the contract with mocked auths, a stellar asset token and one registered verifier.
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(admin.clone());
        let token = TokenClient::new(&env, &sac.address());

        let contract_id = env.register(Aidline, (admin.clone(), sac.address()));
        let client = AidlineClient::new(&env, &contract_id);

        let verifier = Address::generate(&env);
        client.add_verifier(&admin, &verifier);

        Setup {
            creator: Address::generate(&env),
            beneficiary: Address::generate(&env),
            env,
            client,
            token,
            admin,
            verifier,
        }
    }
}

#[test]
#[should_panic(expected = "admin and token must be distinct")]
fn constructor_rejects_identical_admin_and_token() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = Address::generate(&env);
    env.register(Aidline, (addr.clone(), addr));
}

#[test]
fn constructor_initializes_storage_properly_with_distinct_addresses() {
    let s = Setup::new();
    assert_eq!(s.client.admin(), s.admin);
    assert!(s.client.is_token_allowed(&s.token.address));
}

impl<'a> Setup<'a> {
    /// A fresh address minted `balance` tokens.
    fn donor(&self, balance: i128) -> Address {
        let donor = Address::generate(&self.env);
        StellarAssetClient::new(&self.env, &self.token.address).mint(&donor, &balance);
        donor
    }

    /// Campaign with three milestones of 300, 300 and 400 (goal 1000).
    fn campaign(&self) -> u64 {
        self.client.create_campaign(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &self.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://flood-relief"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
        )
    }

    /// Campaign with three milestones of 300, 300 and 400 (goal 1000) and a minimum donation.
    fn campaign_with_min_donation(&self, min_donation: i128) -> u64 {
        self.client.create_campaign_min_donation(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &self.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://flood-relief"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
            &min_donation,
        )
    }

    /// Campaign with three milestones of 300, 300 and 400 (goal 1000) and an approval cooldown.
    fn campaign_with_cooldown(&self, cooldown: u64) -> u64 {
        self.client.create_campaign_with_cooldown(
            &self.creator,
            &self.beneficiary,
            &self.verifier,
            &self.token.address,
            &CampaignKind::Emergency,
            &String::from_str(&self.env, "ipfs://flood-relief"),
            &(self.env.ledger().timestamp() + 30 * DAY),
            &vec![&self.env, 300, 300, 400],
            &cooldown,
        )
    }

    /// A placeholder proof URI for milestone approvals.
    fn proof(&self) -> String {
        String::from_str(&self.env, "ipfs://proof")
    }

    /// Moves the ledger clock past a default 30 day campaign deadline.
    fn pass_deadline(&self) {
        let now = self.env.ledger().timestamp();
        self.env.ledger().set_timestamp(now + 31 * DAY);
    }
}

impl Setup<'_> {
    /// A fresh address holding `balance` tokens that has posted a bond of `bond`.
    fn bonded_verifier(&self, balance: i128, bond: i128) -> Address {
        let v = self.donor(balance);
        self.client.register_with_bond(&v, &bond);
        v
    }
}

mod admin;
mod admin_access;
mod bond_lock;
mod bonds;
mod budget;
mod campaigns;
mod cancel_vote;
mod challenge_window;
mod contract_address;
mod costs_and_events;
mod deadlines;
mod donations;
mod due_dates;
mod fast_track;
mod matching;
mod metadata;
mod min_donation;
mod multisig;
mod pause;
mod pledge_scope;
mod pledges;
mod property;
mod rbac;
mod refunds;
mod rollover;
mod schema;
mod slasher;
mod split_payouts;
mod streaming;
mod stretch;
mod timelock;
mod tokens;
mod ttl;
mod upgrade;
mod verifier_reassignment;
mod verifier_stats;
