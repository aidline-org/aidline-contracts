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
    /// Active campaign whose deadline has passed (computed).
    Expired,
}

/// Structured reason codes for a verifier rejecting a pending milestone (Issue #161).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum RejectionReason {
    /// Submitted deliverables are incomplete or do not meet milestone scope.
    IncompleteWork = 1,
    /// Fraudulent activity or falsified evidence detected; triggers campaign cancellation.
    FraudDetected = 2,
    /// Provided proof is insufficient to verify milestone completion.
    InsufficientProof = 3,
    /// Reported expenditures exceed the milestone budget or allocation.
    BudgetExceeded = 4,
}

impl RejectionReason {
    /// Converts a numeric reason code into a [`RejectionReason`] variant.
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::IncompleteWork),
            2 => Some(Self::FraudDetected),
            3 => Some(Self::InsufficientProof),
            4 => Some(Self::BudgetExceeded),
            _ => None,
        }
    }
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
    /// Token (Stellar Asset Contract) this campaign raises and pays out in (Issue #10).
    pub token: Address,
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
    // Issue #23: emergency first-milestone advance.
    /// Amount already paid to the beneficiary as an emergency advance against
    /// milestone 0.  Zero for non-Emergency campaigns and for Emergency
    /// campaigns that have not yet used the fast track.
    pub emergency_advance: i128,
    // Issue #9: multisig verification
    /// Authorized verifiers who can approve milestones for this campaign.
    pub verifiers: Vec<Address>,
    /// Minimum number of independent verifier approvals needed to release a milestone.
    pub threshold: u32,
    /// Whether the campaign deadline has already been extended (one-time only).
    pub deadline_extended: bool,
    /// Optional minimum donation floor. Zero means no floor.
    pub min_donation: i128,
    /// Number of times this campaign's verifier has been reassigned.
    pub verifier_reassignments: u32,
    /// Minimum gap (in seconds) between consecutive milestone approvals (0 = no cooldown).
    pub approval_cooldown: u64,
    /// Ledger timestamp of the last milestone approval/release (0 initially).
    pub last_approval_time: u64,
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

/// A milestone approval waiting out its challenge window (Issue #115).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingApproval {
    pub index: u32,
    pub proof_uri: String,
    pub proposed_at: u64,
    /// Earliest timestamp at which `finalize_milestone` may release the funds.
    pub ready_at: u64,
    /// Contribution weight of the donors who have voted to block it.
    pub block_weight: i128,
    /// Distinguishes this proposal's donor votes from an earlier, blocked one.
    pub nonce: u32,
}

/// A sensitive admin action that must wait out the timelock before it runs (Issue #113).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimelockedAction {
    /// `reassign_verifier(campaign_id, new_verifier)`
    ReassignVerifier(u64, Address),
    /// `slash_verifier(verifier, campaign_id, slash_amount)`
    SlashVerifier(Address, u64, i128),
    /// `set_bond_requirement(amount)`
    SetBondRequirement(i128),
    /// `set_bond_withdraw_delay(delay)`
    SetBondWithdrawDelay(u64),
}

/// An action waiting in the timelock queue.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuedAction {
    pub action: TimelockedAction,
    /// Earliest ledger timestamp (seconds) at which anyone may execute the action.
    pub eta: u64,
}

/// A milestone payout that vests linearly after approval (Issue #117).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Stream {
    /// Amount that will vest in total (shrinks to the vested amount on cancel).
    pub total: i128,
    /// Ledger timestamp (seconds) at which vesting started: the approval.
    pub start: u64,
    /// Seconds over which `total` vests. `0` means fully vested.
    pub duration: u64,
    /// Amount the beneficiary has already claimed.
    pub claimed: i128,
}

/// Running record of a verifier's work, kept on chain (Issue #119). One fixed
/// size entry per verifier, however many campaigns they have handled.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifierStats {
    /// Campaigns this verifier has been assigned to, including by reassignment.
    pub campaigns_assigned: u32,
    /// Approvals this verifier has recorded for milestones.
    pub milestones_approved: u32,
    /// Campaigns that completed while this verifier was assigned.
    pub campaigns_completed: u32,
    /// Campaigns cancelled while this verifier was assigned.
    pub campaigns_cancelled: u32,
    /// Total bond amount slashed from this verifier.
    pub total_slashed: i128,
}

/// One recipient of a split payout and its share in basis points (Issue #116).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Beneficiary {
    pub to: Address,
    pub bps: u32,
}

/// Bond status for verifier bond accounting (Issue #29).
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BondStatus {
    /// Bond is active, verifier is currently registered.
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    Admin,
    VerifierManager,
    Slasher,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Token,
    CampaignCount,
    Verifier(Address),
    Role(Role, Address),
    Campaign(u64),
    Contribution(u64, Address),
    // Issue #27: sponsor matching pools
    SponsorPool(u64, Address),
    // Issue #29: verifier bonds
    VerifierBond(Address),
    // Issue #140: historical verifier bonds: (verifier, nonce) -> VerifierBond
    HistoricalVerifierBond(Address, u32),
    VerifierBondNonce(Address),
    BondRequirement,
    BondWithdrawDelay,
    // Issue #30: pledges
    PledgeCount,
    Pledge(u64),
    // Issue #26: per-milestone due dates
    MilestoneDueDates(u64),
    // Issue #9: milestone approvals: (campaign_id, milestone_index) -> Vec<Address>
    MilestoneApprovals(u64, u32),
    // Issue #172: milestone approval proof URIs: (campaign_id, milestone_index) -> Vec<String>
    MilestoneApprovalProofs(u64, u32),
    // Issue #10: token allowlist
    AllowedToken(Address),
    // Emergency pause switch
    Paused,
    // Platform fee and treasury
    PlatformFee,
    Treasury,
    // Two-step admin transfer
    PendingAdmin,
    // Verifier profile URI
    VerifierProfile(Address),
    // Paginated donors list
    CampaignDonors(u64),
    // Issue #111: address allowed to slash verifier bonds (instance storage)
    Slasher,
    // Issue #115: challenge window between proposing and paying a milestone
    ChallengeWindow(u64),
    PendingApproval(u64),
    ProposalNonce(u64),
    BlockVote(u64, u32, Address),
    // Issue #128: donated amount not yet matched by a sponsor, per campaign and donor
    UnmatchedDonation(u64, Address),
    // Issue #113: timelock queue
    NextActionId,
    QueuedAction(u64),
    // Issue #120: number of stretch milestones at the end of a campaign's milestone list
    StretchCount(u64),
    // Slashed verifier bonds awaiting pro-rata refund to donors
    SlashedRecovery(u64),
    // Issue #125: ids of the pledges made to one campaign
    CampaignPledges(u64),
    // Issue #129: campaigns a verifier is assigned to that are not yet completed or cancelled
    VerifierActiveCampaigns(Address),
    // Issue #114: donor votes to cancel a stalled campaign
    CancelVote(u64, Address),
    CancelVoteTotal(u64),
    CampaignStart(u64),
    // Issue #117: per-milestone streaming release
    MilestoneStreamDays(u64),
    Stream(u64, u32),
    // Issue #116: beneficiaries of a split-payout campaign
    Beneficiaries(u64),
    // Issue #118: version of the stored data layout (instance storage)
    SchemaVersion,
    // Issue #119: per-verifier track record
    VerifierStats(Address),
}
