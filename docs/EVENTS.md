# Contract Events Reference

This document provides a comprehensive reference for all contract events emitted by Aidline (`contracts/aidline/src/events.rs`).

The backend indexer and off-chain analytics listen to these events to reconstruct campaign progress, donor histories, verifier reputations, and escrow disbursements without querying on-chain storage.

---

## Event Summary

| Event Struct | Topic Symbol | Topics | Data Payload | Emitted By |
|---|---|---|---|---|
| `CampaignCreated` | `created` | `(symbol, campaign_id)` | `(creator, kind, goal, deadline, min_donation)` | `create_campaign`, `create_campaign_with_due_dates`, `create_multisig_campaign`, `create_campaign_min_donation` |
| `Donated` | `donated` | `(symbol, campaign_id, donor)` | `amount` | `donate` |
| `MilestoneReleased` | `released` | `(symbol, campaign_id, index)` | `(amount, fee, proof_uri)` | `approve_milestone`, `approve_milestone_multisig` |
| `MilestoneRejected` | `rejected` | `(symbol, campaign_id, index)` | `(reason, details_uri)` | `reject_milestone` |
| `CampaignCancelled` | `canceled` | `(symbol, campaign_id)` | `()` (empty) | `cancel_campaign` |
| `Refunded` | `refunded` | `(symbol, campaign_id, donor)` | `amount` | `refund` |
| `VerifierUpdated` | `verifier` | `(symbol, verifier)` | `active` | `add_verifier`, `remove_verifier` |
| `AdminChanged` | `adm_chng` | `(symbol, old)` | `new` | `set_admin` |
| `VerifierReassigned` | `vf_reass` | `(symbol, campaign_id)` | `(old_verifier, new_verifier)` | `reassign_verifier` |
| `SponsorPoolDeposited` | `sp_depos` | `(symbol, campaign_id, sponsor)` | `(amount, ratio_bps, cap)` | `deposit_sponsor_pool` |
| `MatchingApplied` | `sp_match` | `(symbol, campaign_id, sponsor)` | `(donor, donation_amount, matched_amount)` | `apply_matching` |
| `SponsorPoolReturned` | `sp_ret` | `(symbol, campaign_id, sponsor)` | `amount` | `return_sponsor_pool` |
| `VerifierBondPosted` | `vb_post` | `(symbol, verifier)` | `amount` | `register_with_bond` |
| `VerifierDeregistered` | `vb_dereg` | `(symbol, verifier)` | `deregistered_at` | `deregister_verifier` |
| `BondWithdrawn` | `vb_withd` | `(symbol, verifier)` | `amount` | `withdraw_bond` |
| `BondSlashed` | `vb_slash` | `(symbol, verifier, campaign_id)` | `slashed_amount` | `slash_bond` |
| `PledgeCreated` | `pl_creat` | `(symbol, campaign_id, donor)` | `(pledge_id, pledged_amount)` | `create_pledge` |
| `PledgePulled` | `pl_pull` | `(symbol, campaign_id, donor)` | `(pledge_id, pulled_amount)` | `approve_milestone`, `approve_milestone_multisig` |
| `PledgeSkipped` | `pl_skip` | `(symbol, campaign_id, donor)` | `(pledge_id, reason)` | `approve_milestone`, `approve_milestone_multisig` |
| `PledgeSettlement` | `pl_settl` | `(symbol, campaign_id, milestone_index)` | `total_pledged_pulled` | `approve_milestone`, `approve_milestone_multisig` |
| `EmergencyAdvanceReleased` | `em_adv` | `(symbol, campaign_id, verifier)` | `(milestone_index, amount)` | `emergency_fast_track` |
| `MilestoneApproved` | `approved` | `(symbol, campaign_id, milestone_index)` | `(verifier, approvals_count, threshold)` | `approve_milestone`, `approve_milestone_multisig` |
| `TokenAllowlistUpdated` | `tok_allow` | `(symbol, token)` | `allowed` | `add_allowed_token`, `remove_allowed_token` |
| `PlatformFeeUpdated` | `fee_upd` | `(symbol,)` | `fee_bps` | `set_platform_fee` |
| `TreasuryUpdated` | `tres_upd` | `(symbol,)` | `treasury` | `set_treasury` |
| `ContractPaused` | `paused` | `(symbol, admin)` | `()` (empty) | `pause` |
| `ContractUnpaused` | `unpaused` | `(symbol, admin)` | `()` (empty) | `unpause` |
| `ContractUpgraded` | `upgraded` | `(symbol,)` | `new_wasm_hash` | `upgrade` |
| `MetadataUpdated` | `meta_upd` | `(symbol, campaign_id)` | `new_metadata_uri` | `update_metadata` |
| `DeadlineExtended` | `dl_ext` | `(symbol, campaign_id)` | `(old_deadline, new_deadline)` | `extend_deadline`, `extend_deadline_by` |
| `RoleGranted` | `role_grnt` | `(symbol, role)` | `account` | `grant_role` |
| `RoleRevoked` | `role_rvk` | `(symbol, role)` | `account` | `revoke_role` |

---

## Event Details

### 1. `CampaignCreated`
Emitted when a new campaign is successfully opened.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `creator: Address`
  3. `kind: CampaignKind`
  4. `goal: i128`
  5. `deadline: u64`
  6. `min_donation: i128`
- **Topics**: `(symbol_short!("created"), campaign_id: u64)`
- **Data**: `(creator: Address, kind: CampaignKind, goal: i128, deadline: u64, min_donation: i128)`
- **Emitted by**: `create_campaign`, `create_campaign_with_due_dates`, `create_multisig_campaign`, `create_campaign_min_donation`

### 2. `Donated`
Emitted when a donor deposits tokens into campaign escrow.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `donor: Address`
  3. `amount: i128`
- **Topics**: `(symbol_short!("donated"), campaign_id: u64, donor: Address)`
- **Data**: `amount: i128`
- **Emitted by**: `donate`

### 3. `MilestoneReleased`
Emitted when a milestone reaches its required verification threshold and funds are transferred to the beneficiary and treasury.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `index: u32`
  3. `amount: i128`
  4. `fee: i128`
  5. `proof_uri: String`
- **Topics**: `(symbol_short!("released"), campaign_id: u64, index: u32)`
- **Data**: `(amount: i128, fee: i128, proof_uri: String)`
- **Emitted by**: `approve_milestone`, `approve_milestone_multisig`

### 4. `MilestoneRejected`
Emitted when a verifier rejects a pending milestone, providing a structured reason code and details URI for off-chain auditing.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `index: u32`
  3. `reason: u32`
  4. `details_uri: String`
- **Topics**: `(symbol_short!("rejected"), campaign_id: u64, index: u32)`
- **Data**: `(reason: u32, details_uri: String)`
- **Emitted by**: `reject_milestone`

### 5. `CampaignCancelled`
Emitted when a campaign is stopped early by the creator or admin, opening pro-rata refunds.
- **Fields in code**:
  1. `campaign_id: u64`
- **Topics**: `(symbol_short!("canceled"), campaign_id: u64)`
- **Data**: `()`
- **Emitted by**: `cancel_campaign`

### 6. `Refunded`
Emitted when a donor reclaims their pro-rata unreleased contribution following campaign cancellation or expiration.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `donor: Address`
  3. `amount: i128`
- **Topics**: `(symbol_short!("refunded"), campaign_id: u64, donor: Address)`
- **Data**: `amount: i128`
- **Emitted by**: `refund`

### 7. `VerifierUpdated`
Emitted when the admin registers or deactivates a verifier.
- **Fields in code**:
  1. `verifier: Address`
  2. `active: bool`
- **Topics**: `(symbol_short!("verifier"), verifier: Address)`
- **Data**: `active: bool`
- **Emitted by**: `add_verifier`, `remove_verifier`

### 8. `AdminChanged`
Emitted when the contract admin transfers administrative control to a new address.
- **Fields in code**:
  1. `old: Address`
  2. `new: Address`
- **Topics**: `(symbol_short!("adm_chng"), old: Address)`
- **Data**: `new: Address`
- **Emitted by**: `set_admin`

### 9. `VerifierReassigned`
Emitted when the admin reassigns an active campaign to a new verifier.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `old_verifier: Address`
  3. `new_verifier: Address`
- **Topics**: `(symbol_short!("vf_reass"), campaign_id: u64)`
- **Data**: `(old_verifier: Address, new_verifier: Address)`
- **Emitted by**: `reassign_verifier`

### 10. `SponsorPoolDeposited`
Emitted when a sponsor deposits and locks matching funds for a campaign.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `sponsor: Address`
  3. `amount: i128`
  4. `ratio_bps: u32`
  5. `cap: i128`
- **Topics**: `(symbol_short!("sp_depos"), campaign_id: u64, sponsor: Address)`
- **Data**: `(amount: i128, ratio_bps: u32, cap: i128)`
- **Emitted by**: `deposit_sponsor_pool`

### 11. `MatchingApplied`
Emitted when a donation triggers a matching allocation from a sponsor pool.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `sponsor: Address`
  3. `donor: Address`
  4. `donation_amount: i128`
  5. `matched_amount: i128`
- **Topics**: `(symbol_short!("sp_match"), campaign_id: u64, sponsor: Address)`
- **Data**: `(donor: Address, donation_amount: i128, matched_amount: i128)`
- **Emitted by**: `apply_matching`

### 12. `SponsorPoolReturned`
Emitted when unmatched sponsor pool funds are returned to the sponsor after campaign closure.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `sponsor: Address`
  3. `amount: i128`
- **Topics**: `(symbol_short!("sp_ret"), campaign_id: u64, sponsor: Address)`
- **Data**: `amount: i128`
- **Emitted by**: `return_sponsor_pool`

### 13. `VerifierBondPosted`
Emitted when a verifier deposits their required stake bond into escrow.
- **Fields in code**:
  1. `verifier: Address`
  2. `amount: i128`
- **Topics**: `(symbol_short!("vb_post"), verifier: Address)`
- **Data**: `amount: i128`
- **Emitted by**: `register_with_bond`

### 14. `VerifierDeregistered`
Emitted when a verifier initiates deregistration, starting the withdrawal delay period.
- **Fields in code**:
  1. `verifier: Address`
  2. `deregistered_at: u64`
- **Topics**: `(symbol_short!("vb_dereg"), verifier: Address)`
- **Data**: `deregistered_at: u64`
- **Emitted by**: `deregister_verifier`

### 15. `BondWithdrawn`
Emitted when a deregistered verifier withdraws their posted bond after the delay has passed.
- **Fields in code**:
  1. `verifier: Address`
  2. `amount: i128`
- **Topics**: `(symbol_short!("vb_withd"), verifier: Address)`
- **Data**: `amount: i128`
- **Emitted by**: `withdraw_bond`

### 16. `BondSlashed`
Emitted when the admin slashes a fraudulent verifier's bond and deposits the slashed tokens into the affected campaign escrow.
- **Fields in code**:
  1. `verifier: Address`
  2. `campaign_id: u64`
  3. `slashed_amount: i128`
- **Topics**: `(symbol_short!("vb_slash"), verifier: Address, campaign_id: u64)`
- **Data**: `slashed_amount: i128`
- **Emitted by**: `slash_bond`

### 17. `PledgeCreated`
Emitted when a donor registers an allowance-backed pledge for a campaign.
- **Fields in code**:
  1. `pledge_id: u64`
  2. `campaign_id: u64`
  3. `donor: Address`
  4. `pledged_amount: i128`
- **Topics**: `(symbol_short!("pl_creat"), campaign_id: u64, donor: Address)`
- **Data**: `(pledge_id: u64, pledged_amount: i128)`
- **Emitted by**: `create_pledge`

### 18. `PledgePulled`
Emitted when tokens are transferred from a pledgor's allowance to cover a released milestone.
- **Fields in code**:
  1. `pledge_id: u64`
  2. `campaign_id: u64`
  3. `donor: Address`
  4. `pulled_amount: i128`
- **Topics**: `(symbol_short!("pl_pull"), campaign_id: u64, donor: Address)`
- **Data**: `(pledge_id: u64, pulled_amount: i128)`
- **Emitted by**: `approve_milestone`, `approve_milestone_multisig` (during pledge settlement)

### 19. `PledgeSkipped`
Emitted when a pledge cannot be pulled during milestone release due to insufficient allowance or depletion.
- **Fields in code**:
  1. `pledge_id: u64`
  2. `campaign_id: u64`
  3. `donor: Address`
  4. `reason: u32` (`1` = No Allowance, `2` = Exhausted)
- **Topics**: `(symbol_short!("pl_skip"), campaign_id: u64, donor: Address)`
- **Data**: `(pledge_id: u64, reason: u32)`
- **Emitted by**: `approve_milestone`, `approve_milestone_multisig` (during pledge settlement)

### 20. `PledgeSettlement`
Emitted at the conclusion of milestone release summarizing the total tokens pulled from all pledges.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `milestone_index: u32`
  3. `total_pledged_pulled: i128`
- **Topics**: `(symbol_short!("pl_settl"), campaign_id: u64, milestone_index: u32)`
- **Data**: `total_pledged_pulled: i128`
- **Emitted by**: `approve_milestone`, `approve_milestone_multisig`

### 21. `EmergencyAdvanceReleased`
Emitted when a verifier authorizes an immediate advance payout (up to 20% of goal) for an Emergency campaign.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `verifier: Address`
  3. `milestone_index: u32`
  4. `amount: i128`
- **Topics**: `(symbol_short!("em_adv"), campaign_id: u64, verifier: Address)`
- **Data**: `(milestone_index: u32, amount: i128)`
- **Emitted by**: `emergency_fast_track`

### 22. `MilestoneApproved`
Emitted when an authorized verifier logs an approval for a milestone under single or multisig verification.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `milestone_index: u32`
  3. `verifier: Address`
  4. `approvals_count: u32`
  5. `threshold: u32`
- **Topics**: `(symbol_short!("approved"), campaign_id: u64, milestone_index: u32)`
- **Data**: `(verifier: Address, approvals_count: u32, threshold: u32)`
- **Emitted by**: `approve_milestone`, `approve_milestone_multisig`

### 23. `TokenAllowlistUpdated`
Emitted when the admin adds or removes an accepted campaign token.
- **Fields in code**:
  1. `token: Address`
  2. `allowed: bool`
- **Topics**: `(symbol_short!("tok_allow"), token: Address)`
- **Data**: `allowed: bool`
- **Emitted by**: `add_allowed_token`, `remove_allowed_token`

### 24. `PlatformFeeUpdated`
Emitted when the admin configures or updates the platform fee rate.
- **Fields in code**:
  1. `fee_bps: u32`
- **Topics**: `(symbol_short!("fee_upd"),)`
- **Data**: `fee_bps: u32`
- **Emitted by**: `set_platform_fee`

### 25. `TreasuryUpdated`
Emitted when the admin sets or updates the platform fee treasury address.
- **Fields in code**:
  1. `treasury: Address`
- **Topics**: `(symbol_short!("tres_upd"),)`
- **Data**: `treasury: Address`
- **Emitted by**: `set_treasury`

### 26. `ContractPaused`
Emitted when the admin activates the emergency pause switch.
- **Fields in code**:
  1. `admin: Address`
- **Topics**: `(symbol_short!("paused"), admin: Address)`
- **Data**: `()`
- **Emitted by**: `pause`

### 27. `ContractUnpaused`
Emitted when the admin deactivates the emergency pause switch, resuming mutations.
- **Fields in code**:
  1. `admin: Address`
- **Topics**: `(symbol_short!("unpaused"), admin: Address)`
- **Data**: `()`
- **Emitted by**: `unpause`

### 28. `ContractUpgraded`
Emitted when the admin upgrades the contract's Wasm executable bytecode.
- **Fields in code**:
  1. `new_wasm_hash: BytesN<32>`
- **Topics**: `(symbol_short!("upgraded"),)`
- **Data**: `new_wasm_hash: BytesN<32>`
- **Emitted by**: `upgrade`

### 29. `MetadataUpdated`
Emitted when the campaign creator updates the metadata URI before donations begin.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `new_metadata_uri: String`
- **Topics**: `(symbol_short!("meta_upd"), campaign_id: u64)`
- **Data**: `new_metadata_uri: String`
- **Emitted by**: `update_metadata`

### 30. `DeadlineExtended`
Emitted when the campaign creator grants a one-time deadline extension.
- **Fields in code**:
  1. `campaign_id: u64`
  2. `old_deadline: u64`
  3. `new_deadline: u64`
- **Topics**: `(symbol_short!("dl_ext"), campaign_id: u64)`
- **Data**: `(old_deadline: u64, new_deadline: u64)`
- **Emitted by**: `extend_deadline`, `extend_deadline_by`

### 31. `ActionQueued`
Emitted when the admin queues a timelocked action.
- **Fields in code**: `action_id: u64`, `eta: u64`
- **Topics**: `(symbol_short!("tl_queue"), action_id: u64)`
- **Data**: `eta: u64`
- **Emitted by**: `queue_action`

### 32. `ActionCancelled`
Emitted when the admin cancels a queued action.
- **Fields in code**: `action_id: u64`
- **Topics**: `(symbol_short!("tl_cancel"), action_id: u64)`
- **Data**: `()`
- **Emitted by**: `cancel_action`

### 33. `ActionExecuted`
Emitted when a queued action is executed after its delay.
- **Fields in code**: `action_id: u64`
- **Topics**: `(symbol_short!("tl_exec"), action_id: u64)`
- **Data**: `()`
- **Emitted by**: `execute_action`
### 34. `BeneficiaryPaid`
Emitted once per recipient when a split-payout campaign pays out a milestone or the emergency advance (#116).
- **Fields in code**:
  1. `campaign_id: u64`
  2. `recipient: Address`
  3. `amount: i128`
- **Topics**: `(symbol_short!("paid"), campaign_id: u64, recipient: Address)`
- **Data**: `amount: i128`
- **Emitted by**: `approve_milestone`, `approve_milestone_multisig`, `emergency_fast_track` (split-payout campaigns only)

