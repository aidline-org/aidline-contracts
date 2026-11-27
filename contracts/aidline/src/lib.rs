#![no_std]

//! Aidline: milestone based escrow for disaster relief and climate campaigns.
//!
//! Donations are held by the contract and only reach the beneficiary one
//! milestone at a time, after the campaign's verifier confirms the work was
//! done. If a campaign is cancelled or runs out of time, donors can reclaim
//! their share of whatever has not been released yet.

mod errors;
mod events;
mod storage;
mod types;

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod test;

use soroban_sdk::{
    Address, BytesN, ContractExecutable, Env, String, Vec, contract, contractimpl, token,
};

pub use errors::Error;
pub use types::{
    Beneficiary, BondStatus, Campaign, CampaignKind, CampaignStatus, PendingApproval, Pledge,
    QueuedAction, RejectionReason, Role, SponsorPool, Stream, TimelockedAction, VerifierBond,
    VerifierStats,
};

use events::{
    ActionCancelled, ActionExecuted, ActionQueued, AdminAccepted, AdminChanged, AdminNominated,
    AdminProposed, BeneficiaryPaid, BondSlashed, BondWithdrawn, CampaignCancelled,
    CampaignCancelledWithReason, CampaignCreated, CancelVoteWithdrawn, CancelVoted, ContractPaused,
    ContractUnpaused, ContractUpgraded, DeadlineExtended, Donated, EmergencyAdvanceReleased,
    MatchingApplied, MetadataUpdated, Migrated, MilestoneApproved, MilestoneBlocked,
    MilestoneFinalized, MilestoneProposed, MilestoneRejected, MilestoneReleased,
    PlatformFeeUpdated, PledgeCancelled, PledgeCreated, PledgePulled, PledgeSettlement,
    PledgeSkipped, Refunded, RoleGranted, RoleRevoked, SlasherChanged, SponsorPoolDeposited,
    SponsorPoolReturned, StreamClaimed, TokenAllowlistUpdated, TreasuryUpdated, VerifierBondPosted,
    VerifierDeregistered, VerifierProfileUpdated, VerifierReassigned, VerifierUpdated,
};

const MAX_MILESTONES: u32 = 20;
/// Maximum bond withdrawal delay in seconds (90 days).
pub const MAX_BOND_WITHDRAW_DELAY: u64 = 90 * 86_400;
/// Maximum length in bytes for metadata and proof URIs.
const MAX_URI_LENGTH: u32 = 256;
/// Maximum length in bytes for milestone proof URIs.
pub const MAX_PROOF_URI_LENGTH: u32 = 500;
/// Maximum length in bytes for campaign metadata URIs.
pub const MAX_METADATA_URI_LENGTH: u32 = 500;
/// Minimum donation amount accepted by the contract to eliminate storage dust spam.
pub const MIN_DONATION_AMOUNT: i128 = 10;
/// Maximum matching ratio (100 % = 10 000 bps).
const MAX_RATIO_BPS: u32 = 10_000;
/// Minimum deposit and cap for a sponsor matching pool (Issue #158).
pub const MIN_SPONSOR_POOL: i128 = 1_000_000;
/// Maximum platform fee in basis points (200 bps = 2.00%).
const MAX_PLATFORM_FEE_BPS: u32 = 200;
/// Reason code emitted in PledgeSkipped when the donor has no allowance.
const SKIP_REASON_NO_ALLOWANCE: u32 = 1;
/// Reason code emitted in PledgeSkipped when the pledge is fully consumed.
const SKIP_REASON_EXHAUSTED: u32 = 2;
/// Reason code emitted in PledgeSkipped when multiplication overflows.
const SKIP_REASON_OVERFLOW: u32 = 3;
/// Reason code: the pledging donor's wallet holds no tokens.
const SKIP_REASON_INSUFFICIENT_BALANCE: u32 = 4;

/// Maximum duration (in seconds) for a one-time deadline extension (30 days).
const MAX_DEADLINE_EXTENSION: u64 = 30 * 86_400;

/// Maximum number of times a campaign's verifier may be reassigned.
pub const MAX_VERIFIER_REASSIGNMENTS: u32 = 2;

/// Longest challenge window a campaign may set, in seconds (Issue #115).
pub const MAX_CHALLENGE_WINDOW: u64 = 14 * 86_400;

/// Share of `raised`, in basis points, that donors' block votes must reach to
/// block a proposed approval without the admin (Issue #115).
pub const BLOCK_QUORUM_BPS: i128 = 3_000;
/// Seconds a queued admin action must wait before anyone may execute it.
pub const TIMELOCK_DELAY: u64 = 2 * 86_400;

/// Seconds after the delay during which a queued action can still be executed.
pub const TIMELOCK_GRACE_PERIOD: u64 = 7 * 86_400;
/// Seconds a campaign must go without progress (campaign start or the last
/// approved milestone, whichever is later) before donors can cancel it by vote.
pub const STALL_PERIOD: u64 = 14 * 86_400;

/// Share of `raised`, in basis points, that cancel votes must reach.
pub const CANCEL_VOTE_BPS: i128 = 5_000;
/// Version of the stored data layout this build reads and writes (Issue #118).
/// Bump it when a stored struct changes and add the step to `migrate`.
pub const SCHEMA_VERSION: u32 = 1;

/// Most beneficiaries a split-payout campaign may name (Issue #116).
pub const MAX_BENEFICIARIES: u32 = 10;
/// Maximum page limit for paginated donor enumeration.
const MAX_DONORS_PAGE_LIMIT: u32 = 50;

/// The emergency advance is capped at 20% of the campaign goal (200 bps out of
/// 1000, or equivalently goal / 5).  Integer arithmetic: advance = goal / 5
/// (rounds down, so the cap is never exceeded).
const EMERGENCY_ADVANCE_BPS: i128 = 2_000; // 20% in basis points
const BPS_DENOM: i128 = 10_000;

#[contract]
pub struct Aidline;

#[contractimpl]
impl Aidline {
    /// Initialises the contract, storing the `admin` address and the `token`
    /// (a Stellar Asset Contract such as USDC) used for all campaigns.
    ///
    /// Called once at deployment time; may not be called again.
    ///
    /// # Errors
    /// This function does not return errors; panics if storage is unavailable
    /// or if `admin` and `token` are invalid (e.g. identical or self-referential).
    pub fn __constructor(env: Env, admin: Address, token: Address) {
        if admin == token {
            panic!("admin and token must be distinct");
        }
        let contract = env.current_contract_address();
        if admin == contract || token == contract {
            panic!("admin and token cannot be self address");
        }

        storage::set_admin(&env, &admin);
        storage::set_token(&env, &token);
        storage::set_token_allowed(&env, &token, true);
        storage::set_schema_version(&env, SCHEMA_VERSION);
        storage::bump_instance(&env);
    }

    // ─── Admin ────────────────────────────────────────────────────────────────

    /// Grants `role` to `account`.
    ///
    /// Only the primary admin may call this. Emits [`RoleGranted`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin or `account` is the
    ///   contract address itself.
    pub fn grant_role(env: Env, role: Role, account: Address) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        if account == env.current_contract_address() {
            return Err(Error::Unauthorized);
        }
        storage::set_role(&env, role, &account, true);
        RoleGranted { role, account }.publish(&env);
        Ok(())
    }

    /// Revokes `role` from `account`.
    ///
    /// Only the primary admin may call this. Emits [`RoleRevoked`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    pub fn revoke_role(env: Env, role: Role, account: Address) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        storage::set_role(&env, role, &account, false);
        RoleRevoked { role, account }.publish(&env);
        Ok(())
    }

    /// Returns `true` if `account` holds `role` (or is the primary admin for `Role::Admin`).
    pub fn has_role(env: Env, role: Role, account: Address) -> bool {
        storage::has_role(&env, role, &account)
    }

    /// Registers `verifier` as an active verifier.
    ///
    /// Only the admin or an account with [`Role::VerifierManager`] may call
    /// this function. Emits [`VerifierUpdated`] with `active = true`.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: `caller` lacks admin or
    ///   [`Role::VerifierManager`] authorization, or `verifier` is the
    ///   contract address itself.
    pub fn add_verifier(env: Env, caller: Address, verifier: Address) -> Result<(), Error> {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::VerifierManager)?;
        storage::bump_instance(&env);
        if verifier == env.current_contract_address() {
            return Err(Error::Unauthorized);
        }
        storage::set_verifier(&env, &verifier, true);
        VerifierUpdated {
            verifier,
            active: true,
        }
        .publish(&env);
        Ok(())
    }

    /// Deactivates `verifier`, preventing them from approving future milestones.
    ///
    /// Only the admin or an account with [`Role::VerifierManager`] may call
    /// this function. Campaigns previously assigned to the verifier will freeze
    /// until the campaign is cancelled or reassigned via [`reassign_verifier`].
    /// Emits [`VerifierUpdated`] with `active = false`.
    ///
    /// If the verifier has an active bond it moves to `PendingWithdrawal` (as in
    /// [`deregister_verifier`]) and [`VerifierDeregistered`] is emitted, so the
    /// bond can always be withdrawn after the delay.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: `caller` lacks admin or
    ///   [`Role::VerifierManager`] authorization.
    pub fn remove_verifier(env: Env, caller: Address, verifier: Address) -> Result<(), Error> {
        caller.require_auth();
        Self::require_role(&env, &caller, Role::VerifierManager)?;
        storage::bump_instance(&env);
        storage::set_verifier(&env, &verifier, false);

        if let Some(mut bond) = storage::verifier_bond(&env, &verifier)
            && bond.status == BondStatus::Active
        {
            let now = env.ledger().timestamp();
            bond.status = BondStatus::PendingWithdrawal;
            bond.deregistered_at = now;
            storage::save_verifier_bond(&env, &bond);
            VerifierDeregistered {
                verifier: verifier.clone(),
                deregistered_at: now,
            }
            .publish(&env);
        }

        VerifierUpdated {
            verifier,
            active: false,
        }
        .publish(&env);
        Ok(())
    }

    /// Sets an off-chain profile URI for a registered `verifier`.
    ///
    /// Only the admin may call this, and only for verifiers currently registered.
    /// `profile_uri` must not exceed [`MAX_URI_LENGTH`] bytes. Emits [`VerifierProfileUpdated`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    /// - [`Error::UriTooLong`]: `profile_uri` exceeds [`MAX_URI_LENGTH`] bytes.
    /// - [`Error::NotVerifier`]: `verifier` is not currently registered.
    pub fn set_verifier_profile(
        env: Env,
        verifier: Address,
        profile_uri: String,
    ) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        storage::bump_instance(&env);

        if profile_uri.len() > MAX_URI_LENGTH {
            return Err(Error::UriTooLong);
        }

        if !storage::is_verifier(&env, &verifier) {
            return Err(Error::NotVerifier);
        }

        storage::set_verifier_profile(&env, &verifier, &profile_uri);

        VerifierProfileUpdated {
            verifier,
            profile_uri,
        }
        .publish(&env);

        Ok(())
    }

    /// Transfers admin control to `new_admin` in a single step.
    ///
    /// # Safety Warning (Deprecated Pattern)
    /// Direct single-step transfer permanently overwrites the admin address
    /// without verifying that `new_admin` can sign transactions. Prefer the
    /// two-step transfer via [`nominate_admin`] (or [`propose_admin`]) and
    /// [`accept_admin`] to avoid accidental irrevocable loss of admin control.
    ///
    /// Only the current admin may call this. Emits [`AdminChanged`] with
    /// both the old and new admin addresses so indexers can track the change.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the current admin (enforced
    ///   via `require_auth` on the stored admin address).
    pub fn set_admin(env: Env, new_admin: Address) {
        let old_admin = storage::admin(&env);
        old_admin.require_auth();
        storage::bump_instance(&env);
        storage::set_admin(&env, &new_admin);
        storage::remove_pending_admin(&env);
        AdminChanged {
            old: old_admin,
            new: new_admin,
        }
        .publish(&env);
    }

    /// Nominates `candidate` to take over admin control in a two-step transfer.
    ///
    /// Only the current admin may call this. The current admin retains full
    /// administrative authority until `candidate` explicitly accepts via
    /// [`accept_admin`]. Calling this again replaces any pending nomination.
    /// Emits [`AdminNominated`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the current admin.
    pub fn nominate_admin(env: Env, candidate: Address) {
        let current_admin = storage::admin(&env);
        current_admin.require_auth();
        storage::bump_instance(&env);
        storage::set_pending_admin(&env, &candidate);
        AdminNominated {
            current_admin,
            pending_admin: candidate,
        }
        .publish(&env);
    }

    /// Proposes transferring admin control to `new_admin`.
    ///
    /// Only the current admin may call this. The transfer is not finalized
    /// until `new_admin` calls [`accept_admin`]. Calling this again replaces
    /// any existing pending proposal. Emits [`AdminProposed`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the current admin.
    pub fn propose_admin(env: Env, new_admin: Address) {
        let current_admin = storage::admin(&env);
        current_admin.require_auth();
        storage::bump_instance(&env);
        storage::set_pending_admin(&env, &new_admin);
        AdminProposed {
            current_admin,
            pending_admin: new_admin,
        }
        .publish(&env);
    }

    /// Finalizes a pending admin transfer.
    ///
    /// Must be called with authorization from the pending admin set via
    /// [`propose_admin`]. Updates the stored admin, clears the pending proposal,
    /// and emits [`AdminAccepted`] and [`AdminChanged`].
    ///
    /// # Errors
    /// - [`Error::NoPendingAdmin`]: no admin transfer proposal is currently pending.
    /// - [`Error::Unauthorized`]: caller is not the pending admin.
    pub fn accept_admin(env: Env) -> Result<(), Error> {
        let pending = storage::pending_admin(&env).ok_or(Error::NoPendingAdmin)?;
        pending.require_auth();
        storage::bump_instance(&env);
        let old_admin = storage::admin(&env);
        storage::set_admin(&env, &pending);
        storage::remove_pending_admin(&env);

        AdminAccepted {
            old_admin: old_admin.clone(),
            new_admin: pending.clone(),
        }
        .publish(&env);

        AdminChanged {
            old: old_admin,
            new: pending,
        }
        .publish(&env);

        Ok(())
    }

    /// Pauses the contract in emergencies.
    ///
    /// Only the admin may call this. While paused, `donate`, `approve_milestone`,
    /// `create_campaign`, `emergency_fast_track`, and `deposit_sponsor_pool`
    /// fail with [`Error::Paused`].
    /// Refunds keep working so donors are never locked in. Emits [`ContractPaused`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    pub fn pause(env: Env) {
        let admin = storage::admin(&env);
        admin.require_auth();
        storage::bump_instance(&env);
        storage::set_paused(&env, true);
        ContractPaused { admin }.publish(&env);
    }

    /// Unpauses the contract, restoring normal operation.
    ///
    /// Only the admin may call this. Emits [`ContractUnpaused`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    pub fn unpause(env: Env) {
        let admin = storage::admin(&env);
        admin.require_auth();
        storage::bump_instance(&env);
        storage::set_paused(&env, false);
        ContractUnpaused { admin }.publish(&env);
    }

    /// Upgrades the contract's Wasm executable to `new_wasm_hash`.
    ///
    /// Only the current admin may call this. The new Wasm code must already
    /// be installed on-chain. Emits [`ContractUpgraded`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    pub fn upgrade(env: Env, new_wasm_hash: BytesN<32>) {
        let admin = storage::admin(&env);
        admin.require_auth();
        storage::bump_instance(&env);
        env.deployer()
            .update_current_contract(ContractExecutable::Wasm(new_wasm_hash.clone()));
        ContractUpgraded { new_wasm_hash }.publish(&env);
    }

    // ─── Storage schema (Issue #118) ──────────────────────────────────────────

    /// Returns the version of the stored data layout. `0` means the data was
    /// written before versioning existed and [`migrate`] has not run yet.
    pub fn schema_version(env: Env) -> u32 {
        storage::schema_version(&env)
    }

    /// Upgrades stored data to [`SCHEMA_VERSION`], one version at a time, and
    /// returns the resulting version.
    ///
    /// Only the admin may call this. Until it has run after an upgrade that
    /// raised [`SCHEMA_VERSION`], the state-changing entry points fail with
    /// [`Error::MigrationPending`]. Calling it when nothing is pending changes
    /// nothing. Emits [`Migrated`] on every call.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    pub fn migrate(env: Env) -> u32 {
        storage::admin(&env).require_auth();
        let from = storage::schema_version(&env);
        let mut version = from;
        while version < SCHEMA_VERSION {
            // Each arm upgrades the stored data from `version` to `version + 1`.
            #[allow(clippy::single_match)]
            match version {
                // 0 -> 1: the layout is unchanged; this step only marks the
                // existing data as versioned.
                0 => {}
                _ => {}
            }
            version += 1;
        }
        storage::set_schema_version(&env, version);
        storage::bump_instance(&env);
        Migrated { from, to: version }.publish(&env);
        version
    }

    // ─── Platform Fee and Treasury (admin only) ───────────────────────────────

    /// Sets the platform fee in basis points.
    ///
    /// Only the admin may call this. The fee is capped at 200 basis points (2.00%).
    /// Setting `fee_bps` to `0` disables the platform fee (default).
    /// Emits [`PlatformFeeUpdated`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    /// - [`Error::InvalidAmount`]: `fee_bps` exceeds 200 basis points.
    pub fn set_platform_fee(env: Env, fee_bps: u32) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        storage::bump_instance(&env);
        if fee_bps > MAX_PLATFORM_FEE_BPS {
            return Err(Error::InvalidAmount);
        }
        storage::set_platform_fee(&env, fee_bps);
        PlatformFeeUpdated { fee_bps }.publish(&env);
        Ok(())
    }

    /// Sets the platform treasury address that receives milestone release fees.
    ///
    /// Only the admin may call this. Emits [`TreasuryUpdated`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    pub fn set_treasury(env: Env, treasury: Address) {
        storage::admin(&env).require_auth();
        storage::bump_instance(&env);
        storage::set_treasury(&env, &treasury);
        TreasuryUpdated { treasury }.publish(&env);
    }

    // ─── Token Allowlist (admin only) ─────────────────────────────────────────

    /// Adds `token` to the list of allowed campaign tokens.
    ///
    /// Only the admin may call this. Emits [`TokenAllowlistUpdated`].
    pub fn add_allowed_token(env: Env, token: Address) {
        storage::admin(&env).require_auth();
        storage::bump_instance(&env);
        storage::set_token_allowed(&env, &token, true);
        TokenAllowlistUpdated {
            token,
            allowed: true,
        }
        .publish(&env);
    }

    /// Removes `token` from the list of allowed campaign tokens.
    ///
    /// Only the admin may call this. Emits [`TokenAllowlistUpdated`].
    pub fn remove_allowed_token(env: Env, token: Address) {
        storage::admin(&env).require_auth();
        storage::bump_instance(&env);
        storage::set_token_allowed(&env, &token, false);
        TokenAllowlistUpdated {
            token,
            allowed: false,
        }
        .publish(&env);
    }

    /// Returns `true` if `token` is currently on the allowlist for new campaigns.
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        storage::is_token_allowed(&env, &token)
    }

    // ─── Bond configuration (admin only) ──────────────────────────────────────

    /// Sets the minimum bond amount a verifier must post when calling
    /// [`register_with_bond`].
    ///
    /// Only the admin may call this. Setting `amount` to `0` disables the bond
    /// requirement.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    /// - [`Error::InvalidAmount`]: `amount` is negative.
    pub fn set_bond_requirement(env: Env, amount: i128) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        storage::bump_instance(&env);
        if amount < 0 {
            return Err(Error::InvalidAmount);
        }
        storage::set_bond_requirement(&env, amount);
        Ok(())
    }

    /// Sets the delay (in seconds) a verifier must wait after deregistering
    /// before they can withdraw their bond.
    ///
    /// Only the admin may call this. A non-zero delay gives the admin time to
    /// slash a fraudulent verifier before they can withdraw funds.
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    /// Sets the delay (in seconds) a verifier must wait after deregistering
    /// before they can withdraw their bond.
    ///
    /// Only the admin may call this. A non-zero delay gives the admin time to
    /// slash a fraudulent verifier before they can withdraw funds. Capped at
    /// [`MAX_BOND_WITHDRAW_DELAY`] (90 days).
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    /// - [`Error::InvalidAmount`]: `delay` exceeds [`MAX_BOND_WITHDRAW_DELAY`].
    pub fn set_bond_withdraw_delay(env: Env, delay: u64) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        storage::bump_instance(&env);
        if delay > MAX_BOND_WITHDRAW_DELAY {
            return Err(Error::InvalidAmount);
        }
        storage::set_bond_withdraw_delay(&env, delay);
        Ok(())
    }

    // ─── Bonded verifier registration ─────────────────────────────────────────

    /// Registers `verifier` and transfers their bond into escrow.
    ///
    /// The caller must be `verifier` (auth enforced). `bond_amount` must be at
    /// least the value returned by [`bond_requirement`] and must be > 0. The
    /// tokens are transferred from `verifier` to the contract immediately.
    /// Emits [`VerifierBondPosted`].
    ///
    /// # Errors
    /// - [`Error::BondRequired`]: `bond_amount` is below the required minimum
    ///   or is zero.
    /// - [`Error::Unauthorized`]: `verifier` is already registered or is the
    ///   contract address itself.
    pub fn register_with_bond(env: Env, verifier: Address, bond_amount: i128) -> Result<(), Error> {
        Self::ensure_current_schema(&env)?;
        verifier.require_auth();
        storage::bump_instance(&env);
        if verifier == env.current_contract_address() {
            return Err(Error::Unauthorized);
        }

        let required = storage::bond_requirement(&env);
        if bond_amount < required || bond_amount <= 0 {
            return Err(Error::BondRequired);
        }

        if storage::is_verifier(&env, &verifier) {
            return Err(Error::Unauthorized);
        }

        if let Some(existing_bond) = storage::verifier_bond(&env, &verifier) {
            match existing_bond.status {
                BondStatus::Active | BondStatus::PendingWithdrawal => {
                    return Err(Error::Unauthorized);
                }
                BondStatus::Withdrawn | BondStatus::Slashed => {
                    storage::archive_verifier_bond(&env, &verifier, &existing_bond);
                }
            }
        }

        token::Client::new(&env, &storage::token(&env)).transfer(
            &verifier,
            env.current_contract_address(),
            &bond_amount,
        );

        let bond = VerifierBond {
            verifier: verifier.clone(),
            amount: bond_amount,
            remaining: bond_amount,
            status: BondStatus::Active,
            deregistered_at: 0,
        };
        storage::save_verifier_bond(&env, &bond);
        storage::set_verifier(&env, &verifier, true);

        VerifierBondPosted {
            verifier,
            amount: bond_amount,
        }
        .publish(&env);

        Ok(())
    }

    /// Deregisters `verifier`, beginning the bond withdrawal delay countdown.
    ///
    /// Either the verifier themselves or the admin may call this. Explicitly
    /// requires `verifier.require_auth()` for self-deregistration or
    /// `admin.require_auth()` when `caller != verifier`. The verifier is
    /// removed from the active registry immediately, and their bond enters
    /// `PendingWithdrawal` status until the delay set by
    /// [`set_bond_withdraw_delay`] has elapsed. Emits [`VerifierDeregistered`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: `caller` is neither `verifier` nor the admin.
    /// - [`Error::NotVerifier`]: `verifier` is not currently registered.
    pub fn deregister_verifier(env: Env, caller: Address, verifier: Address) -> Result<(), Error> {
        caller.require_auth();
        if caller != verifier && Self::require_role(&env, &caller, Role::VerifierManager).is_err() {
            return Err(Error::Unauthorized);
        }
        storage::bump_instance(&env);

        if !storage::is_verifier(&env, &verifier) {
            return Err(Error::NotVerifier);
        }

        storage::set_verifier(&env, &verifier, false);

        let now = env.ledger().timestamp();

        if let Some(mut bond) = storage::verifier_bond(&env, &verifier) {
            bond.status = BondStatus::PendingWithdrawal;
            bond.deregistered_at = now;
            storage::save_verifier_bond(&env, &bond);
        }

        VerifierDeregistered {
            verifier,
            deregistered_at: now,
        }
        .publish(&env);

        Ok(())
    }

    /// Withdraws the verifier's remaining bond back to their wallet.
    ///
    /// Requires auth from `verifier`. The bond must be in `PendingWithdrawal`
    /// status and the withdrawal delay must have elapsed since deregistration.
    /// Emits [`BondWithdrawn`] and returns the amount transferred.
    ///
    /// # Errors
    /// - [`Error::BondNotFound`]: no bond record exists for `verifier`.
    /// - [`Error::BondNotWithdrawable`]: bond is not in `PendingWithdrawal`
    ///   status, or the remaining amount is zero.
    /// - [`Error::WithdrawDelayNotMet`]: the required delay since
    ///   deregistration has not yet elapsed.
    pub fn withdraw_bond(env: Env, verifier: Address) -> Result<i128, Error> {
        verifier.require_auth();
        storage::bump_instance(&env);

        let mut bond = storage::verifier_bond(&env, &verifier).ok_or(Error::BondNotFound)?;

        if bond.status != BondStatus::PendingWithdrawal {
            return Err(Error::BondNotWithdrawable);
        }

        // The bond is the collateral for every campaign the verifier is still
        // responsible for, so it stays slashable until they are all finished.
        if storage::active_campaigns(&env, &verifier) > 0 {
            return Err(Error::VerifierHasActiveCampaigns);
        }

        let delay = storage::bond_withdraw_delay(&env);
        let elapsed = env
            .ledger()
            .timestamp()
            .saturating_sub(bond.deregistered_at);
        if elapsed < delay {
            return Err(Error::WithdrawDelayNotMet);
        }

        let withdrawable = bond.remaining;
        if withdrawable <= 0 {
            return Err(Error::BondNotWithdrawable);
        }

        bond.remaining = 0;
        bond.status = BondStatus::Withdrawn;
        storage::save_verifier_bond(&env, &bond);

        token::Client::new(&env, &storage::token(&env)).transfer(
            &env.current_contract_address(),
            &verifier,
            &withdrawable,
        );

        BondWithdrawn {
            verifier,
            amount: withdrawable,
        }
        .publish(&env);

        Ok(withdrawable)
    }

    /// Slashes `slash_amount` from `verifier`'s bond and credits it to
    /// `campaign_id`'s slashed recovery pool so it is distributed to donors pro-rata on refund.
    ///
    /// Only the slasher may call this (the admin unless [`set_slasher`] named
    /// someone else). The slash amount must not exceed the verifier's remaining
    /// bond. Emits [`BondSlashed`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the slasher.
    /// - [`Error::InvalidAmount`]: `slash_amount` is zero or negative.
    /// - [`Error::BondNotFound`]: no bond record exists for `verifier`.
    /// - [`Error::BondNotWithdrawable`]: bond has already been withdrawn.
    /// - [`Error::SlashExceedsBond`]: `slash_amount` exceeds remaining bond.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`]: campaign is completed.
    pub fn slash_verifier(
        env: Env,
        caller: Address,
        verifier: Address,
        campaign_id: u64,
        slash_amount: i128,
    ) -> Result<(), Error> {
        caller.require_auth();
        if caller != storage::slasher(&env) && !storage::has_role(&env, Role::Slasher, &caller) {
            return Err(Error::Unauthorized);
        }
        Self::slash_verifier_impl(env, verifier, campaign_id, slash_amount)
    }

    fn slash_verifier_impl(
        env: Env,
        verifier: Address,
        campaign_id: u64,
        slash_amount: i128,
    ) -> Result<(), Error> {
        if slash_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut bond = storage::verifier_bond(&env, &verifier).ok_or(Error::BondNotFound)?;

        if bond.status == BondStatus::Withdrawn {
            return Err(Error::BondNotWithdrawable);
        }
        if slash_amount > bond.remaining {
            return Err(Error::SlashExceedsBond);
        }

        let campaign = storage::campaign(&env, campaign_id)?;
        if campaign.status == CampaignStatus::Completed {
            return Err(Error::CampaignNotActive);
        }

        bond.remaining -= slash_amount;
        if bond.remaining == 0 {
            bond.status = BondStatus::Slashed;
        }
        storage::save_verifier_bond(&env, &bond);
        Self::bump_verifier(&env, &verifier, |s| s.total_slashed += slash_amount);

        let recovery = storage::slashed_recovery(&env, campaign_id);
        storage::set_slashed_recovery(&env, campaign_id, recovery + slash_amount);

        BondSlashed {
            verifier: verifier.clone(),
            campaign_id,
            slashed_amount: slash_amount,
        }
        .publish(&env);

        Ok(())
    }

    /// Names the address that may slash verifier bonds, for example a multisig
    /// account separate from the admin key.
    ///
    /// Only the admin may call this. Until it is called the slasher is the
    /// admin. Once another address is set the admin can no longer slash unless
    /// they set themselves again. Emits [`SlasherChanged`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    pub fn set_slasher(env: Env, slasher: Address) {
        storage::admin(&env).require_auth();
        let old = storage::slasher(&env);
        storage::set_slasher(&env, &slasher);
        storage::bump_instance(&env);
        SlasherChanged { old, new: slasher }.publish(&env);
    }

    /// Returns the address that may slash verifier bonds.
    pub fn get_slasher(env: Env) -> Address {
        storage::slasher(&env)
    }

    // ─── Bond views ───────────────────────────────────────────────────────────

    /// Returns the [`VerifierBond`] record for `verifier`, or `None` if no
    /// bond has ever been posted.
    pub fn get_verifier_bond(env: Env, verifier: Address) -> Option<VerifierBond> {
        storage::verifier_bond(&env, &verifier)
    }

    /// Returns the historical [`VerifierBond`] record for `verifier` at `nonce`.
    pub fn get_historical_verifier_bond(
        env: Env,
        verifier: Address,
        nonce: u32,
    ) -> Option<VerifierBond> {
        storage::historical_verifier_bond(&env, &verifier, nonce)
    }

    /// Returns the number of archived bond cycles for `verifier`.
    pub fn get_verifier_bond_nonce(env: Env, verifier: Address) -> u32 {
        storage::verifier_bond_nonce(&env, &verifier)
    }

    /// Returns the current required bond amount. `0` means no bond is required.
    pub fn bond_requirement(env: Env) -> i128 {
        storage::bond_requirement(&env)
    }

    /// Returns the bond withdrawal delay in seconds. `0` means no delay.
    pub fn bond_withdraw_delay(env: Env) -> u64 {
        storage::bond_withdraw_delay(&env)
    }

    // ─── Campaigns ────────────────────────────────────────────────────────────

    /// Creates a new campaign and returns its ID.
    ///
    /// `creator` must authorise the call. `verifier` must be a registered
    /// verifier. `token` must be on the admin approved allowlist.
    /// `deadline` must be in the future. `milestones` must be
    /// non-empty, have at most [`MAX_MILESTONES`] entries, and every entry
    /// must be positive. The campaign's `goal` is the sum of all milestones.
    /// Emits [`CampaignCreated`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: `beneficiary` is the contract address itself.
    /// - [`Error::InvalidMetadataUri`]: `metadata_uri` is empty or exceeds
    ///   [`MAX_METADATA_URI_LENGTH`] bytes.
    /// - [`Error::ConflictOfInterest`]: `verifier` is equal to `creator` or
    ///   `beneficiary`.
    /// - [`Error::NotVerifier`]: `verifier` is not currently registered.
    /// - [`Error::TokenNotAllowed`]: `token` is not on the admin allowlist.
    /// - [`Error::DeadlineInPast`]: `deadline` is not after the current
    ///   ledger timestamp.
    /// - [`Error::InvalidMilestones`]: `milestones` is empty, exceeds
    ///   [`MAX_MILESTONES`], contains a non-positive value, or the sum
    ///   overflows `i128`.
    pub fn create_campaign(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
    ) -> Result<u64, Error> {
        let mut verifiers = Vec::new(&env);
        verifiers.push_back(verifier.clone());
        Self::create_multisig_campaign_inner(
            env,
            creator,
            beneficiary,
            verifiers,
            1,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
            0,
            0,
        )
    }

    /// Creates a new campaign with a minimum donation floor.
    ///
    /// Donations below `min_donation` are rejected by [`donate`], except for
    /// the final donation that reaches or exceeds the remaining goal.
    /// A `min_donation` of 0 imposes no floor.
    ///
    /// # Errors
    /// - [`Error::InvalidAmount`]: `min_donation` is negative.
    /// - All other errors from [`create_campaign`].
    pub fn create_campaign_min_donation(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
        min_donation: i128,
    ) -> Result<u64, Error> {
        let mut verifiers = Vec::new(&env);
        verifiers.push_back(verifier.clone());
        Self::create_multisig_campaign_inner(
            env,
            creator,
            beneficiary,
            verifiers,
            1,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
            min_donation,
            0,
        )
    }

    /// Creates a new campaign with a minimum gap (cooldown) in seconds between milestone approvals.
    ///
    /// `approval_cooldown` defines the required time that must elapse between consecutive
    /// milestone approvals. A cooldown of `0` imposes no delay between approvals.
    ///
    /// # Errors
    /// All errors from [`create_campaign`].
    pub fn create_campaign_with_cooldown(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
        approval_cooldown: u64,
    ) -> Result<u64, Error> {
        let mut verifiers = Vec::new(&env);
        verifiers.push_back(verifier.clone());
        Self::create_multisig_campaign_inner(
            env,
            creator,
            beneficiary,
            verifiers,
            1,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
            0,
            approval_cooldown,
        )
    }

    /// Creates a new campaign with multiple verifiers, an approval threshold, and a token.
    ///
    /// `threshold` verifier approvals are required before any milestone can be
    /// released. `threshold` must be between 1 and `verifiers.len()` inclusive.
    /// Every entry in `verifiers` must be currently registered and distinct.
    /// `token` must be on the admin approved allowlist.
    ///
    /// # Errors
    /// - [`Error::TokenNotAllowed`]: `token` is not on the allowlist.
    /// - [`Error::InvalidThreshold`]: `threshold` is 0 or exceeds `verifiers.len()`.
    /// - [`Error::DuplicateVerifier`]: `verifiers` contains duplicate addresses.
    /// - [`Error::NotVerifier`]: an address in `verifiers` is not registered.
    /// - All other errors from [`create_campaign`].
    pub fn create_multisig_campaign(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifiers: Vec<Address>,
        threshold: u32,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
    ) -> Result<u64, Error> {
        Self::create_multisig_campaign_inner(
            env,
            creator,
            beneficiary,
            verifiers,
            threshold,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
            0,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn create_multisig_campaign_inner(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifiers: Vec<Address>,
        threshold: u32,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
        min_donation: i128,
        approval_cooldown: u64,
    ) -> Result<u64, Error> {
        Self::ensure_current_schema(&env)?;
        creator.require_auth();
        storage::bump_instance(&env);

        let contract = env.current_contract_address();
        if beneficiary == contract {
            return Err(Error::Unauthorized);
        }
        if min_donation < 0 {
            return Err(Error::InvalidAmount);
        }
        if metadata_uri.is_empty() || metadata_uri.len() > MAX_METADATA_URI_LENGTH {
            return Err(Error::InvalidMetadataUri);
        }
        if verifiers.contains(&creator) || verifiers.contains(&beneficiary) {
            return Err(Error::ConflictOfInterest);
        }

        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }

        if !storage::is_token_allowed(&env, &token) {
            return Err(Error::TokenNotAllowed);
        }

        if verifiers.is_empty() || threshold == 0 || threshold > verifiers.len() {
            return Err(Error::InvalidThreshold);
        }

        let mut seen = Vec::new(&env);
        for v in verifiers.iter() {
            if seen.contains(&v) {
                return Err(Error::DuplicateVerifier);
            }
            if v == contract || !storage::is_verifier(&env, &v) {
                return Err(Error::NotVerifier);
            }
            seen.push_back(v);
        }

        if deadline <= env.ledger().timestamp() {
            return Err(Error::DeadlineInPast);
        }
        if milestones.is_empty() || milestones.len() > MAX_MILESTONES {
            return Err(Error::InvalidMilestones);
        }
        let mut goal: i128 = 0;
        for amount in milestones.iter() {
            if amount <= 0 {
                return Err(Error::InvalidMilestones);
            }
            goal = goal.checked_add(amount).ok_or(Error::InvalidMilestones)?;
        }

        let id = storage::next_campaign_id(&env);
        let primary_verifier = verifiers.get(0).unwrap();
        let campaign = Campaign {
            id,
            creator: creator.clone(),
            beneficiary,
            verifier: primary_verifier,
            token,
            kind,
            metadata_uri,
            goal,
            deadline,
            milestones,
            milestones_released: 0,
            raised: 0,
            released: 0,
            status: CampaignStatus::Active,
            emergency_advance: 0,
            verifiers,
            threshold,
            deadline_extended: false,
            min_donation,
            verifier_reassignments: 0,
            approval_cooldown,
            last_approval_time: 0,
        };
        storage::create_campaign(&env, &campaign);
        storage::set_campaign_start(&env, id, env.ledger().timestamp());
        Self::open_verifier_assignments(&env, &campaign.verifiers);
        Self::bump_all_verifiers(&env, &campaign.verifiers, |s| s.campaigns_assigned += 1);

        CampaignCreated {
            campaign_id: id,
            creator,
            kind,
            goal,
            deadline,
            min_donation,
        }
        .publish(&env);
        Ok(id)
    }

    /// Creates a campaign with per-milestone due dates.
    ///
    /// # Errors
    /// - [`Error::InvalidMilestones`]: `milestone_due_dates` length does not match
    ///   `milestones` length, due dates are not strictly monotonically increasing,
    ///   or a due date exceeds `deadline`.
    /// - [`Error::DeadlineInPast`]: a milestone due date is not in the future.
    pub fn create_campaign_with_due_dates(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
        milestone_due_dates: Vec<u64>,
    ) -> Result<u64, Error> {
        if milestone_due_dates.len() != milestones.len() {
            return Err(Error::InvalidMilestones);
        }

        let now = env.ledger().timestamp();
        let mut prev_due = 0u64;
        for (i, due_date) in milestone_due_dates.iter().enumerate() {
            if due_date <= now {
                return Err(Error::DeadlineInPast);
            }
            if i > 0 && due_date <= prev_due {
                return Err(Error::InvalidMilestones);
            }
            if due_date > deadline {
                return Err(Error::InvalidMilestones);
            }
            prev_due = due_date;
        }

        let id = Self::create_campaign(
            env.clone(),
            creator,
            beneficiary,
            verifier,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
        )?;
        storage::set_milestone_due_dates(&env, id, &milestone_due_dates, deadline);
        Ok(id)
    }

    /// Creates a campaign whose payouts are split across several beneficiaries
    /// and returns its ID (Issue #116).
    ///
    /// `beneficiaries` lists each recipient with a share in basis points; the
    /// shares must sum to 10,000, be positive and name distinct addresses, and
    /// the verifier cannot be one of them. Each payout (milestone or emergency
    /// advance) gives every beneficiary `amount * bps / 10_000`, rounded down,
    /// with the rounding dust going to the first beneficiary, and emits
    /// [`BeneficiaryPaid`] per recipient. With a single beneficiary this is
    /// the same as [`create_campaign`]. Other arguments and errors are those of
    /// [`create_campaign`].
    ///
    /// # Errors
    /// - [`Error::InvalidBeneficiaries`]: the list is empty, longer than
    ///   [`MAX_BENEFICIARIES`], has a zero share, repeats an address, or its
    ///   shares do not sum to 10,000.
    /// - [`Error::ConflictOfInterest`]: the verifier is one of the beneficiaries.
    pub fn create_campaign_beneficiaries(
        env: Env,
        creator: Address,
        beneficiaries: Vec<Beneficiary>,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
    ) -> Result<u64, Error> {
        if beneficiaries.is_empty() || beneficiaries.len() > MAX_BENEFICIARIES {
            return Err(Error::InvalidBeneficiaries);
        }
        let contract = env.current_contract_address();
        let mut total_bps: u32 = 0;
        for (i, b) in beneficiaries.iter().enumerate() {
            if b.to == contract {
                return Err(Error::Unauthorized);
            }
            if b.bps == 0 {
                return Err(Error::InvalidBeneficiaries);
            }
            total_bps = total_bps
                .checked_add(b.bps)
                .ok_or(Error::InvalidBeneficiaries)?;
            if b.to == verifier {
                return Err(Error::ConflictOfInterest);
            }
            for earlier in beneficiaries.iter().take(i) {
                if earlier.to == b.to {
                    return Err(Error::InvalidBeneficiaries);
                }
            }
        }
        if total_bps != MAX_RATIO_BPS {
            return Err(Error::InvalidBeneficiaries);
        }

        let primary = beneficiaries.get(0).unwrap().to;
        let id = Self::create_campaign(
            env.clone(),
            creator,
            primary,
            verifier,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
        )?;
        if beneficiaries.len() > 1 {
            storage::set_beneficiaries(&env, id, &beneficiaries);
        }
        Ok(id)
    }

    /// Returns the beneficiaries of `campaign_id`: the split list, or the single
    /// beneficiary with 10,000 bps for an ordinary campaign.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn get_beneficiaries(env: Env, campaign_id: u64) -> Result<Vec<Beneficiary>, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        Ok(
            storage::beneficiaries(&env, campaign_id).unwrap_or_else(|| {
                let mut single = Vec::new(&env);
                single.push_back(Beneficiary {
                    to: campaign.beneficiary,
                    bps: MAX_RATIO_BPS,
                });
                single
            }),
        )
    }

    /// Creates a campaign with optional stretch milestones and returns its ID.
    ///
    /// The stretch milestones are appended after the base `milestones` and the
    /// campaign's `goal` is the sum of both, so donations are accepted past the
    /// base goal only up to the stretch total. Stretch milestones are approved
    /// in order like any other milestone. Campaigns that do not reach the
    /// stretch refund pro rata as usual. With an empty `stretch_milestones`
    /// this behaves exactly like [`create_campaign`].
    ///
    /// # Errors
    /// Same as [`create_campaign`]. The combined milestone list must not exceed
    /// [`MAX_MILESTONES`].
    pub fn create_campaign_with_stretch(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
        stretch_milestones: Vec<i128>,
    ) -> Result<u64, Error> {
        let mut all_milestones = milestones;
        for amount in stretch_milestones.iter() {
            all_milestones.push_back(amount);
        }

        let id = Self::create_campaign(
            env.clone(),
            creator,
            beneficiary,
            verifier,
            token,
            kind,
            metadata_uri,
            deadline,
            all_milestones,
        )?;
        if !stretch_milestones.is_empty() {
            storage::set_stretch_count(&env, id, stretch_milestones.len());
        }
        Ok(id)
    }

    /// Returns how many of `campaign_id`'s trailing milestones are stretch milestones.
    pub fn stretch_milestone_count(env: Env, campaign_id: u64) -> u32 {
        storage::stretch_count(&env, campaign_id)
    }

    /// Creates a campaign whose milestone approvals wait out a challenge window
    /// and returns its ID (Issue #115).
    ///
    /// With `challenge_window` seconds above zero, an approval that reaches the
    /// verifier threshold does not pay out. It becomes a pending proposal that
    /// anyone can [`finalize_milestone`] once the window has passed, and that
    /// the admin, or donors holding [`BLOCK_QUORUM_BPS`] of `raised`, can
    /// [`block_milestone`] meanwhile. A window of `0` is the normal immediate
    /// behaviour. Other arguments and errors are those of [`create_campaign`].
    ///
    /// # Errors
    /// - [`Error::InvalidChallengeWindow`]: `challenge_window` exceeds
    ///   [`MAX_CHALLENGE_WINDOW`].
    pub fn create_campaign_challenge_window(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
        challenge_window: u64,
    ) -> Result<u64, Error> {
        if challenge_window > MAX_CHALLENGE_WINDOW {
            return Err(Error::InvalidChallengeWindow);
        }
        let id = Self::create_campaign(
            env.clone(),
            creator,
            beneficiary,
            verifier,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
        )?;
        if challenge_window > 0 {
            storage::set_challenge_window(&env, id, challenge_window);
        }
        Ok(id)
    }

    /// Proposes the next milestone's approval on a campaign with a challenge
    /// window (Issue #115). Same checks and effect as
    /// [`approve_milestone_multisig`], except that when the threshold is
    /// reached the funds stay in escrow until [`finalize_milestone`]. Returns
    /// `0` because nothing is paid yet. Emits [`MilestoneProposed`].
    ///
    /// # Errors
    /// - [`Error::NoChallengeWindow`]: the campaign pays out immediately.
    /// - [`Error::ProposalPending`]: an earlier proposal is still waiting.
    /// - Otherwise the errors of [`approve_milestone_multisig`].
    pub fn propose_milestone_approval(
        env: Env,
        campaign_id: u64,
        verifier: Address,
        proof_uri: String,
    ) -> Result<i128, Error> {
        if storage::challenge_window(&env, campaign_id) == 0 {
            return Err(Error::NoChallengeWindow);
        }
        Self::internal_approve_milestone(&env, campaign_id, verifier, proof_uri)
    }

    /// Pays out a proposed milestone once its challenge window has passed
    /// (Issue #115). Anyone may call it. Returns the milestone amount and emits
    /// [`MilestoneFinalized`] and [`MilestoneReleased`].
    ///
    /// # Errors
    /// - [`Error::NoProposal`]: nothing is pending for the campaign.
    /// - [`Error::WindowNotElapsed`]: the challenge window is still open.
    /// - Otherwise the funding and status errors of a normal release.
    pub fn finalize_milestone(env: Env, campaign_id: u64) -> Result<i128, Error> {
        Self::ensure_current_schema(&env)?;
        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }
        let campaign = storage::campaign(&env, campaign_id)?;
        let pending = storage::pending_approval(&env, campaign_id).ok_or(Error::NoProposal)?;
        if env.ledger().timestamp() < pending.ready_at {
            return Err(Error::WindowNotElapsed);
        }
        Self::ensure_open(&env, &campaign)?;
        let milestone_amount = campaign
            .milestones
            .get(pending.index)
            .ok_or(Error::NoMilestonesLeft)?;

        storage::remove_pending_approval(&env, campaign_id);
        MilestoneFinalized {
            campaign_id,
            index: pending.index,
        }
        .publish(&env);
        Self::release_milestone(
            &env,
            campaign,
            pending.index,
            milestone_amount,
            pending.proof_uri,
        )
    }

    /// Blocks a proposed approval during its challenge window (Issue #115).
    ///
    /// The admin blocks it alone. A donor adds their contribution to the block
    /// votes, once per proposal, and the proposal is blocked when the votes
    /// reach [`BLOCK_QUORUM_BPS`] of `raised`. A blocked approval returns to
    /// pending: its approvals are cleared so the verifier can propose it again.
    /// Returns `true` if this call blocked it. Emits [`MilestoneBlocked`].
    ///
    /// # Errors
    /// - [`Error::NoProposal`]: nothing is pending for the campaign.
    /// - [`Error::WindowClosed`]: the challenge window has already passed.
    /// - [`Error::AlreadyChallenged`]: this donor already voted on this proposal.
    /// - [`Error::NothingToRefund`]: the caller is neither the admin nor a donor.
    pub fn block_milestone(env: Env, caller: Address, campaign_id: u64) -> Result<bool, Error> {
        caller.require_auth();
        let campaign = storage::campaign(&env, campaign_id)?;
        let mut pending = storage::pending_approval(&env, campaign_id).ok_or(Error::NoProposal)?;
        if env.ledger().timestamp() >= pending.ready_at {
            return Err(Error::WindowClosed);
        }

        let blocked = if caller == storage::admin(&env) {
            true
        } else {
            if storage::has_block_vote(&env, campaign_id, pending.nonce, &caller) {
                return Err(Error::AlreadyChallenged);
            }
            let weight = storage::contribution(&env, campaign_id, &caller);
            if weight <= 0 {
                return Err(Error::NothingToRefund);
            }
            storage::set_block_vote(&env, campaign_id, pending.nonce, &caller);
            pending.block_weight += weight;
            if pending.block_weight * 10_000 >= campaign.raised * BLOCK_QUORUM_BPS {
                true
            } else {
                storage::save_pending_approval(&env, campaign_id, &pending);
                false
            }
        };

        if blocked {
            storage::remove_pending_approval(&env, campaign_id);
            storage::save_milestone_approvals(&env, campaign_id, pending.index, &Vec::new(&env));
            MilestoneBlocked {
                campaign_id,
                index: pending.index,
                by: caller,
            }
            .publish(&env);
        }
        Ok(blocked)
    }

    /// Returns the proposal waiting for `finalize_milestone`, if any.
    pub fn pending_approval(env: Env, campaign_id: u64) -> Option<PendingApproval> {
        storage::pending_approval(&env, campaign_id)
    }

    /// Returns the campaign's challenge window in seconds (`0` for none).
    pub fn challenge_window(env: Env, campaign_id: u64) -> u64 {
        storage::challenge_window(&env, campaign_id)
    }

    /// Updates the off-chain metadata URI for `campaign_id`.
    ///
    /// Only the campaign's creator may call this, and only before any
    /// donations have been received (`raised == 0`). Emits [`MetadataUpdated`].
    ///
    /// # Errors
    /// - [`Error::MigrationPending`]: contract storage migration is pending.
    /// - [`Error::InvalidMetadataUri`]: `new_metadata_uri` is empty.
    /// - [`Error::UriTooLong`]: `new_metadata_uri` exceeds [`MAX_URI_LENGTH`] bytes.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`]: campaign is not currently active.
    /// - [`Error::CampaignExpired`]: campaign deadline has passed.
    /// - [`Error::DonationsAlreadyStarted`]: campaign has already received donations.
    pub fn update_metadata(
        env: Env,
        campaign_id: u64,
        new_metadata_uri: String,
    ) -> Result<(), Error> {
        Self::ensure_current_schema(&env)?;
        let mut campaign = storage::campaign(&env, campaign_id)?;
        campaign.creator.require_auth();
        Self::ensure_open(&env, &campaign)?;

        if new_metadata_uri.is_empty() {
            return Err(Error::InvalidMetadataUri);
        }
        if new_metadata_uri.len() > MAX_URI_LENGTH {
            return Err(Error::UriTooLong);
        }

        if campaign.raised > 0 {
            return Err(Error::DonationsAlreadyStarted);
        }

        campaign.metadata_uri = new_metadata_uri.clone();
        storage::save_campaign(&env, &campaign);

        MetadataUpdated {
            campaign_id,
            new_metadata_uri,
        }
        .publish(&env);

        Ok(())
    }

    /// Extends an active campaign's deadline to `new_deadline` (at most 30 days).
    ///
    /// Only the campaign's creator may call this. An extension can be
    /// granted at most once per campaign, and only while the campaign is
    /// active and before the current deadline has elapsed. Emits
    /// [`DeadlineExtended`].
    ///
    /// # Errors
    /// - [`Error::MigrationPending`]: contract storage migration is pending.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`]: campaign is not in `Active` status.
    /// - [`Error::CampaignExpired`]: current deadline has already elapsed.
    /// - [`Error::DeadlineAlreadyExtended`]: campaign has already used its
    ///   one-time extension.
    /// - [`Error::InvalidDeadlineExtension`]: `new_deadline` is not after the
    ///   current deadline or extends it by more than 30 days.
    pub fn extend_deadline(env: Env, campaign_id: u64, new_deadline: u64) -> Result<(), Error> {
        Self::internal_extend_deadline(&env, campaign_id, |current| {
            let max_deadline = current.saturating_add(MAX_DEADLINE_EXTENSION);
            if new_deadline <= current || new_deadline > max_deadline {
                return Err(Error::InvalidDeadlineExtension);
            }
            Ok(new_deadline)
        })
    }

    /// Extends an active campaign's deadline by `additional_seconds` (1 to 30 days).
    ///
    /// Only the campaign's creator may call this. An extension can be
    /// granted at most once per campaign, and only while the campaign is
    /// active and before the current deadline has elapsed. Emits
    /// [`DeadlineExtended`].
    ///
    /// # Errors
    /// - [`Error::MigrationPending`]: contract storage migration is pending.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`]: campaign is not in `Active` status.
    /// - [`Error::CampaignExpired`]: current deadline has already elapsed.
    /// - [`Error::DeadlineAlreadyExtended`]: campaign has already used its
    ///   one-time extension.
    /// - [`Error::InvalidDeadlineExtension`]: `additional_seconds` is zero,
    ///   exceeds 30 days, or overflows `u64`.
    pub fn extend_deadline_by(
        env: Env,
        campaign_id: u64,
        additional_seconds: u64,
    ) -> Result<(), Error> {
        Self::internal_extend_deadline(&env, campaign_id, |current| {
            if additional_seconds == 0 || additional_seconds > MAX_DEADLINE_EXTENSION {
                return Err(Error::InvalidDeadlineExtension);
            }
            current
                .checked_add(additional_seconds)
                .ok_or(Error::InvalidDeadlineExtension)
        })
    }

    fn internal_extend_deadline(
        env: &Env,
        campaign_id: u64,
        compute_new_deadline: impl FnOnce(u64) -> Result<u64, Error>,
    ) -> Result<(), Error> {
        Self::ensure_current_schema(env)?;
        let mut campaign = storage::campaign(env, campaign_id)?;
        campaign.creator.require_auth();
        Self::ensure_open(env, &campaign)?;

        if campaign.deadline_extended {
            return Err(Error::DeadlineAlreadyExtended);
        }

        let old_deadline = campaign.deadline;
        let new_deadline = compute_new_deadline(old_deadline)?;
        campaign.deadline = new_deadline;
        campaign.deadline_extended = true;
        storage::save_campaign(env, &campaign);
        storage::extend_milestone_due_dates_ttl(env, campaign_id, new_deadline);

        DeadlineExtended {
            campaign_id,
            old_deadline,
            new_deadline,
        }
        .publish(env);

        Ok(())
    }

    /// Donates `amount` tokens from `donor` to `campaign_id`.
    ///
    /// Accepts up to the remaining amount needed to reach the campaign goal
    /// (`min(amount, goal - raised)`). Only that accepted amount is transferred
    /// into escrow, and the accepted amount is returned.
    ///
    /// The campaign must be active and not expired. If the campaign is already
    /// fully funded (`raised >= goal`), the call fails with [`Error::GoalExceeded`].
    /// Emits [`Donated`] with the accepted amount.
    ///
    /// # Errors
    /// - [`Error::InvalidAmount`]: `amount` is below [`MIN_DONATION_AMOUNT`].
    /// - [`Error::DonationBelowMinimum`]: `amount` is below the campaign's
    ///   `min_donation` and does not reach the remaining goal.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`]: campaign is not in `Active` status.
    /// - [`Error::CampaignExpired`]: campaign deadline has passed.
    /// - [`Error::MilestoneOverdue`]: a milestone is overdue.
    /// - [`Error::GoalExceeded`]: campaign is already fully funded.
    pub fn donate(env: Env, donor: Address, campaign_id: u64, amount: i128) -> Result<i128, Error> {
        Self::ensure_current_schema(&env)?;
        donor.require_auth();
        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }
        if amount < MIN_DONATION_AMOUNT {
            return Err(Error::InvalidAmount);
        }

        let mut campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        if campaign.raised >= campaign.goal {
            return Err(Error::GoalExceeded);
        }

        let remaining = campaign.goal - campaign.raised;
        if campaign.min_donation > 0 && amount < campaign.min_donation && amount < remaining {
            return Err(Error::DonationBelowMinimum);
        }

        let accepted = amount.min(remaining);

        token::Client::new(&env, &campaign.token).transfer(
            &donor,
            env.current_contract_address(),
            &accepted,
        );

        campaign.raised = campaign
            .raised
            .checked_add(accepted)
            .ok_or(Error::InvalidAmount)?;
        storage::save_campaign(&env, &campaign);
        let previous = storage::contribution(&env, campaign_id, &donor);
        storage::set_contribution(&env, campaign_id, &donor, previous + accepted);
        let unmatched = storage::unmatched_donation(&env, campaign_id, &donor);
        storage::set_unmatched_donation(&env, campaign_id, &donor, unmatched + accepted);
        storage::add_campaign_donor(&env, campaign_id, &donor);

        Donated {
            campaign_id,
            donor: donor.clone(),
            amount: accepted,
        }
        .publish(&env);

        Ok(accepted)
    }

    // ─── Sponsor matching ─────────────────────────────────────────────────────

    /// Deposits a sponsor matching pool for `campaign_id`.
    ///
    /// The sponsor pre-funds the pool with `amount` tokens. For each
    /// subsequent call to [`apply_matching`], the contract matches eligible
    /// donations at `ratio_bps` basis points up to `cap`. `ratio_bps` must be
    /// between 1 and 10 000 inclusive. `cap` and `amount` must both be
    /// positive and equal. Only one pool per sponsor per campaign is allowed.
    /// Emits [`SponsorPoolDeposited`].
    ///
    /// # Errors
    /// - [`Error::Paused`]: the contract is currently paused.
    /// - [`Error::InvalidMatchingConfig`]: `ratio_bps` is 0 or above 10 000,
    ///   `cap` or `amount` are non-positive, `amount < MIN_SPONSOR_POOL`, or
    ///   `amount != cap`.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`] / [`Error::MilestoneOverdue`]: campaign
    ///   is not open.
    /// - [`Error::SponsorPoolExists`]: a pool for this sponsor and campaign
    ///   already exists.
    pub fn deposit_sponsor_pool(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
        ratio_bps: u32,
        cap: i128,
        amount: i128,
    ) -> Result<(), Error> {
        Self::ensure_current_schema(&env)?;
        sponsor.require_auth();

        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }
        if ratio_bps == 0 || ratio_bps > MAX_RATIO_BPS {
            return Err(Error::InvalidMatchingConfig);
        }
        if cap <= 0 || amount <= 0 || amount < MIN_SPONSOR_POOL {
            return Err(Error::InvalidMatchingConfig);
        }
        if amount != cap {
            return Err(Error::InvalidMatchingConfig);
        }

        let campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        if storage::sponsor_pool(&env, campaign_id, &sponsor).is_some() {
            return Err(Error::SponsorPoolExists);
        }

        token::Client::new(&env, &campaign.token).transfer(
            &sponsor,
            env.current_contract_address(),
            &amount,
        );

        let pool = SponsorPool {
            sponsor: sponsor.clone(),
            campaign_id,
            ratio_bps,
            cap,
            deposited: amount,
            remaining: amount,
            matched: 0,
        };
        storage::save_sponsor_pool(&env, &pool);

        SponsorPoolDeposited {
            campaign_id,
            sponsor,
            amount,
            ratio_bps,
            cap,
        }
        .publish(&env);

        Ok(())
    }

    /// Applies sponsor matching for a donation, crediting matched tokens to
    /// the campaign's raised total.
    ///
    /// Any account may call this. The match is computed as
    /// `min(donation_amount, donor's unmatched donations) * ratio_bps / 10_000`,
    /// so only real donations that were not matched before can be matched. It is
    /// clamped to the pool's cap
    /// headroom, remaining balance, and campaign headroom. Returns the actual
    /// matched amount (may be `0` if the pool is exhausted or the donation is
    /// too small). Emits [`MatchingApplied`] when the matched amount is > 0.
    ///
    /// # Errors
    /// - [`Error::InvalidAmount`]: `donation_amount` is zero or negative, or
    ///   the multiplication overflows.
    /// - [`Error::SponsorPoolNotFound`]: no pool exists for this sponsor and
    ///   campaign.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`] / [`Error::MilestoneOverdue`]: campaign
    ///   is not open.
    pub fn apply_matching(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
        donor: Address,
        donation_amount: i128,
    ) -> Result<i128, Error> {
        Self::ensure_current_schema(&env)?;
        if donation_amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let mut pool =
            storage::sponsor_pool(&env, campaign_id, &sponsor).ok_or(Error::SponsorPoolNotFound)?;

        let mut campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        // Only real, not yet matched donations by `donor` can be matched.
        let unmatched = storage::unmatched_donation(&env, campaign_id, &donor);
        let eligible = donation_amount.min(unmatched);
        if eligible <= 0 {
            return Ok(0);
        }

        let raw_match = eligible
            .checked_mul(pool.ratio_bps as i128)
            .ok_or(Error::InvalidAmount)?
            / 10_000_i128;

        if raw_match <= 0 {
            return Ok(0);
        }

        let cap_remaining = pool.cap - pool.matched;
        let pool_limit = pool.remaining;
        let campaign_headroom = campaign.goal - campaign.raised;

        let matched_amount = raw_match
            .min(cap_remaining)
            .min(pool_limit)
            .min(campaign_headroom);

        if matched_amount <= 0 {
            return Ok(0);
        }

        // Consume the part of the donation that this match covers (rounded up),
        // so the same donation cannot be matched again.
        let consumed = eligible
            .checked_mul(matched_amount)
            .map(|x| (x + raw_match - 1) / raw_match)
            .unwrap_or(eligible)
            .min(eligible);
        storage::set_unmatched_donation(&env, campaign_id, &donor, unmatched - consumed);

        pool.remaining -= matched_amount;
        pool.matched += matched_amount;
        storage::save_sponsor_pool(&env, &pool);

        campaign.raised += matched_amount;
        storage::save_campaign(&env, &campaign);

        let prev = storage::contribution(&env, campaign_id, &sponsor);
        storage::set_contribution(&env, campaign_id, &sponsor, prev + matched_amount);

        MatchingApplied {
            campaign_id,
            sponsor: sponsor.clone(),
            donor,
            donation_amount: eligible,
            matched_amount,
        }
        .publish(&env);

        Ok(matched_amount)
    }

    /// Returns the sponsor's unmatched pool balance after the campaign ends.
    ///
    /// Can only be called once the campaign is no longer active (completed,
    /// cancelled, or past its deadline). The remaining pool balance is
    /// transferred back to `sponsor` and the pool record is deleted. Emits
    /// [`SponsorPoolReturned`] and returns the amount returned.
    ///
    /// # Errors
    /// - [`Error::SponsorPoolNotFound`]: no pool exists for this sponsor and
    ///   campaign.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::PoolNotReturnable`]: campaign is still active and has not
    ///   yet expired.
    pub fn return_sponsor_pool(
        env: Env,
        sponsor: Address,
        campaign_id: u64,
    ) -> Result<i128, Error> {
        sponsor.require_auth();

        let pool =
            storage::sponsor_pool(&env, campaign_id, &sponsor).ok_or(Error::SponsorPoolNotFound)?;

        let campaign = storage::campaign(&env, campaign_id)?;

        let expired = Self::is_expired(&env, &campaign);
        let ended = campaign.status != CampaignStatus::Active || expired;
        if !ended {
            return Err(Error::PoolNotReturnable);
        }

        let remaining = pool.remaining;
        storage::remove_sponsor_pool(&env, campaign_id, &sponsor);

        if remaining > 0 {
            token::Client::new(&env, &campaign.token).transfer(
                &env.current_contract_address(),
                &sponsor,
                &remaining,
            );
        }

        SponsorPoolReturned {
            campaign_id,
            sponsor,
            amount: remaining,
        }
        .publish(&env);

        Ok(remaining)
    }

    // ─── Pledges ──────────────────────────────────────────────────────────────

    /// Records a pledge from `donor` for `campaign_id`.
    ///
    /// A donor creates a pledge for a campaign. The donor must have granted
    /// the Aidline contract a token allowance of at least `pledged_amount`
    /// before calling this. The pledge is recorded persistently and funds are
    /// pulled proportionally when milestones are approved.
    ///
    /// `pledged_amount` must be > 0. The campaign must be active and not
    /// expired. Emits [`PledgeCreated`] and returns the pledge ID.
    ///
    /// # Errors
    /// - [`Error::InvalidPledge`]: `pledged_amount` is zero or negative, or
    ///   the donor's current token allowance is below `pledged_amount`.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`] / [`Error::MilestoneOverdue`]: campaign
    ///   is not open.
    pub fn create_pledge(
        env: Env,
        donor: Address,
        campaign_id: u64,
        pledged_amount: i128,
    ) -> Result<u64, Error> {
        Self::ensure_current_schema(&env)?;
        donor.require_auth();

        if pledged_amount <= 0 {
            return Err(Error::InvalidPledge);
        }

        let campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        // Verify the donor has granted sufficient allowance.
        // We do not transfer tokens here, they are pulled at milestone approval.
        let tok = token::Client::new(&env, &campaign.token);
        let allowance = tok.allowance(&donor, &env.current_contract_address());
        if allowance < pledged_amount {
            return Err(Error::InvalidPledge);
        }

        let pledge_id = storage::next_pledge_id(&env);
        let pledge = Pledge {
            pledge_id,
            donor: donor.clone(),
            campaign_id,
            pledged_amount,
            pulled_amount: 0,
            active: true,
        };
        storage::save_pledge(&env, &pledge);
        let mut campaign_pledges = storage::campaign_pledges(&env, campaign_id);
        campaign_pledges.push_back(pledge_id);
        storage::save_campaign_pledges(&env, campaign_id, &campaign_pledges);

        PledgeCreated {
            pledge_id,
            campaign_id,
            donor,
            pledged_amount,
        }
        .publish(&env);

        Ok(pledge_id)
    }

    /// Creates a campaign whose milestones can release linearly over time and
    /// returns its ID (Issue #117).
    ///
    /// `stream_days` has one entry per milestone. `0` keeps that milestone a
    /// lump sum; a positive value makes its payout vest linearly over that many
    /// days after approval, claimed with [`claim_streamed`]. Other arguments and
    /// errors are those of [`create_campaign`].
    ///
    /// # Errors
    /// - [`Error::InvalidStream`]: `stream_days` and `milestones` differ in length.
    pub fn create_campaign_with_streams(
        env: Env,
        creator: Address,
        beneficiary: Address,
        verifier: Address,
        token: Address,
        kind: CampaignKind,
        metadata_uri: String,
        deadline: u64,
        milestones: Vec<i128>,
        stream_days: Vec<u32>,
    ) -> Result<u64, Error> {
        if stream_days.len() != milestones.len() {
            return Err(Error::InvalidStream);
        }
        let id = Self::create_campaign(
            env.clone(),
            creator,
            beneficiary,
            verifier,
            token,
            kind,
            metadata_uri,
            deadline,
            milestones,
        )?;
        if stream_days.iter().any(|days| days > 0) {
            storage::set_milestone_stream_days(&env, id, &stream_days);
        }
        Ok(id)
    }

    /// Pays the beneficiary everything that has vested from the campaign's
    /// streamed milestones and not yet been claimed, and returns the amount.
    ///
    /// Only the beneficiary may call this, and can do so as often as they like.
    /// Emits [`StreamClaimed`].
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::NothingToClaim`]: nothing has vested since the last claim.
    pub fn claim_streamed(env: Env, campaign_id: u64) -> Result<i128, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        campaign.beneficiary.require_auth();

        let mut claimable: i128 = 0;
        for index in 0..campaign.milestones_released {
            if let Some(mut stream) = storage::stream(&env, campaign_id, index) {
                let due = Self::vested(&env, &stream) - stream.claimed;
                if due > 0 {
                    stream.claimed += due;
                    storage::save_stream(&env, campaign_id, index, &stream);
                    claimable += due;
                }
            }
        }
        if claimable <= 0 {
            return Err(Error::NothingToClaim);
        }

        Self::pay_beneficiaries(&env, &campaign, claimable);
        StreamClaimed {
            campaign_id,
            amount: claimable,
        }
        .publish(&env);
        Ok(claimable)
    }

    /// Votes to cancel a campaign, weighted by the donor's contribution
    /// (Issue #114).
    ///
    /// Each donor votes once while the campaign is `Active`. Once the votes
    /// reach [`CANCEL_VOTE_BPS`] of `raised` and the campaign has made no
    /// progress for [`STALL_PERIOD`], the campaign is cancelled and refunds
    /// open. Returns `true` if this vote cancelled it. Emits [`CancelVoted`],
    /// and [`CampaignCancelled`] when it cancels.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`] / [`Error::CampaignNotActive`]
    /// - [`Error::AlreadyVoted`]: this donor already has a vote on the campaign.
    /// - [`Error::NothingToRefund`]: the donor has no contribution to weigh.
    pub fn vote_cancel(env: Env, donor: Address, campaign_id: u64) -> Result<bool, Error> {
        donor.require_auth();
        let mut campaign = storage::campaign(&env, campaign_id)?;
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }
        if storage::cancel_vote(&env, campaign_id, &donor) > 0 {
            return Err(Error::AlreadyVoted);
        }
        let weight = storage::contribution(&env, campaign_id, &donor);
        if weight <= 0 {
            return Err(Error::NothingToRefund);
        }

        let total = storage::cancel_vote_total(&env, campaign_id) + weight;
        storage::set_cancel_vote(&env, campaign_id, &donor, weight);
        storage::set_cancel_vote_total(&env, campaign_id, total);
        CancelVoted {
            campaign_id,
            donor,
            weight,
            total,
        }
        .publish(&env);

        Ok(Self::cancel_by_vote(&env, &mut campaign).is_ok())
    }

    /// Withdraws the donor's cancel vote while the campaign is still `Active`.
    /// Emits [`CancelVoteWithdrawn`].
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`] / [`Error::CampaignNotActive`]
    /// - [`Error::NoVote`]: the donor has no vote on this campaign.
    pub fn withdraw_cancel_vote(env: Env, donor: Address, campaign_id: u64) -> Result<(), Error> {
        donor.require_auth();
        let campaign = storage::campaign(&env, campaign_id)?;
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }
        let weight = storage::cancel_vote(&env, campaign_id, &donor);
        if weight <= 0 {
            return Err(Error::NoVote);
        }

        let total = storage::cancel_vote_total(&env, campaign_id) - weight;
        storage::set_cancel_vote(&env, campaign_id, &donor, 0);
        storage::set_cancel_vote_total(&env, campaign_id, total);
        CancelVoteWithdrawn {
            campaign_id,
            donor,
            weight,
            total,
        }
        .publish(&env);
        Ok(())
    }

    /// Cancels the campaign if its votes already reached the threshold and the
    /// stall period has now passed. Anyone may call it; this is how votes cast
    /// before the stall period ended take effect (Issue #114).
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`] / [`Error::CampaignNotActive`]
    /// - [`Error::NotStalled`]: the stall period has not passed.
    /// - [`Error::VoteThresholdNotReached`]: the votes are below the threshold.
    pub fn execute_vote_cancel(env: Env, campaign_id: u64) -> Result<(), Error> {
        let mut campaign = storage::campaign(&env, campaign_id)?;
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }
        Self::cancel_by_vote(&env, &mut campaign)
    }

    /// Returns the total weight of cancel votes on `campaign_id`.
    pub fn cancel_vote_weight(env: Env, campaign_id: u64) -> i128 {
        storage::cancel_vote_total(&env, campaign_id)
    }

    /// Cancels an active pledge, marking it inactive.
    ///
    /// Only the donor who made the pledge can cancel it. Milestone pulls and
    /// settlements will skip cancelled pledges without error. Emits [`PledgeCancelled`].
    ///
    /// # Errors
    /// - [`Error::InvalidPledge`]: no pledge with `pledge_id` exists.
    /// - [`Error::Unauthorized`]: caller is not the donor of the pledge.
    /// - [`Error::PledgeAlreadyCancelled`]: the pledge has already been cancelled or is inactive.
    pub fn cancel_pledge(env: Env, donor: Address, pledge_id: u64) -> Result<(), Error> {
        donor.require_auth();
        storage::bump_instance(&env);

        let mut pledge = storage::pledge(&env, pledge_id).ok_or(Error::InvalidPledge)?;
        if pledge.donor != donor {
            return Err(Error::Unauthorized);
        }
        if !pledge.active {
            return Err(Error::PledgeAlreadyCancelled);
        }

        pledge.active = false;
        storage::save_pledge(&env, &pledge);

        PledgeCancelled {
            pledge_id,
            campaign_id: pledge.campaign_id,
            donor,
        }
        .publish(&env);

        Ok(())
    }

    /// Releases an emergency fast-track advance for an `Emergency` campaign.
    ///
    /// Only the campaign's registered verifier may call this. Can only be
    /// used once per campaign and only on `Emergency` campaigns. The advance
    /// is capped at 20% of `campaign.goal` and cannot exceed the currently
    /// available escrow (`raised - released`). Emits
    /// [`EmergencyAdvanceReleased`] and returns the amount transferred.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::NotEmergencyCampaign`]: campaign `kind` is not `Emergency`.
    /// - [`Error::NotVerifier`]: the campaign's verifier is no longer
    ///   registered.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`]: campaign
    ///   is not open.
    /// - [`Error::AdvanceAlreadyTaken`]: the fast-track has already been used
    ///   on this campaign.
    /// - [`Error::AdvanceExceedsEscrow`]: no funds are currently in escrow.
    /// - [`Error::InvalidAmount`]: emergency advance cap calculation overflows.
    pub fn emergency_fast_track(env: Env, campaign_id: u64) -> Result<i128, Error> {
        Self::ensure_current_schema(&env)?;
        let mut campaign = storage::campaign(&env, campaign_id)?;
        campaign.verifier.require_auth();

        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }

        // Only Emergency campaigns may use the fast track.
        if campaign.kind != CampaignKind::Emergency {
            return Err(Error::NotEmergencyCampaign);
        }
        if !storage::is_verifier(&env, &campaign.verifier) {
            return Err(Error::NotVerifier);
        }
        Self::ensure_open(&env, &campaign)?;

        // Fast track can only be used once.
        if campaign.emergency_advance > 0 {
            return Err(Error::AdvanceAlreadyTaken);
        }

        // Cap: 20% of goal, rounded down, and never more than milestone 0.
        let m0 = campaign.milestones.get(0).ok_or(Error::NoMilestonesLeft)?;
        let cap = campaign
            .goal
            .checked_mul(EMERGENCY_ADVANCE_BPS)
            .ok_or(Error::InvalidAmount)?
            / BPS_DENOM;
        let cap = cap.min(m0);

        // Available escrow = raised − already released.
        let available = campaign.raised - campaign.released;
        if available <= 0 {
            return Err(Error::AdvanceExceedsEscrow);
        }

        // Advance is the lesser of the 20% cap and available escrow.
        let advance = if available < cap { available } else { cap };

        Self::pay_beneficiaries(&env, &campaign, advance);

        // Record: advance is accounted as both released and emergency_advance.
        campaign.released += advance;
        campaign.emergency_advance = advance;
        storage::save_campaign(&env, &campaign);

        EmergencyAdvanceReleased {
            campaign_id,
            verifier: campaign.verifier.clone(),
            milestone_index: 0,
            amount: advance,
        }
        .publish(&env);

        Ok(advance)
    }

    /// Approves the next milestone for a campaign as `verifier`.
    ///
    /// For single-verifier campaigns, releases the milestone immediately.
    /// For multisig campaigns, records `verifier`'s approval and releases the
    /// milestone once the campaign's `threshold` is met. Returns the scheduled
    /// milestone amount if released, or `0` if more approvals are required.
    /// Emits [`MilestoneApproved`] on each approval, and [`MilestoneReleased`]
    /// when the threshold is reached.
    ///
    /// # Errors
    /// - [`Error::DuplicateApproval`]: `verifier` has already approved this milestone.
    /// - [`Error::NotVerifier`]: `verifier` is not an assigned verifier for this campaign
    ///   or is not currently registered.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`]: campaign is not open.
    /// - [`Error::NoMilestonesLeft`]: all milestones have already been released.
    /// - [`Error::MilestoneNotFunded`]: raised funds insufficient to cover milestone.
    pub fn approve_milestone_multisig(
        env: Env,
        campaign_id: u64,
        verifier: Address,
        proof_uri: String,
    ) -> Result<i128, Error> {
        Self::internal_approve_milestone(&env, campaign_id, verifier, proof_uri)
    }

    /// Approves the next milestone for `campaign_id` and pays it to the
    /// beneficiary.
    ///
    /// In a single-verifier campaign, authorizes on the campaign's registered
    /// verifier and releases the milestone. For multisig campaigns with
    /// `threshold > 1`, callers should use [`approve_milestone_multisig`] to
    /// identify and authorize which verifier is signing.
    ///
    /// # Errors
    /// - [`Error::InvalidProofUri`]: `proof_uri` is empty or exceeds [`MAX_PROOF_URI_LENGTH`] bytes.
    /// - [`Error::Unauthorized`]: campaign has multiple verifiers requiring specific authorization.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::NotVerifier`]: the campaign's verifier is no longer registered.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`]: campaign is not open.
    /// - [`Error::NoMilestonesLeft`]: all milestones have already been released.
    /// - [`Error::MilestoneNotFunded`]: raised funds insufficient to cover the next milestone.
    pub fn approve_milestone(env: Env, campaign_id: u64, proof_uri: String) -> Result<i128, Error> {
        if proof_uri.is_empty() || proof_uri.len() > MAX_PROOF_URI_LENGTH {
            return Err(Error::InvalidProofUri);
        }
        let campaign = storage::campaign(&env, campaign_id)?;
        if campaign.verifiers.len() > 1 && campaign.threshold > 1 {
            return Err(Error::Unauthorized);
        }
        Self::internal_approve_milestone(&env, campaign_id, campaign.verifier.clone(), proof_uri)
    }

    /// Records a verifier's rejection of the next pending milestone with a structured reason code
    /// and an explanatory details URI.
    ///
    /// Only the campaign's registered verifier may call this. Does not move funds or advance
    /// milestone progress. If `reason` corresponds to [`RejectionReason::FraudDetected`] (`2`),
    /// the campaign is immediately cancelled so donors can reclaim unreleased funds.
    /// Emits [`MilestoneRejected`] (and [`CampaignCancelled`] when fraud is reported).
    ///
    /// # Errors
    /// - [`Error::MigrationPending`]: contract storage migration is pending.
    /// - [`Error::UriTooLong`]: `details_uri` exceeds [`MAX_URI_LENGTH`] bytes.
    /// - [`Error::InvalidRejectionReason`]: `reason` is not a valid [`RejectionReason`] code (`1..=4`).
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::NotVerifier`]: the campaign verifier is no longer registered.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`]: campaign is not open.
    /// - [`Error::NoMilestonesLeft`]: all milestones have already been released.
    pub fn reject_milestone(
        env: Env,
        campaign_id: u64,
        reason: u32,
        details_uri: String,
    ) -> Result<(), Error> {
        Self::ensure_current_schema(&env)?;
        if details_uri.len() > MAX_URI_LENGTH {
            return Err(Error::UriTooLong);
        }

        let rejection_reason =
            RejectionReason::from_code(reason).ok_or(Error::InvalidRejectionReason)?;

        let mut campaign = storage::campaign(&env, campaign_id)?;
        campaign.verifier.require_auth();

        if !storage::is_verifier(&env, &campaign.verifier) {
            return Err(Error::NotVerifier);
        }

        Self::ensure_open(&env, &campaign)?;

        let index = campaign.milestones_released;
        if index >= campaign.milestones.len() {
            return Err(Error::NoMilestonesLeft);
        }

        MilestoneRejected {
            campaign_id,
            index,
            reason,
            details_uri,
        }
        .publish(&env);

        if rejection_reason == RejectionReason::FraudDetected {
            Self::cancel_campaign_state(&env, &mut campaign, true);
        }

        Ok(())
    }

    fn internal_approve_milestone(
        env: &Env,
        campaign_id: u64,
        verifier: Address,
        proof_uri: String,
    ) -> Result<i128, Error> {
        if proof_uri.is_empty() || proof_uri.len() > MAX_PROOF_URI_LENGTH {
            return Err(Error::InvalidProofUri);
        }
        Self::ensure_current_schema(env)?;
        verifier.require_auth();

        if storage::is_paused(env) {
            return Err(Error::Paused);
        }

        let campaign = storage::campaign(env, campaign_id)?;
        if !campaign.verifiers.contains(&verifier) {
            return Err(Error::NotVerifier);
        }
        if !storage::is_verifier(env, &verifier) {
            return Err(Error::NotVerifier);
        }
        Self::ensure_open(env, &campaign)?;
        if storage::pending_approval(env, campaign_id).is_some() {
            return Err(Error::ProposalPending);
        }

        let index = campaign.milestones_released;
        let milestone_amount = campaign
            .milestones
            .get(index)
            .ok_or(Error::NoMilestonesLeft)?;

        if campaign.approval_cooldown > 0 && campaign.last_approval_time > 0 {
            let earliest_approval = campaign
                .last_approval_time
                .saturating_add(campaign.approval_cooldown);
            if env.ledger().timestamp() < earliest_approval {
                return Err(Error::ApprovalCooldownNotMet);
            }
        }

        let mut approvals = storage::milestone_approvals(env, campaign_id, index);
        if approvals.contains(&verifier) {
            return Err(Error::DuplicateApproval);
        }
        approvals.push_back(verifier.clone());
        storage::save_milestone_approvals(env, campaign_id, index, &approvals);
        let mut proofs = storage::milestone_approval_proofs(env, campaign_id, index);
        proofs.push_back(proof_uri.clone());
        storage::save_milestone_approval_proofs(env, campaign_id, index, &proofs);
        Self::bump_verifier(env, &verifier, |s| s.milestones_approved += 1);

        MilestoneApproved {
            campaign_id,
            milestone_index: index,
            verifier,
            approvals_count: approvals.len(),
            threshold: campaign.threshold,
        }
        .publish(env);

        if approvals.len() < campaign.threshold {
            return Ok(0);
        }

        // With a challenge window the funds do not move yet: the approval waits
        // for `finalize_milestone` and can be blocked in the meantime (#115).
        let window = storage::challenge_window(env, campaign_id);
        if window > 0 {
            let now = env.ledger().timestamp();
            let ready_at = now.saturating_add(window);
            storage::save_pending_approval(
                env,
                campaign_id,
                &PendingApproval {
                    index,
                    proof_uri,
                    proposed_at: now,
                    ready_at,
                    block_weight: 0,
                    nonce: storage::next_proposal_nonce(env, campaign_id),
                },
            );
            MilestoneProposed {
                campaign_id,
                index,
                ready_at,
            }
            .publish(env);
            return Ok(0);
        }

        Self::release_milestone(env, campaign, index, milestone_amount, proof_uri)
    }

    /// Pays out the milestone `index` of `campaign`: pulls pledges, checks the
    /// escrow covers it, pays the beneficiary and the platform fee and advances
    /// the campaign. Shared by immediate approval and `finalize_milestone`.
    fn release_milestone(
        env: &Env,
        mut campaign: Campaign,
        index: u32,
        milestone_amount: i128,
        proof_uri: String,
    ) -> Result<i128, Error> {
        let campaign_id = campaign.id;
        let total_pledged_pulled =
            Self::pull_pledges(env, campaign_id, milestone_amount, &mut campaign);

        PledgeSettlement {
            campaign_id,
            milestone_index: index,
            total_pledged_pulled,
        }
        .publish(env);

        let already_paid = if index == 0 {
            campaign.emergency_advance
        } else {
            0
        };
        let remaining = milestone_amount.saturating_sub(already_paid);

        let available = campaign.raised - campaign.released;
        if available < remaining {
            return Err(Error::MilestoneNotFunded);
        }

        let fee_bps = storage::platform_fee(env);
        let treasury = if fee_bps > 0 {
            storage::treasury(env)
        } else {
            None
        };
        let fee = if fee_bps > 0 && treasury.is_some() {
            let raw_fee = milestone_amount.checked_mul(fee_bps as i128).unwrap_or(0) / BPS_DENOM;
            raw_fee.min(remaining)
        } else {
            0
        };

        let beneficiary_payout = remaining - fee;

        if beneficiary_payout > 0 {
            let stream_days = storage::milestone_stream_days(env, campaign_id)
                .and_then(|days| days.get(index))
                .unwrap_or(0);
            if stream_days > 0 {
                // A streamed milestone vests over time instead of paying out
                // now; the funds stay in escrow until claimed (Issue #117).
                storage::save_stream(
                    env,
                    campaign_id,
                    index,
                    &Stream {
                        total: beneficiary_payout,
                        start: env.ledger().timestamp(),
                        duration: stream_days as u64 * 86_400,
                        claimed: 0,
                    },
                );
            } else {
                Self::pay_beneficiaries(env, &campaign, beneficiary_payout);
            }
        }

        if fee > 0
            && let Some(ref treasury_addr) = treasury
        {
            token::Client::new(env, &campaign.token).transfer(
                &env.current_contract_address(),
                treasury_addr,
                &fee,
            );
        }

        campaign.released += remaining;
        campaign.milestones_released += 1;
        campaign.last_approval_time = env.ledger().timestamp();
        if campaign.milestones_released == campaign.milestones.len() {
            campaign.status = CampaignStatus::Completed;
            Self::close_verifier_assignments(env, &campaign.verifiers);
            Self::bump_all_verifiers(env, &campaign.verifiers, |s| s.campaigns_completed += 1);
        }
        storage::save_campaign(env, &campaign);

        MilestoneReleased {
            campaign_id,
            index,
            amount: milestone_amount,
            fee,
            proof_uri,
        }
        .publish(env);
        Ok(milestone_amount)
    }

    /// Stops a campaign early so donors can reclaim unreleased funds.
    ///
    /// Only the campaign's creator or the admin may call this. The campaign
    /// must currently be `Active`. Emits [`CampaignCancelled`].
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::Unauthorized`]: caller is neither the creator nor the admin.
    /// - [`Error::CampaignNotActive`]: campaign is not in `Active` status.
    pub fn cancel_campaign(env: Env, caller: Address, campaign_id: u64) -> Result<(), Error> {
        caller.require_auth();
        let mut campaign = storage::campaign(&env, campaign_id)?;
        if caller != campaign.creator && Self::require_role(&env, &caller, Role::Admin).is_err() {
            return Err(Error::Unauthorized);
        }
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }

        Self::cancel_campaign_state(&env, &mut campaign, true);
        Ok(())
    }

    /// Stops a campaign early with an explanatory reason URI so donors and indexers know why.
    ///
    /// Only the campaign's creator or the admin may call this. The campaign must currently
    /// be `Active`. `reason_uri` must not exceed [`MAX_URI_LENGTH`]. Emits [`CampaignCancelledWithReason`].
    ///
    /// # Errors
    /// - [`Error::UriTooLong`]: `reason_uri` exceeds [`MAX_URI_LENGTH`] bytes.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::Unauthorized`]: caller is neither the creator nor the admin.
    /// - [`Error::CampaignNotActive`]: campaign is not in `Active` status.
    pub fn cancel_campaign_with_reason(
        env: Env,
        caller: Address,
        campaign_id: u64,
        reason_uri: String,
    ) -> Result<(), Error> {
        if reason_uri.len() > MAX_URI_LENGTH {
            return Err(Error::UriTooLong);
        }
        caller.require_auth();
        let mut campaign = storage::campaign(&env, campaign_id)?;
        if caller != campaign.creator && Self::require_role(&env, &caller, Role::Admin).is_err() {
            return Err(Error::Unauthorized);
        }
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }

        Self::cancel_campaign_state(&env, &mut campaign, false);

        CampaignCancelledWithReason {
            campaign_id,
            caller,
            reason_uri,
        }
        .publish(&env);
        Ok(())
    }

    /// Returns the donor's pro rata share of unreleased escrow funds.
    ///
    /// A refund is available when the campaign is cancelled, past its
    /// deadline, or a per-milestone due date has been missed. The refund
    /// amount is `contribution * (raised - released + slashed_recovery) / raised`. After the
    /// transfer the donor's contribution record is zeroed. Emits [`Refunded`]
    /// and returns the amount transferred.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::RefundNotAvailable`]: campaign is still active, not expired,
    ///   and no milestone is overdue.
    /// - [`Error::NothingToRefund`]: `donor` has no recorded contribution.
    pub fn refund(env: Env, donor: Address, campaign_id: u64) -> Result<i128, Error> {
        donor.require_auth();
        Self::execute_refund(&env, &donor, campaign_id)
    }

    /// Triggers a refund on behalf of `donor` by any account.
    ///
    /// Tokens are always transferred to `donor`, never to the caller.
    /// Requires no authorization from `donor`, enabling relayers, NGOs, or
    /// keepers to push refunds to donors without donor transaction fees or interaction.
    /// Emits [`Refunded`] and returns the amount transferred.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::RefundNotAvailable`]: campaign is still active, not expired,
    ///   and no milestone is overdue.
    /// - [`Error::NothingToRefund`]: `donor` has no recorded contribution.
    pub fn refund_for(env: Env, donor: Address, campaign_id: u64) -> Result<i128, Error> {
        Self::execute_refund(&env, &donor, campaign_id)
    }

    fn execute_refund(env: &Env, donor: &Address, campaign_id: u64) -> Result<i128, Error> {
        let (campaign, amount) = Self::take_refund(env, donor, campaign_id)?;

        if amount > 0 {
            token::Client::new(env, &campaign.token).transfer(
                &env.current_contract_address(),
                donor,
                &amount,
            );
        }

        Refunded {
            campaign_id,
            donor: donor.clone(),
            amount,
        }
        .publish(env);
        Ok(amount)
    }

    /// Checks refund eligibility, zeroes `donor`'s contribution and returns the
    /// campaign with the refund amount, without moving any tokens.
    fn take_refund(
        env: &Env,
        donor: &Address,
        campaign_id: u64,
    ) -> Result<(Campaign, i128), Error> {
        Self::ensure_current_schema(env)?;
        let mut campaign = storage::campaign(env, campaign_id)?;

        let expired = Self::is_expired(env, &campaign);
        let ended = campaign.status == CampaignStatus::Cancelled || expired;

        if !ended {
            // Only an open campaign needs its due dates read.
            if !Self::is_overdue(env, &campaign) {
                return Err(Error::RefundNotAvailable);
            }
            Self::cancel_campaign_state(env, &mut campaign, true);
        } else if expired && Self::settle_streams(env, &mut campaign) {
            // A campaign that expired was never cancelled, so its streams are
            // settled the first time a refund is taken (Issue #117). Every
            // cancellation path settles them already.
            storage::save_campaign(env, &campaign);
        }

        let contributed = storage::contribution(env, campaign_id, donor);
        if contributed == 0 {
            return Err(Error::NothingToRefund);
        }

        if campaign.raised == 0 {
            return Err(Error::InvalidAmount);
        }

        let pool =
            campaign.raised - campaign.released + storage::slashed_recovery(env, campaign_id);
        let numerator = contributed.checked_mul(pool).ok_or(Error::InvalidAmount)?;
        let amount = numerator / campaign.raised;

        storage::set_contribution(env, campaign_id, donor, 0);
        Ok((campaign, amount))
    }

    /// Rolls `donor`'s refund from `from_campaign` into `to_campaign` in one call.
    ///
    /// The amount is computed exactly like [`refund`] and then donated to the
    /// target with the same limits as [`donate`], but the tokens never leave the
    /// contract. Requires auth from `donor`. Emits [`Refunded`] for the source and
    /// [`Donated`] for the target.
    ///
    /// Partial rollovers are not supported: if the refund is larger than the
    /// target's remaining goal the call fails with [`Error::GoalExceeded`] and the
    /// donor can claim a normal refund instead. A donor's contribution is zeroed
    /// by the rollover, so it cannot be rolled over twice or refunded afterwards.
    ///
    /// # Errors
    /// - [`Error::Paused`]: the contract is paused.
    /// - [`Error::SameCampaign`]: `from_campaign == to_campaign`.
    /// - [`Error::CampaignNotFound`]: either campaign does not exist.
    /// - [`Error::CampaignNotActive`] / [`Error::CampaignExpired`] /
    ///   [`Error::MilestoneOverdue`]: the target is not open.
    /// - [`Error::RefundNotAvailable`] / [`Error::NothingToRefund`]: as for [`refund`].
    /// - [`Error::TokenMismatch`]: the campaigns use different tokens.
    /// - [`Error::InvalidAmount`]: the refund amount is zero.
    /// - [`Error::GoalExceeded`]: the refund would pass the target's goal.
    /// - [`Error::DonationBelowMinimum`]: the refund is below the target's minimum donation.
    pub fn rollover_refund(
        env: Env,
        donor: Address,
        from_campaign: u64,
        to_campaign: u64,
    ) -> Result<i128, Error> {
        donor.require_auth();
        if storage::is_paused(&env) {
            return Err(Error::Paused);
        }
        if from_campaign == to_campaign {
            return Err(Error::SameCampaign);
        }

        let mut target = storage::campaign(&env, to_campaign)?;
        Self::ensure_open(&env, &target)?;

        let (source, amount) = Self::take_refund(&env, &donor, from_campaign)?;
        if source.token != target.token {
            return Err(Error::TokenMismatch);
        }
        if amount <= 0 {
            return Err(Error::InvalidAmount);
        }

        let remaining = target.goal - target.raised;
        if amount > remaining {
            return Err(Error::GoalExceeded);
        }
        if target.min_donation > 0 && amount < target.min_donation && amount < remaining {
            return Err(Error::DonationBelowMinimum);
        }

        target.raised += amount;
        storage::save_campaign(&env, &target);
        let previous = storage::contribution(&env, to_campaign, &donor);
        storage::set_contribution(&env, to_campaign, &donor, previous + amount);
        storage::add_campaign_donor(&env, to_campaign, &donor);

        Refunded {
            campaign_id: from_campaign,
            donor: donor.clone(),
            amount,
        }
        .publish(&env);
        Donated {
            campaign_id: to_campaign,
            donor,
            amount,
        }
        .publish(&env);
        Ok(amount)
    }

    // ─── Issue #4: Verifier reassignment ──────────────────────────────────────

    /// Reassigns an active campaign's verifier to `new_verifier`.
    ///
    /// Only the admin may call this. The campaign must be `Active` and not
    /// expired. `new_verifier` must be a currently registered verifier. This
    /// unblocks campaigns whose original verifier was removed. Emits
    /// [`VerifierReassigned`].
    ///
    /// # Errors
    /// - [`Error::Unauthorized`]: caller is not the admin.
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    /// - [`Error::CampaignNotActive`]: campaign is not in `Active` status.
    /// - [`Error::CampaignExpired`]: campaign deadline has passed.
    /// - [`Error::VerifierCannotBeBeneficiary`]: `new_verifier` is the campaign's beneficiary.
    /// - [`Error::NotVerifier`]: `new_verifier` is not currently registered.
    /// - [`Error::ConflictOfInterest`]: `new_verifier` is the campaign's creator.
    pub fn reassign_verifier(
        env: Env,
        campaign_id: u64,
        new_verifier: Address,
    ) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        Self::reassign_verifier_impl(env, campaign_id, new_verifier)
    }

    fn reassign_verifier_impl(
        env: Env,
        campaign_id: u64,
        new_verifier: Address,
    ) -> Result<(), Error> {
        let mut campaign = storage::campaign(&env, campaign_id)?;
        Self::ensure_open(&env, &campaign)?;

        if campaign.verifier_reassignments >= MAX_VERIFIER_REASSIGNMENTS {
            return Err(Error::MaxVerifierReassignmentsExceeded);
        }

        if new_verifier == campaign.beneficiary {
            return Err(Error::VerifierCannotBeBeneficiary);
        }

        if new_verifier == env.current_contract_address()
            || !storage::is_verifier(&env, &new_verifier)
        {
            return Err(Error::NotVerifier);
        }

        if new_verifier == campaign.creator {
            return Err(Error::ConflictOfInterest);
        }

        let old_verifier = campaign.verifier.clone();
        // The campaign moves from the old verifiers' bonds to the new one's.
        Self::close_verifier_assignments(&env, &campaign.verifiers);
        campaign.verifier = new_verifier.clone();
        let mut verifiers = Vec::new(&env);
        verifiers.push_back(new_verifier.clone());
        campaign.verifiers = verifiers;
        campaign.threshold = 1;
        campaign.verifier_reassignments += 1;
        storage::save_campaign(&env, &campaign);
        Self::open_verifier_assignments(&env, &campaign.verifiers);
        // Only the new verifier is assigned now; the old one keeps the record
        // of the campaign they held (Issue #119).
        Self::bump_verifier(&env, &new_verifier, |s| s.campaigns_assigned += 1);

        VerifierReassigned {
            campaign_id,
            old_verifier,
            new_verifier,
        }
        .publish(&env);

        Ok(())
    }

    // ─── Timelock (issue #113) ────────────────────────────────────────────────

    /// Queues a sensitive admin `action` and returns its id.
    ///
    /// Only the admin may call this, except that queueing a slash needs the
    /// slasher. The action can be executed by anyone once
    /// [`TIMELOCK_DELAY`] seconds have passed, and stays executable for
    /// [`TIMELOCK_GRACE_PERIOD`] seconds after that. Emits [`ActionQueued`].
    pub fn queue_action(env: Env, action: TimelockedAction) -> u64 {
        // Slashing belongs to the slasher (#111), so queueing a slash needs the
        // slasher too, or the timelock would let the admin bypass the role.
        match &action {
            TimelockedAction::SlashVerifier(..) => storage::slasher(&env).require_auth(),
            _ => storage::admin(&env).require_auth(),
        }
        let action_id = storage::next_action_id(&env);
        let eta = env.ledger().timestamp() + TIMELOCK_DELAY;
        storage::save_queued_action(&env, action_id, &QueuedAction { action, eta });
        ActionQueued { action_id, eta }.publish(&env);
        action_id
    }

    /// Cancels a queued action before it is executed.
    ///
    /// Only the admin may call this. Emits [`ActionCancelled`].
    ///
    /// # Errors
    /// - [`Error::ActionNotFound`]: no queued action with `action_id` exists.
    pub fn cancel_action(env: Env, action_id: u64) -> Result<(), Error> {
        storage::admin(&env).require_auth();
        storage::queued_action(&env, action_id).ok_or(Error::ActionNotFound)?;
        storage::remove_queued_action(&env, action_id);
        ActionCancelled { action_id }.publish(&env);
        Ok(())
    }

    /// Executes a queued action once its delay has elapsed.
    ///
    /// Callable by anyone. The inner action runs with the same checks as its
    /// direct admin entry point. Emits [`ActionExecuted`].
    ///
    /// # Errors
    /// - [`Error::ActionNotFound`]: no queued action with `action_id` exists.
    /// - [`Error::TimelockNotReady`]: the delay has not elapsed yet.
    /// - [`Error::ActionExpired`]: the grace period has passed.
    pub fn execute_action(env: Env, action_id: u64) -> Result<(), Error> {
        let queued = storage::queued_action(&env, action_id).ok_or(Error::ActionNotFound)?;
        let now = env.ledger().timestamp();
        if now < queued.eta {
            return Err(Error::TimelockNotReady);
        }
        if now > queued.eta + TIMELOCK_GRACE_PERIOD {
            return Err(Error::ActionExpired);
        }
        storage::remove_queued_action(&env, action_id);

        match queued.action {
            TimelockedAction::ReassignVerifier(campaign_id, new_verifier) => {
                Self::reassign_verifier_impl(env.clone(), campaign_id, new_verifier)?
            }
            TimelockedAction::SlashVerifier(verifier, campaign_id, slash_amount) => {
                Self::slash_verifier_impl(env.clone(), verifier, campaign_id, slash_amount)?
            }
            TimelockedAction::SetBondRequirement(amount) => {
                if amount < 0 {
                    return Err(Error::InvalidAmount);
                }
                storage::set_bond_requirement(&env, amount);
            }
            TimelockedAction::SetBondWithdrawDelay(delay) => {
                if delay > MAX_BOND_WITHDRAW_DELAY {
                    return Err(Error::InvalidAmount);
                }
                storage::set_bond_withdraw_delay(&env, delay);
            }
        }

        ActionExecuted { action_id }.publish(&env);
        Ok(())
    }

    /// Returns the queued action `action_id`, or `None` if it was never queued,
    /// cancelled or executed.
    pub fn queued_action(env: Env, action_id: u64) -> Option<QueuedAction> {
        storage::queued_action(&env, action_id)
    }

    // ─── Views ────────────────────────────────────────────────────────────────

    /// Returns `verifier`'s on-chain track record (Issue #119): campaigns
    /// assigned, milestone approvals recorded, campaigns completed and
    /// cancelled while assigned, and the total slashed from their bond. All
    /// zeros for an address that has done nothing. A reassigned campaign counts
    /// for whoever held it at the time of each event.
    pub fn verifier_stats(env: Env, verifier: Address) -> VerifierStats {
        storage::verifier_stats(&env, &verifier)
    }

    /// Returns the [`Campaign`] for `campaign_id`.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn get_campaign(env: Env, campaign_id: u64) -> Result<Campaign, Error> {
        storage::campaign(&env, campaign_id)
    }

    /// Returns the remaining amount needed to reach the campaign goal (`goal - raised`).
    ///
    /// Returns `0` if the campaign goal has already been reached or exceeded.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn remaining_to_goal(env: Env, campaign_id: u64) -> Result<i128, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        let remaining = campaign.goal.saturating_sub(campaign.raised);
        Ok(remaining.max(0))
    }

    /// Returns the next unreleased milestone index and its scheduled amount `(index, amount)`,
    /// or `None` if all milestones have been released.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn next_milestone(env: Env, campaign_id: u64) -> Result<Option<(u32, i128)>, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        let index = campaign.milestones_released;
        if let Some(amount) = campaign.milestones.get(index) {
            Ok(Some((index, amount)))
        } else {
            Ok(None)
        }
    }

    /// Extends the storage TTL of `campaign_id` so a quiet campaign does not
    /// expire between donations or approvals (Issue #5).
    ///
    /// Anyone may call this, with no authorization, so keeper bots, frontends
    /// or community members can keep long-running climate campaigns alive. It
    /// renews the contract instance entry (admin, token, counters), the
    /// campaign entry and the campaign-wide entries it has: milestone due
    /// dates (kept until past the deadline), split beneficiaries, stream
    /// schedule, donor and pledge lists, start time, slashed recovery and the
    /// cancel-vote total.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn bump_campaign_ttl(env: Env, campaign_id: u64) -> Result<(), Error> {
        storage::bump_campaign_ttl(&env, campaign_id)
    }

    /// Same as [`bump_campaign_ttl`]; kept so existing keepers and bindings
    /// keep working.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn extend_campaign_ttl(env: Env, campaign_id: u64) -> Result<(), Error> {
        storage::bump_campaign_ttl(&env, campaign_id)
    }

    /// Returns the total number of campaigns created (also the next campaign ID).
    pub fn campaign_count(env: Env) -> u64 {
        storage::campaign_count(&env)
    }

    /// Returns the effective status of `campaign_id`.
    ///
    /// If the campaign is stored as [`CampaignStatus::Active`] but the ledger timestamp
    /// has passed its deadline, returns [`CampaignStatus::Expired`]. Otherwise returns
    /// the stored status ([`CampaignStatus::Active`], [`CampaignStatus::Completed`], or
    /// [`CampaignStatus::Cancelled`]).
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn effective_status(env: Env, campaign_id: u64) -> Result<CampaignStatus, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        if Self::is_expired(&env, &campaign) {
            Ok(CampaignStatus::Expired)
        } else {
            Ok(campaign.status)
        }
    }

    /// Returns the number of times `campaign_id`'s verifier has been reassigned.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn verifier_reassignments(env: Env, campaign_id: u64) -> Result<u32, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        Ok(campaign.verifier_reassignments)
    }

    /// Returns the milestone due dates for `campaign_id`.
    ///
    /// Returns an empty vector for campaigns created without per-milestone due dates.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn milestone_due_dates(env: Env, campaign_id: u64) -> Result<Vec<u64>, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        Ok(
            storage::milestone_due_dates(&env, campaign_id, campaign.deadline)
                .unwrap_or_else(|| Vec::new(&env)),
        )
    }

    /// Returns the total amount `donor` has contributed to `campaign_id`.
    pub fn contribution_of(env: Env, campaign_id: u64, donor: Address) -> i128 {
        storage::contribution(&env, campaign_id, &donor)
    }

    /// Returns a paginated list of unique donor addresses for `campaign_id`.
    ///
    /// Addresses are returned in stable order (order of first donation). `limit` is clamped
    /// to at most [`MAX_DONORS_PAGE_LIMIT`]. Each donor appears exactly once even after repeat donations.
    pub fn donors(env: Env, campaign_id: u64, start: u32, limit: u32) -> Vec<Address> {
        let all_donors = storage::campaign_donors(&env, campaign_id);
        let total = all_donors.len();
        if start >= total || limit == 0 {
            return Vec::new(&env);
        }

        let effective_limit = limit.min(MAX_DONORS_PAGE_LIMIT);
        let end = (start + effective_limit).min(total);

        let mut page = Vec::new(&env);
        let mut i = start;
        while i < end {
            if let Some(donor) = all_donors.get(i) {
                page.push_back(donor);
            }
            i += 1;
        }
        page
    }

    /// Returns `true` if donors can claim refunds for `campaign_id`.
    ///
    /// Mirrors the availability checks in [`refund`]: a campaign is refundable
    /// if it has been cancelled, if it is active and past its deadline, or if
    /// a milestone is overdue. Returns `false` if the campaign is active and on
    /// schedule, completed, or does not exist.
    pub fn is_refundable(env: Env, campaign_id: u64) -> bool {
        let Ok(campaign) = storage::campaign(&env, campaign_id) else {
            return false;
        };
        Self::is_campaign_refundable(&env, &campaign)
    }

    /// Returns the amount `donor` would receive if they claimed a refund right now.
    ///
    /// Uses the same rounding rules as [`refund`] (rounds down in favor of the contract).
    /// Returns 0 if a refund is not available, if the campaign does not exist, or if
    /// `donor` has no contribution or no unreleased balance remains.
    pub fn refundable_amount(env: Env, campaign_id: u64, donor: Address) -> i128 {
        let Ok(campaign) = storage::campaign(&env, campaign_id) else {
            return 0;
        };
        if !Self::is_campaign_refundable(&env, &campaign) {
            return 0;
        }
        let contributed = storage::contribution(&env, campaign_id, &donor);
        if contributed == 0 || campaign.raised == 0 {
            return 0;
        }
        let pool =
            campaign.raised - campaign.released + storage::slashed_recovery(&env, campaign_id);
        if pool <= 0 {
            return 0;
        }
        let Some(numerator) = contributed.checked_mul(pool) else {
            return 0;
        };
        numerator / campaign.raised
    }

    /// Returns the slashed verifier bond amount held for `campaign_id`'s donors.
    pub fn slashed_recovery(env: Env, campaign_id: u64) -> i128 {
        storage::slashed_recovery(&env, campaign_id)
    }

    /// Returns `true` if `who` is currently a registered verifier.
    pub fn is_verifier(env: Env, who: Address) -> bool {
        storage::is_verifier(&env, &who)
    }

    /// Returns the off-chain profile URI for `verifier`, or `None` if unset.
    pub fn verifier_profile(env: Env, verifier: Address) -> Option<String> {
        storage::verifier_profile(&env, &verifier)
    }

    /// Returns the current admin address.
    pub fn admin(env: Env) -> Address {
        storage::admin(&env)
    }

    /// Returns the pending admin address nominated via [`propose_admin`], or `None`.
    pub fn pending_admin(env: Env) -> Option<Address> {
        storage::pending_admin(&env)
    }

    /// Returns the token contract address used by all campaigns.
    pub fn token(env: Env) -> Address {
        storage::token(&env)
    }

    /// Returns the crate version of the deployed contract.
    pub fn version(env: Env) -> String {
        String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    /// Returns the [`SponsorPool`] for `sponsor` on `campaign_id`, or `None`.
    pub fn get_sponsor_pool(env: Env, campaign_id: u64, sponsor: Address) -> Option<SponsorPool> {
        storage::sponsor_pool(&env, campaign_id, &sponsor)
    }

    /// Returns the [`Pledge`] with the given `pledge_id`, or `None`.
    pub fn get_pledge(env: Env, pledge_id: u64) -> Option<Pledge> {
        storage::pledge(&env, pledge_id)
    }

    /// Returns the list of verifiers who have approved milestone `milestone_index`.
    pub fn get_milestone_approvals(
        env: Env,
        campaign_id: u64,
        milestone_index: u32,
    ) -> Vec<Address> {
        storage::milestone_approvals(&env, campaign_id, milestone_index)
    }

    /// Returns the list of proof URIs submitted by verifiers who approved milestone `milestone_index`.
    pub fn get_milestone_approval_proofs(
        env: Env,
        campaign_id: u64,
        milestone_index: u32,
    ) -> Vec<String> {
        storage::milestone_approval_proofs(&env, campaign_id, milestone_index)
    }

    /// Returns the configured approval cooldown in seconds for `campaign_id`.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn approval_cooldown(env: Env, campaign_id: u64) -> Result<u64, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        Ok(campaign.approval_cooldown)
    }

    /// Returns the ledger timestamp when the last milestone was approved/released,
    /// or `0` if no milestone has been approved yet.
    ///
    /// # Errors
    /// - [`Error::CampaignNotFound`]: no campaign with `campaign_id` exists.
    pub fn last_approval_time(env: Env, campaign_id: u64) -> Result<u64, Error> {
        let campaign = storage::campaign(&env, campaign_id)?;
        Ok(campaign.last_approval_time)
    }

    /// Returns the current platform fee in basis points (default: 0).
    pub fn platform_fee(env: Env) -> u32 {
        storage::platform_fee(&env)
    }

    /// Returns the current platform treasury address, or `None` if not configured.
    pub fn treasury(env: Env) -> Option<Address> {
        storage::treasury(&env)
    }

    /// Returns `true` if the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        storage::is_paused(&env)
    }

    // ─── Internal ─────────────────────────────────────────────────────────────

    fn require_role(env: &Env, caller: &Address, role: Role) -> Result<(), Error> {
        if *caller == storage::admin(env)
            || storage::has_role(env, Role::Admin, caller)
            || storage::has_role(env, role, caller)
        {
            Ok(())
        } else {
            Err(Error::Unauthorized)
        }
    }

    /// Transitions an active campaign to `Cancelled`, settles running streams,
    /// releases verifier active-campaign locks, and updates verifier cancellation stats.
    fn cancel_campaign_state(env: &Env, campaign: &mut Campaign, emit_cancelled: bool) {
        campaign.status = CampaignStatus::Cancelled;
        if campaign.raised == 0 {
            storage::cleanup_unfunded_campaign(env, campaign.id);
        } else {
            Self::settle_streams(env, campaign);
        }
        storage::save_campaign(env, campaign);
        Self::close_verifier_assignments(env, &campaign.verifiers);
        Self::bump_all_verifiers(env, &campaign.verifiers, |s| s.campaigns_cancelled += 1);
        if emit_cancelled {
            CampaignCancelled {
                campaign_id: campaign.id,
            }
            .publish(env);
        }
    }

    fn ensure_open(env: &Env, campaign: &Campaign) -> Result<(), Error> {
        if campaign.status != CampaignStatus::Active {
            return Err(Error::CampaignNotActive);
        }
        if Self::is_expired(env, campaign) {
            return Err(Error::CampaignExpired);
        }
        if Self::is_overdue(env, campaign) {
            return Err(Error::MilestoneOverdue);
        }
        Ok(())
    }

    /// Applies `update` to one verifier's track record (Issue #119).
    fn bump_verifier(env: &Env, who: &Address, update: impl FnOnce(&mut VerifierStats)) {
        let mut stats = storage::verifier_stats(env, who);
        update(&mut stats);
        storage::save_verifier_stats(env, who, &stats);
    }

    /// Applies `update` to the track record of every verifier on a campaign.
    fn bump_all_verifiers(env: &Env, who: &Vec<Address>, update: impl Fn(&mut VerifierStats)) {
        for verifier in who.iter() {
            Self::bump_verifier(env, &verifier, &update);
        }
    }

    /// Fails while stored data is older than this build expects (Issue #118).
    fn ensure_current_schema(env: &Env) -> Result<(), Error> {
        if storage::schema_version(env) < SCHEMA_VERSION {
            return Err(Error::MigrationPending);
        }
        Ok(())
    }

    /// Pays `amount` out of escrow to the campaign's beneficiary, or splits it
    /// across its beneficiaries (Issue #116). Every share is rounded down and
    /// the leftover goes to the first beneficiary, so the payments always sum to
    /// exactly `amount`.
    fn pay_beneficiaries(env: &Env, campaign: &Campaign, amount: i128) {
        let tok = token::Client::new(env, &campaign.token);
        let from = env.current_contract_address();
        let Some(list) = storage::beneficiaries(env, campaign.id) else {
            tok.transfer(&from, &campaign.beneficiary, &amount);
            return;
        };

        let mut shares: Vec<i128> = Vec::new(env);
        let mut paid: i128 = 0;
        for b in list.iter() {
            let share = amount * (b.bps as i128) / MAX_RATIO_BPS as i128;
            paid += share;
            shares.push_back(share);
        }
        let dust = amount - paid;
        for (i, b) in list.iter().enumerate() {
            let mut share = shares.get(i as u32).unwrap();
            if i == 0 {
                share += dust;
            }
            if share > 0 {
                tok.transfer(&from, &b.to, &share);
                BeneficiaryPaid {
                    campaign_id: campaign.id,
                    recipient: b.to,
                    amount: share,
                }
                .publish(env);
            }
        }
    }

    /// Cancels `campaign` if its cancel votes have reached the threshold after
    /// the stall period (Issue #114).
    fn cancel_by_vote(env: &Env, campaign: &mut Campaign) -> Result<(), Error> {
        let base = campaign
            .last_approval_time
            .max(storage::campaign_start(env, campaign.id));
        if base == 0 || env.ledger().timestamp() < base.saturating_add(STALL_PERIOD) {
            return Err(Error::NotStalled);
        }
        let votes = storage::cancel_vote_total(env, campaign.id);
        if votes <= 0 || votes * 10_000 < campaign.raised * CANCEL_VOTE_BPS {
            return Err(Error::VoteThresholdNotReached);
        }

        campaign.status = CampaignStatus::Cancelled;
        Self::settle_streams(env, campaign);
        storage::save_campaign(env, campaign);
        Self::close_verifier_assignments(env, &campaign.verifiers);
        CampaignCancelled {
            campaign_id: campaign.id,
        }
        .publish(env);
        Ok(())
    }

    /// How much of `stream` has vested at the current ledger time.
    fn vested(env: &Env, stream: &Stream) -> i128 {
        if stream.duration == 0 {
            return stream.total;
        }
        let elapsed = env.ledger().timestamp().saturating_sub(stream.start);
        if elapsed >= stream.duration {
            return stream.total;
        }
        stream
            .total
            .checked_mul(elapsed as i128)
            .map(|scaled| scaled / stream.duration as i128)
            .unwrap_or(stream.total)
    }

    /// Ends every running stream at its vested amount (Issue #117): the vested
    /// part stays claimable by the beneficiary and the unvested part leaves
    /// `released`, so it returns to the donors' refund pool. Returns whether
    /// anything changed. Settled streams have `duration == 0` and are skipped.
    fn settle_streams(env: &Env, campaign: &mut Campaign) -> bool {
        let mut changed = false;
        for index in 0..campaign.milestones_released {
            if let Some(mut stream) = storage::stream(env, campaign.id, index)
                && stream.duration > 0
            {
                let vested = Self::vested(env, &stream);
                campaign.released -= stream.total - vested;
                stream.total = vested;
                stream.duration = 0;
                storage::save_stream(env, campaign.id, index, &stream);
                changed = true;
            }
        }
        changed
    }

    /// Counts a new campaign against each of its verifiers (Issue #129).
    fn open_verifier_assignments(env: &Env, verifiers: &Vec<Address>) {
        for verifier in verifiers.iter() {
            let count = storage::active_campaigns(env, &verifier);
            storage::set_active_campaigns(env, &verifier, count + 1);
        }
    }

    /// Releases a campaign from each of its verifiers once it is completed or
    /// cancelled, or moved to another verifier (Issue #129).
    fn close_verifier_assignments(env: &Env, verifiers: &Vec<Address>) {
        for verifier in verifiers.iter() {
            let count = storage::active_campaigns(env, &verifier);
            storage::set_active_campaigns(env, &verifier, count.saturating_sub(1));
        }
    }

    /// Pull pledge funds proportionally toward `milestone_amount`.
    ///
    /// Returns the total amount actually pulled from pledges.
    ///
    /// Algorithm:
    /// 1. Collect all active pledges for the campaign.
    /// 2. Compute total remaining pledge capacity.
    /// 3. For each pledge compute its proportional share of `milestone_amount`.
    /// 4. Clamp to: min(share, pledge.remaining, allowance).
    /// 5. If pull > 0, transfer and record contribution.
    /// 6. If allowance == 0 or pull == 0, emit PledgeSkipped.
    ///
    /// Rounding: each share is `milestone_amount * pledge.remaining / total_remaining`,
    /// using integer division truncated toward zero. Any unallocated remainder
    /// stays in the milestone's direct-donation funding.
    fn pull_pledges(
        env: &Env,
        campaign_id: u64,
        milestone_amount: i128,
        campaign: &mut Campaign,
    ) -> i128 {
        let tok = token::Client::new(env, &campaign.token);
        let contract_addr = env.current_contract_address();

        // Phase 1: gather active pledges for this campaign.
        let mut active_pledges: Vec<Pledge> = Vec::new(env);
        let mut total_remaining: i128 = 0;
        // Only this campaign's own pledge ids are read (#125). Pledges that are
        // cancelled or fully pulled are dropped from the list so it stays short.
        let pledge_ids = storage::campaign_pledges(env, campaign_id);
        let mut still_open: Vec<u64> = Vec::new(env);
        for pledge_id in pledge_ids.iter() {
            if let Some(p) = storage::pledge(env, pledge_id)
                && p.active
                && p.pulled_amount < p.pledged_amount
            {
                total_remaining += p.pledged_amount - p.pulled_amount;
                still_open.push_back(pledge_id);
                active_pledges.push_back(p);
            }
        }
        if still_open.len() != pledge_ids.len() {
            storage::save_campaign_pledges(env, campaign_id, &still_open);
        }
        if total_remaining == 0 || active_pledges.is_empty() {
            return 0;
        }

        // Phase 2: pull proportionally.
        let mut total_pulled: i128 = 0;

        for mut pledge in active_pledges.iter() {
            // Pulled pledges count towards `raised`, which must never pass the goal.
            let headroom = campaign.goal.saturating_sub(campaign.raised);
            if headroom <= 0 {
                break;
            }

            let pledge_remaining = pledge.pledged_amount - pledge.pulled_amount;

            // Proportional share: milestone_amount * pledge_remaining / total_remaining.
            let numerator = match milestone_amount.checked_mul(pledge_remaining) {
                Some(n) => n,
                None => {
                    PledgeSkipped {
                        pledge_id: pledge.pledge_id,
                        campaign_id,
                        donor: pledge.donor.clone(),
                        reason: SKIP_REASON_OVERFLOW,
                    }
                    .publish(env);
                    continue;
                }
            };
            let share = numerator / total_remaining;

            if share <= 0 {
                PledgeSkipped {
                    pledge_id: pledge.pledge_id,
                    campaign_id,
                    donor: pledge.donor.clone(),
                    reason: SKIP_REASON_EXHAUSTED,
                }
                .publish(env);
                continue;
            }

            // Check allowance.
            let allowance = tok.allowance(&pledge.donor, &contract_addr);
            if allowance <= 0 {
                PledgeSkipped {
                    pledge_id: pledge.pledge_id,
                    campaign_id,
                    donor: pledge.donor.clone(),
                    reason: SKIP_REASON_NO_ALLOWANCE,
                }
                .publish(env);
                continue;
            }

            // The allowance only authorises spending; the wallet may be empty.
            let balance = tok.balance(&pledge.donor);
            if balance <= 0 {
                PledgeSkipped {
                    pledge_id: pledge.pledge_id,
                    campaign_id,
                    donor: pledge.donor.clone(),
                    reason: SKIP_REASON_INSUFFICIENT_BALANCE,
                }
                .publish(env);
                continue;
            }

            // Clamp to what is actually available.
            let pull = share
                .min(pledge_remaining)
                .min(allowance)
                .min(balance)
                .min(headroom);
            if pull <= 0 {
                PledgeSkipped {
                    pledge_id: pledge.pledge_id,
                    campaign_id,
                    donor: pledge.donor.clone(),
                    reason: SKIP_REASON_NO_ALLOWANCE,
                }
                .publish(env);
                continue;
            }

            // Transfer tokens from donor to contract.
            tok.transfer_from(&contract_addr, &pledge.donor, &contract_addr, &pull);

            // Update pledge record.
            pledge.pulled_amount += pull;
            if pledge.pulled_amount >= pledge.pledged_amount {
                pledge.active = false;
            }
            storage::save_pledge(env, &pledge);

            // Record as contribution so donor shares in refunds.
            let prev = storage::contribution(env, campaign_id, &pledge.donor);
            storage::set_contribution(env, campaign_id, &pledge.donor, prev + pull);
            storage::add_campaign_donor(env, campaign_id, &pledge.donor);

            // Increase campaign.raised.
            campaign.raised = campaign.raised.checked_add(pull).unwrap_or(campaign.raised);
            total_pulled += pull;

            PledgePulled {
                pledge_id: pledge.pledge_id,
                campaign_id,
                donor: pledge.donor.clone(),
                pulled_amount: pull,
            }
            .publish(env);
        }

        total_pulled
    }

    fn is_overdue(env: &Env, campaign: &Campaign) -> bool {
        if campaign.status != CampaignStatus::Active {
            return false;
        }
        if let Some(due_dates) = storage::milestone_due_dates(env, campaign.id, campaign.deadline)
            && let Some(due_date) = due_dates.get(campaign.milestones_released)
            && env.ledger().timestamp() > due_date
        {
            return true;
        }
        false
    }

    fn is_expired(env: &Env, campaign: &Campaign) -> bool {
        campaign.status == CampaignStatus::Active && env.ledger().timestamp() > campaign.deadline
    }

    fn is_campaign_refundable(env: &Env, campaign: &Campaign) -> bool {
        let ended = campaign.status == CampaignStatus::Cancelled || Self::is_expired(env, campaign);
        ended || Self::is_overdue(env, campaign)
    }
}
