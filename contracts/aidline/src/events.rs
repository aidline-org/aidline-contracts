use soroban_sdk::{Address, String, contractevent};

use crate::types::CampaignKind;

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCreated {
    #[topic]
    pub campaign_id: u64,
    pub creator: Address,
    pub kind: CampaignKind,
    pub goal: i128,
    pub deadline: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Donated {
    #[topic]
    pub campaign_id: u64,
    #[topic]
    pub donor: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MilestoneReleased {
    #[topic]
    pub campaign_id: u64,
    pub index: u32,
    pub amount: i128,
    pub proof_uri: String,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CampaignCancelled {
    #[topic]
    pub campaign_id: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refunded {
    #[topic]
    pub campaign_id: u64,
    #[topic]
    pub donor: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierUpdated {
    #[topic]
    pub verifier: Address,
    pub active: bool,
}

// ─── Issue #27: Sponsor matching events ───────────────────────────────────────

/// Emitted when a sponsor creates or tops up a matching pool.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SponsorPoolDeposited {
    #[topic]
    pub campaign_id: u64,
    #[topic]
    pub sponsor: Address,
    pub amount: i128,
    pub ratio_bps: u32,
    pub cap: i128,
}

/// Emitted each time matching funds are applied to a donation.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MatchingApplied {
    #[topic]
    pub campaign_id: u64,
    #[topic]
    pub sponsor: Address,
    pub donor: Address,
    pub donation_amount: i128,
    pub matched_amount: i128,
}

/// Emitted when a sponsor retrieves their unused matching pool.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SponsorPoolReturned {
    #[topic]
    pub campaign_id: u64,
    #[topic]
    pub sponsor: Address,
    pub amount: i128,
}

// ─── Issue #29: Verifier bond events ─────────────────────────────────────────

/// Emitted when a verifier posts a bond during registration.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierBondPosted {
    #[topic]
    pub verifier: Address,
    pub amount: i128,
}

/// Emitted when a verifier deregisters and starts the withdrawal delay.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierDeregistered {
    #[topic]
    pub verifier: Address,
    pub deregistered_at: u64,
}

/// Emitted when a verifier withdraws their bond after the delay.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BondWithdrawn {
    #[topic]
    pub verifier: Address,
    pub amount: i128,
}

/// Emitted when the admin slashes a verifier bond.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BondSlashed {
    #[topic]
    pub verifier: Address,
    pub campaign_id: u64,
    pub slashed_amount: i128,
}

// ─── Issue #30: Pledge events ─────────────────────────────────────────────────

/// Emitted when a donor creates a new pledge.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PledgeCreated {
    #[topic]
    pub pledge_id: u64,
    #[topic]
    pub campaign_id: u64,
    pub donor: Address,
    pub pledged_amount: i128,
}

/// Emitted when pledge funds are successfully pulled during milestone approval.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PledgePulled {
    #[topic]
    pub pledge_id: u64,
    #[topic]
    pub campaign_id: u64,
    pub donor: Address,
    pub pulled_amount: i128,
}

/// Emitted when a pledge is skipped because the allowance is unavailable.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PledgeSkipped {
    #[topic]
    pub pledge_id: u64,
    #[topic]
    pub campaign_id: u64,
    pub donor: Address,
    pub reason: u32,
}

/// Emitted when milestone approval settles pledges and records total collected.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PledgeSettlement {
    #[topic]
    pub campaign_id: u64,
    pub milestone_index: u32,
    pub total_pledged_pulled: i128,
}
