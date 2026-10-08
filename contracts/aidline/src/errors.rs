use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    CampaignNotFound = 1,
    CampaignNotActive = 2,
    CampaignExpired = 3,
    InvalidAmount = 4,
    InvalidMilestones = 5,
    DeadlineInPast = 6,
    NotVerifier = 7,
    GoalExceeded = 8,
    MilestoneNotFunded = 9,
    NoMilestonesLeft = 10,
    RefundNotAvailable = 11,
    NothingToRefund = 12,
    Unauthorized = 13,
    // Issue #27 — sponsor matching
    SponsorPoolExists = 14,
    SponsorPoolNotFound = 15,
    InvalidMatchingConfig = 16,
    PoolNotReturnable = 17,
    // Issue #29 — verifier bonds
    BondRequired = 18,
    BondNotFound = 19,
    BondAlreadySlashed = 20,
    SlashExceedsBond = 21,
    WithdrawDelayNotMet = 22,
    BondNotWithdrawable = 23,
    // Issue #30 — pledges
    PledgeNotFound = 24,
    PledgeNotActive = 25,
    InvalidPledge = 26,
}
