use soroban_sdk::{Address, String, Vec, contracttype};

/// What a campaign is raising money for.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignKind {
    /// Disaster relief: floods, earthquakes, wildfires and similar emergencies.
    Emergency,
    /// Long running climate work: reforestation, clean water, renewable energy.
    Climate,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CampaignStatus {
    /// Accepting donations and milestone approvals until the deadline.
    Active,
    /// Every milestone has been approved and paid out.
    Completed,
    /// Stopped early by the creator or admin. Donors can claim refunds.
    Cancelled,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Campaign {
    pub id: u64,
    pub creator: Address,
    /// Receives funds each time a milestone is approved.
    pub beneficiary: Address,
    /// Verifier who must confirm each milestone before funds move.
    pub verifier: Address,
    pub kind: CampaignKind,
    /// Off chain metadata (title, description, images), usually an IPFS or HTTPS URI.
    pub metadata_uri: String,
    pub goal: i128,
    /// Unix timestamp in seconds. After this, no donations or approvals.
    pub deadline: u64,
    /// Amount released per milestone, in order. Sums to `goal`.
    pub milestones: Vec<i128>,
    pub milestones_released: u32,
    pub raised: i128,
    pub released: i128,
    pub status: CampaignStatus,
    // Issue #23 — emergency first-milestone advance.
    /// Amount already paid to the beneficiary as an emergency advance against
    /// milestone 0.  Zero for non-Emergency campaigns and for Emergency
    /// campaigns that have not yet used the fast track.
    pub emergency_advance: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Token,
    CampaignCount,
    Verifier(Address),
    Campaign(u64),
    Contribution(u64, Address),
    // Issue #26 — per-milestone due dates
    MilestoneDueDates(u64),
}
