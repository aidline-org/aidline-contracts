use soroban_sdk::{Address, Env, String, symbol_short};

use crate::types::CampaignKind;

pub struct CampaignCreated {
    pub campaign_id: u64,
    pub creator: Address,
    pub kind: CampaignKind,
    pub goal: i128,
    pub deadline: u64,
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
            (symbol_short!("donated"), self.campaign_id, self.donor.clone()),
            self.amount,
        );
    }
}

pub struct MilestoneReleased {
    pub campaign_id: u64,
    pub index: u32,
    pub amount: i128,
    pub proof_uri: String,
}

impl MilestoneReleased {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("released"), self.campaign_id, self.index),
            (self.amount, self.proof_uri.clone()),
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

pub struct Refunded {
    pub campaign_id: u64,
    pub donor: Address,
    pub amount: i128,
}

impl Refunded {
    pub fn publish(&self, env: &Env) {
        env.events().publish(
            (symbol_short!("refunded"), self.campaign_id, self.donor.clone()),
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
        env.events()
            .publish((symbol_short!("verifier"), self.verifier.clone()), self.active);
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
            (symbol_short!("sp_depos"), self.campaign_id, self.sponsor.clone()),
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
            (symbol_short!("sp_match"), self.campaign_id, self.sponsor.clone()),
            (self.donor.clone(), self.donation_amount, self.matched_amount),
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
            (symbol_short!("sp_ret"), self.campaign_id, self.sponsor.clone()),
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
            (symbol_short!("vb_slash"), self.verifier.clone(), self.campaign_id),
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
            (symbol_short!("pl_creat"), self.campaign_id, self.donor.clone()),
            (self.pledge_id, self.pledged_amount),
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
            (symbol_short!("pl_pull"), self.campaign_id, self.donor.clone()),
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
            (symbol_short!("pl_skip"), self.campaign_id, self.donor.clone()),
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
            (symbol_short!("pl_settl"), self.campaign_id, self.milestone_index),
            self.total_pledged_pulled,
        );
    }
}
