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

// ─── Issue #23: Emergency fast-track event ───────────────────────────────────

/// Emitted when a verifier releases an emergency advance for the first
/// milestone of an Emergency campaign.  Indexers use this to track that the
/// advance has been paid and to deduct it from the subsequent normal
/// milestone-one release.
#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyAdvanceReleased {
    #[topic]
    pub campaign_id: u64,
    #[topic]
    pub verifier: Address,
    /// Milestone index the advance is charged against (always 0).
    pub milestone_index: u32,
    /// Amount transferred to the beneficiary as the advance.
    pub amount: i128,
}
