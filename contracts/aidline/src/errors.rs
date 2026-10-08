use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    Unauthorized = 1,
    CampaignNotFound = 2,
    DeadlineInPast = 3,
    InvalidMilestones = 4,
    InvalidAmount = 5,
    CampaignNotActive = 6,
    CampaignExpired = 7,
    GoalExceeded = 8,
    NotVerifier = 9,
    MilestoneNotFunded = 10,
    NoMilestonesLeft = 11,
    RefundNotAvailable = 12,
    NothingToRefund = 13,

    // Issue #27: sponsor matching pools
    SponsorPoolExists = 14,
    SponsorPoolNotFound = 15,
    InvalidMatchingConfig = 16,
    PoolNotReturnable = 17,

    // Issue #29: verifier bonds
    BondRequired = 18,
    BondNotFound = 19,
    BondNotWithdrawable = 20,
    WithdrawDelayNotMet = 21,
    SlashExceedsBond = 22,

    // Issue #30: pledges
    InvalidPledge = 23,

    // Issue #23: emergency fast track
    NotEmergencyCampaign = 24,
    AdvanceAlreadyTaken = 25,
    AdvanceExceedsEscrow = 26,
}
