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

/// A sponsor matching pool for a specific campaign.
///
/// The sponsor deposits `pool` tokens. For each eligible donation of `d` tokens
/// the contract computes:
///   `match = min(d * ratio_bps / 10_000, remaining_cap, remaining_pool)`
/// and transfers `match` from the contract's held pool to the campaign's
/// raised total. The matched amount is credited on behalf of the sponsor
/// so the sponsor's share of any pro‑rata refund is properly tracked.
///
/// After the campaign ends (completed, cancelled, or expired) the sponsor
/// can call `return_sponsor_pool` to withdraw whatever remains.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SponsorPool {
    /// The address that deposited the pool.
    pub sponsor: Address,
    /// Campaign this pool is attached to.
    pub campaign_id: u64,
    /// Matching ratio in basis points (e.g. 5000 = 50 %, 10000 = 100 %).
    pub ratio_bps: u32,
    /// Maximum total matching the sponsor will provide, in token units.
    pub cap: i128,
    /// Tokens actually deposited into the pool by the sponsor.
    pub deposited: i128,
    /// Tokens still available for matching (deposited minus already matched).
    pub remaining: i128,
    /// Total tokens already matched and added to the campaign's raised amount.
    pub matched: i128,
}

/// Bond status for verifier bond accounting (Issue #29).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BondStatus {
    /// Bond is active — verifier is currently registered.
    Active,
    /// Verifier has deregistered; withdrawal delay is counting down.
    PendingWithdrawal,
    /// Bond has been fully withdrawn by the verifier.
    Withdrawn,
    /// Bond has been fully slashed by the admin.
    Slashed,
}

/// Persistent record of a verifier's bond (Issue #29).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierBond {
    pub verifier: Address,
    /// Amount originally posted.
    pub amount: i128,
    /// Amount still available (not yet slashed).
    pub remaining: i128,
    pub status: BondStatus,
    /// Ledger timestamp (seconds) when deregister was called; 0 while active.
    pub deregistered_at: u64,
}

/// A donor pledge: funds to be pulled when a milestone is approved (Issue #30).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pledge {
    pub pledge_id: u64,
    pub donor: Address,
    pub campaign_id: u64,
    /// Amount the donor pledged in total.
    pub pledged_amount: i128,
    /// Amount already pulled from this pledge.
    pub pulled_amount: i128,
    /// Whether the pledge is still active (not fully consumed or cancelled).
    pub active: bool,
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
    // Issue #27 — sponsor matching pools
    SponsorPool(u64, Address),
    // Issue #29 — verifier bonds
    VerifierBond(Address),
    BondRequirement,
    BondWithdrawDelay,
    // Issue #30 — pledges
    PledgeCount,
    Pledge(u64),
    // Issue #26 — per-milestone due dates
    MilestoneDueDates(u64),
}
