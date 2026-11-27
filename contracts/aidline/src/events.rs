#![allow(deprecated)]

use soroban_sdk::{Address, BytesN, Env, String, symbol_short};

use crate::types::{CampaignKind, Role};

pub struct CampaignCreated {
    pub campaign_id: u64,
    pub creator: Address,
    pub kind: CampaignKind,
    pub goal: i128,
    pub deadline: u64,
    pub min_donation: i128,
}

impl CampaignCreated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("created"), self.campaign_id),
            (
                self.creator.clone(),
                self.kind,
                self.goal,
                self.deadline,
                self.min_donation,
            ),
        );
    }
}

pub struct Donated {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
}

impl Donated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("donated"),
                self.campaign_id,
                self.donor.clone(),
            ),
            self.amount,
        );
    }
}

pub struct MilestoneReleased {
    pub campaign_id: u64,
    pub index: u32,
    pub amount: i128,
    pub fee: i128,
    pub proof_uri: String,
}

impl MilestoneReleased {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("released"), self.campaign_id, self.index),
            (self.amount, self.fee, self.proof_uri.clone()),
        );
    }
}

pub struct CampaignCancelled {
    pub campaign_id: u64,
}

impl CampaignCancelled {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("canceled"), self.campaign_id), ());
    }
}

pub struct CampaignCancelledWithReason {
    pub campaign_id: u64,
    pub caller: Address,
    pub reason_uri: String,
}

impl CampaignCancelledWithReason {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("canceled"), self.campaign_id),
            (self.caller.clone(), self.reason_uri.clone()),
        );
    }
}

pub struct Refunded {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
}

impl Refunded {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("refunded"),
                self.campaign_id,
                self.donor.clone(),
            ),
            self.amount,
        );
    }
}

pub struct VerifierUpdated {
    pub verifier: Address,
    pub active: bool,
}

impl VerifierUpdated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("verifier"), self.verifier.clone()),
            self.active,
        );
    }
}

pub struct VerifierProfileUpdated {
    pub verifier: Address,
    pub profile_uri: String,
}

impl VerifierProfileUpdated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("vf_prof"), self.verifier.clone()),
            self.profile_uri.clone(),
        );
    }
}

// ─── Admin change events ─────────────────────────────────────────────────────

/// Emitted when the admin transfers control to a new address.
pub struct AdminChanged {
    pub old: Address,
    pub new: Address,
}

impl AdminChanged {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("adm_chng"), self.old.clone()),
            self.new.clone(),
        );
    }
}

/// Emitted when an admin nominates a new address for two-step admin transfer.
pub struct AdminProposed {
    pub current_admin: Address,
    pub pending_admin: Address,
}

impl AdminProposed {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("adm_prop"), self.current_admin.clone()),
            self.pending_admin.clone(),
        );
    }
}

/// Emitted when the current admin nominates a candidate for two-step admin transfer.
pub struct AdminNominated {
    pub current_admin: Address,
    pub pending_admin: Address,
}

impl AdminNominated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("adm_nom"), self.current_admin.clone()),
            self.pending_admin.clone(),
        );
    }
}

/// Emitted when the pending admin accepts the transfer.
pub struct AdminAccepted {
    pub old_admin: Address,
    pub new_admin: Address,
}

impl AdminAccepted {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("adm_acc"), self.old_admin.clone()),
            self.new_admin.clone(),
        );
    }
}

// ─── Issue #4: Verifier reassignment event ────────────────────────────────────

/// Emitted when the admin reassigns a campaign to a different verifier.
pub struct VerifierReassigned {
    pub campaign_id: u64,
    pub old_verifier: Address,
    pub new_verifier: Address,
}

impl VerifierReassigned {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("vf_reass"), self.campaign_id),
            (self.old_verifier.clone(), self.new_verifier.clone()),
        );
    }
}

// ─── Issue #27: Sponsor matching pools ────────────────────────────────────────

pub struct SponsorPoolDeposited {
    pub campaign_id: u64,
    pub sponsor: Address,
    pub amount: i128,
    pub ratio_bps: u32,
    pub cap: i128,
}

impl SponsorPoolDeposited {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("sp_depos"),
                self.campaign_id,
                self.sponsor.clone(),
            ),
            (self.amount, self.ratio_bps, self.cap),
        );
    }
}

pub struct MatchingApplied {
    pub campaign_id: u64,
    pub sponsor: Address,
    pub donor: Address,
    pub donation_amount: i128,
    pub matched_amount: i128,
}

impl MatchingApplied {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("sp_match"),
                self.campaign_id,
                self.sponsor.clone(),
            ),
            (
                self.donor.clone(),
                self.donation_amount,
                self.matched_amount,
            ),
        );
    }
}

pub struct SponsorPoolReturned {
    pub campaign_id: u64,
    pub sponsor: Address,
    pub amount: i128,
}

impl SponsorPoolReturned {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("sp_ret"),
                self.campaign_id,
                self.sponsor.clone(),
            ),
            self.amount,
        );
    }
}

// ─── Issue #29: Verifier bonds ────────────────────────────────────────────────

pub struct VerifierBondPosted {
    pub verifier: Address,
    pub amount: i128,
}

impl VerifierBondPosted {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("vb_post"), self.verifier.clone()),
            self.amount,
        );
    }
}

pub struct VerifierDeregistered {
    pub verifier: Address,
    pub deregistered_at: u64,
}

impl VerifierDeregistered {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("vb_dereg"), self.verifier.clone()),
            self.deregistered_at,
        );
    }
}

pub struct BondWithdrawn {
    pub verifier: Address,
    pub amount: i128,
}

impl BondWithdrawn {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("vb_withd"), self.verifier.clone()),
            self.amount,
        );
    }
}

pub struct BondSlashed {
    pub verifier: Address,
    pub campaign_id: u64,
    pub slashed_amount: i128,
}

impl BondSlashed {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("vb_slash"),
                self.verifier.clone(),
                self.campaign_id,
            ),
            self.slashed_amount,
        );
    }
}

// ─── Issue #30: Pledges ───────────────────────────────────────────────────────

pub struct PledgeCreated {
    pub pledge_id: u64,
    pub campaign_id: u64,
    pub donor: Address,
    pub pledged_amount: i128,
}

impl PledgeCreated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("pl_creat"),
                self.campaign_id,
                self.donor.clone(),
            ),
            (self.pledge_id, self.pledged_amount),
        );
    }
}

pub struct PledgeCancelled {
    pub pledge_id: u64,
    pub campaign_id: u64,
    pub donor: Address,
}

impl PledgeCancelled {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("pl_canc"),
                self.campaign_id,
                self.donor.clone(),
            ),
            self.pledge_id,
        );
    }
}

pub struct PledgePulled {
    pub pledge_id: u64,
    pub campaign_id: u64,
    pub donor: Address,
    pub pulled_amount: i128,
}

impl PledgePulled {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("pl_pull"),
                self.campaign_id,
                self.donor.clone(),
            ),
            (self.pledge_id, self.pulled_amount),
        );
    }
}

pub struct PledgeSkipped {
    pub pledge_id: u64,
    pub campaign_id: u64,
    pub donor: Address,
    pub reason: u32,
}

impl PledgeSkipped {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("pl_skip"),
                self.campaign_id,
                self.donor.clone(),
            ),
            (self.pledge_id, self.reason),
        );
    }
}

pub struct PledgeSettlement {
    pub campaign_id: u64,
    pub milestone_index: u32,
    pub total_pledged_pulled: i128,
}

impl PledgeSettlement {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("pl_settl"),
                self.campaign_id,
                self.milestone_index,
            ),
            self.total_pledged_pulled,
        );
    }
}

// ─── Issue #23: Emergency fast-track event ───────────────────────────────────

/// Emitted when a verifier releases an emergency advance for the first
/// milestone of an Emergency campaign. Indexers use this to track that the
/// advance has been paid and to deduct it from the subsequent normal
/// milestone release.
pub struct EmergencyAdvanceReleased {
    pub campaign_id: u64,
    pub verifier: Address,
    /// Milestone index the advance is charged against (always 0).
    pub milestone_index: u32,
    /// Amount transferred to the beneficiary as the advance.
    pub amount: i128,
}

impl EmergencyAdvanceReleased {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("em_adv"),
                self.campaign_id,
                self.verifier.clone(),
            ),
            (self.milestone_index, self.amount),
        );
    }
}

// ─── Issue #114: donor votes to cancel ───────────────────────────────────────

/// Emitted when a donor votes to cancel a campaign.
pub struct CancelVoted {
    pub campaign_id: u64,
    pub donor: Address,
    /// The donor's contribution, which is their vote's weight.
    pub weight: i128,
    /// Total weight of all votes after this one.
    pub total: i128,
}

impl CancelVoted {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("cvote"), self.campaign_id, self.donor.clone()),
            (self.weight, self.total),
        );
    }
}

/// Emitted when a donor withdraws their cancel vote.
pub struct CancelVoteWithdrawn {
    pub campaign_id: u64,
    pub donor: Address,
    pub weight: i128,
    pub total: i128,
}

impl CancelVoteWithdrawn {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("cvote_wd"),
                self.campaign_id,
                self.donor.clone(),
            ),
            (self.weight, self.total),
        );
    }
}

// ─── Issue #117: streaming release ───────────────────────────────────────────

/// Emitted when the beneficiary claims what has vested from streamed milestones.
pub struct StreamClaimed {
    pub campaign_id: u64,
    pub amount: i128,
}

impl StreamClaimed {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("claimed"), self.campaign_id), self.amount);
    }
}

// ─── Issue #115: challenge window ────────────────────────────────────────────

/// Emitted when a milestone approval is proposed and starts its challenge window.
pub struct MilestoneProposed {
    pub campaign_id: u64,
    pub index: u32,
    pub ready_at: u64,
}

impl MilestoneProposed {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("proposed"), self.campaign_id, self.index),
            self.ready_at,
        );
    }
}

/// Emitted when a proposed approval is blocked and returns to pending.
pub struct MilestoneBlocked {
    pub campaign_id: u64,
    pub index: u32,
    /// The admin, or the donor whose vote reached the quorum.
    pub by: Address,
}

impl MilestoneBlocked {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("blocked"), self.campaign_id, self.index),
            self.by.clone(),
        );
    }
}

/// Emitted when a proposed approval survives its window and is finalized.
pub struct MilestoneFinalized {
    pub campaign_id: u64,
    pub index: u32,
}

impl MilestoneFinalized {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("finalized"), self.campaign_id, self.index),
            (),
        );
    }
}

// ─── Issue #111: slasher role ────────────────────────────────────────────────

/// Emitted when the admin changes who may slash verifier bonds.
pub struct SlasherChanged {
    pub old: Address,
    pub new: Address,
}

impl SlasherChanged {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("slasher"),),
            (self.old.clone(), self.new.clone()),
        );
    }
}

// ─── Issue #118: storage schema migration ────────────────────────────────────

/// Emitted by every `migrate` call. `from == to` when nothing was pending.
pub struct Migrated {
    pub from: u32,
    pub to: u32,
}

impl Migrated {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("migrated"),), (self.from, self.to));
    }
}

// ─── Issue #116: split payouts ───────────────────────────────────────────────

/// Emitted once per beneficiary when a split-payout campaign pays out a
/// milestone or the emergency advance.
pub struct BeneficiaryPaid {
    pub campaign_id: u64,
    pub recipient: Address,
    pub amount: i128,
}

impl BeneficiaryPaid {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("paid"),
                self.campaign_id,
                self.recipient.clone(),
            ),
            self.amount,
        );
    }
}

// ─── Issue #9: Multisig milestone approved event ───────────────────────────

/// Emitted when a verifier records their approval for a milestone.
pub struct MilestoneApproved {
    pub campaign_id: u64,
    pub milestone_index: u32,
    pub verifier: Address,
    pub approvals_count: u32,
    pub threshold: u32,
}

impl MilestoneApproved {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (
                symbol_short!("approved"),
                self.campaign_id,
                self.milestone_index,
            ),
            (self.verifier.clone(), self.approvals_count, self.threshold),
        );
    }
}

pub struct MilestoneRejected {
    pub campaign_id: u64,
    pub index: u32,
    pub reason: u32,
    pub details_uri: String,
}

impl MilestoneRejected {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("rejected"), self.campaign_id, self.index),
            (self.reason, self.details_uri.clone()),
        );
    }
}

// ─── Issue #10: Token allowlist updated event ───────────────────────────────

/// Emitted when the admin adds or removes a token from the campaign allowlist.
pub struct TokenAllowlistUpdated {
    pub token: Address,
    pub allowed: bool,
}

impl TokenAllowlistUpdated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("tok_allow"), self.token.clone()),
            self.allowed,
        );
    }
}

// ─── Emergency pause events ──────────────────────────────────────────────────

pub struct ContractPaused {
    pub admin: Address,
}

impl ContractPaused {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("paused"), self.admin.clone()), ());
    }
}

pub struct ContractUnpaused {
    pub admin: Address,
}

impl ContractUnpaused {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("unpaused"), self.admin.clone()), ());
    }
}

// ─── Upgrade event ───────────────────────────────────────────────────────────

pub struct ContractUpgraded {
    pub new_wasm_hash: BytesN<32>,
}

impl ContractUpgraded {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("upgraded"),), self.new_wasm_hash.clone());
    }
}

// ─── Metadata update event ───────────────────────────────────────────────────

pub struct MetadataUpdated {
    pub campaign_id: u64,
    pub new_metadata_uri: String,
}

impl MetadataUpdated {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("meta_upd"), self.campaign_id),
            self.new_metadata_uri.clone(),
        );
    }
}

// ─── Deadline extension event ────────────────────────────────────────────────

pub struct DeadlineExtended {
    pub campaign_id: u64,
    pub old_deadline: u64,
    pub new_deadline: u64,
}

impl DeadlineExtended {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("dl_ext"), self.campaign_id),
            (self.old_deadline, self.new_deadline),
        );
    }
}

// ─── Platform fee events ─────────────────────────────────────────────────────

pub struct PlatformFeeUpdated {
    pub fee_bps: u32,
}

impl PlatformFeeUpdated {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("fee_upd"),), self.fee_bps);
    }
}

pub struct TreasuryUpdated {
    pub treasury: Address,
}

impl TreasuryUpdated {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("tres_upd"),), self.treasury.clone());
    }
}

// ─── Issue #178: RBAC events ──────────────────────────────────────────────────

pub struct RoleGranted {
    pub role: Role,
    pub account: Address,
}

impl RoleGranted {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("role_grnt"), self.role),
            self.account.clone(),
        );
    }
}

pub struct RoleRevoked {
    pub role: Role,
    pub account: Address,
}

impl RoleRevoked {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("role_rvk"), self.role), self.account.clone());
    }
}

// ─── Timelock events (issue #113) ────────────────────────────────────────────

pub struct ActionQueued {
    pub action_id: u64,
    pub eta: u64,
}

impl ActionQueued {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("tl_queue"), self.action_id), self.eta);
    }
}

pub struct ActionCancelled {
    pub action_id: u64,
}

impl ActionCancelled {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("tl_cancel"), self.action_id), ());
    }
}

pub struct ActionExecuted {
    pub action_id: u64,
}

impl ActionExecuted {
    pub fn publish(&self, env: &Env) {
        env.events()
            .publish((symbol_short!("tl_exec"), self.action_id), ());
    }
}
