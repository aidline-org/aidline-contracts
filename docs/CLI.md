# Stellar CLI Guide

This guide provides copy-and-paste `stellar contract invoke` examples for every public function in the Aidline smart contract against the testnet deployment.

## Prerequisites and Setup

Before interacting with the contract:

1. Install the [Stellar CLI](https://developers.stellar.org/docs/tools/cli/install-cli) (v25.2+).
2. Configure environment variables for the testnet deployment:

```sh
export AIDLINE_CONTRACT_ID="CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK"
export NATIVE_XLM_ID="CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"
export NETWORK="testnet"
```

3. Create and fund testnet accounts:

```sh
stellar keys generate deployer --network testnet --fund
stellar keys generate verifier1 --network testnet --fund
stellar keys generate donor1 --network testnet --fund
stellar keys generate beneficiary1 --network testnet --fund
stellar keys generate sponsor1 --network testnet --fund
```

> **Note on amounts**: Amounts are denominated in token stroops (7 decimals for native XLM: `1 XLM = 10_000_000` stroops; `100 XLM = 1_000_000_000` stroops).

---

## Table of Contents

- [Contract Initialization](#contract-initialization)
  - [\_\_constructor](#__constructor)
- [Campaign Lifecycle](#campaign-lifecycle)
  - [create_campaign](#create_campaign)
  - [create_campaign_with_due_dates](#create_campaign_with_due_dates)
  - [donate](#donate)
  - [approve_milestone](#approve_milestone)
  - [cancel_campaign](#cancel_campaign)
  - [refund](#refund)
  - [emergency_fast_track](#emergency_fast_track)
- [Verifier Management & Bonds](#verifier-management--bonds)
  - [add_verifier](#add_verifier)
  - [remove_verifier](#remove_verifier)
  - [reassign_verifier](#reassign_verifier)
  - [set_bond_requirement](#set_bond_requirement)
  - [set_bond_withdraw_delay](#set_bond_withdraw_delay)
  - [register_with_bond](#register_with_bond)
  - [deregister_verifier](#deregister_verifier)
  - [withdraw_bond](#withdraw_bond)
  - [slash_verifier](#slash_verifier)
- [Sponsor Matching Pools & Pledges](#sponsor-matching-pools--pledges)
  - [deposit_sponsor_pool](#deposit_sponsor_pool)
  - [apply_matching](#apply_matching)
  - [return_sponsor_pool](#return_sponsor_pool)
  - [create_pledge](#create_pledge)
- [Administration](#administration)
  - [set_admin](#set_admin)
- [Read-Only Views & Maintenance](#read-only-views--maintenance)
  - [get_campaign](#get_campaign)
  - [campaign_count](#campaign_count)
  - [contribution_of](#contribution_of)
  - [is_refundable](#is_refundable)
  - [is_verifier](#is_verifier)
  - [admin](#admin)
  - [token](#token)
  - [get_verifier_bond](#get_verifier_bond)
  - [bond_requirement](#bond_requirement)
  - [bond_withdraw_delay](#bond_withdraw_delay)
  - [get_sponsor_pool](#get_sponsor_pool)
  - [get_pledge](#get_pledge)
  - [bump_campaign_ttl](#bump_campaign_ttl)
  - [extend_campaign_ttl](#extend_campaign_ttl)

---

## Contract Initialization

### `__constructor`

Initializes contract storage with the admin and accepted token contract address during contract deployment.

```sh
stellar contract deploy \
  --wasm target/wasm32v1-none/release/aidline.wasm \
  --source deployer \
  --network testnet \
  -- \
  --admin $(stellar keys address deployer) \
  --token $NATIVE_XLM_ID
```

---

## Campaign Lifecycle

### `create_campaign`

Creates a new campaign. The total campaign goal is calculated as the sum of all milestone amounts. The `verifier` must not equal the `creator` or `beneficiary`, and `metadata_uri` cannot exceed 256 bytes.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- create_campaign \
  --creator $(stellar keys address deployer) \
  --beneficiary $(stellar keys address beneficiary1) \
  --verifier $(stellar keys address verifier1) \
  --token $NATIVE_XLM_ID \
  --kind Climate \
  --metadata_uri "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi" \
  --deadline 1767225600 \
  --milestones '[5000000000, 5000000000]'
```

### `create_campaign_with_due_dates`

Creates a campaign with explicit due dates for each milestone.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- create_campaign_with_due_dates \
  --creator $(stellar keys address deployer) \
  --beneficiary $(stellar keys address beneficiary1) \
  --verifier $(stellar keys address verifier1) \
  --token $NATIVE_XLM_ID \
  --kind Climate \
  --metadata_uri "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi" \
  --deadline 1767225600 \
  --milestones '[5000000000, 5000000000]' \
  --milestone_due_dates '[1751328000, 1767225600]'
```

### `donate`

Donates tokens into the campaign escrow. Returns the accepted donation amount (capped to the remaining goal).

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source donor1 \
  --network testnet \
  -- donate \
  --donor $(stellar keys address donor1) \
  --campaign_id 0 \
  --amount 1000000000
```

### `approve_milestone`

Called by the campaign's registered verifier to release the next milestone payout to the beneficiary. `proof_uri` must not exceed 256 bytes.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source verifier1 \
  --network testnet \
  -- approve_milestone \
  --campaign_id 0 \
  --proof_uri "ipfs://bafybeic7proofofcompletionforfirstmilestoneexampleuri"
```

### `cancel_campaign`

Stops an active campaign early so donors can claim unreleased funds. Can be called by the campaign creator or the admin.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- cancel_campaign \
  --caller $(stellar keys address deployer) \
  --campaign_id 0
```

### `refund`

Claims a pro-rata refund of unreleased escrow funds for a donor after a campaign has expired, been cancelled, or missed a milestone due date.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source donor1 \
  --network testnet \
  -- refund \
  --donor $(stellar keys address donor1) \
  --campaign_id 0
```

### `emergency_fast_track`

For `Emergency` campaigns, allows the creator to withdraw an immediate advance (up to 20% of goal, bounded by current escrow) to respond rapidly to crises.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- emergency_fast_track \
  --creator $(stellar keys address deployer) \
  --campaign_id 0
```

---

## Verifier Management & Bonds

### `add_verifier`

Called by the admin or a `VerifierManager` role holder to register a trusted verifier without requiring a bond.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- add_verifier \
  --caller $(stellar keys address deployer) \
  --verifier $(stellar keys address verifier1)
```

### `remove_verifier`

Called by the admin or a `VerifierManager` role holder to deregister a verifier and freeze their approvals.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- remove_verifier \
  --caller $(stellar keys address deployer) \
  --verifier $(stellar keys address verifier1)
```

### `reassign_verifier`

Called by the admin on an active campaign to reassign verification duties to a new registered verifier (e.g., if the original verifier became unavailable).

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- reassign_verifier \
  --campaign_id 0 \
  --new_verifier $(stellar keys address verifier1)
```

### `set_bond_requirement`

Called by the admin to set the required token bond for permissionless verifier registration.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- set_bond_requirement \
  --amount 1000000000
```

### `set_bond_withdraw_delay`

Called by the admin to configure the lockup period in seconds before a deregistered verifier can withdraw their bond.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- set_bond_withdraw_delay \
  --delay 604800
```

### `register_with_bond`

Allows an address to register as a verifier permissionlessly by depositing at least the required bond amount into escrow.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source verifier1 \
  --network testnet \
  -- register_with_bond \
  --verifier $(stellar keys address verifier1) \
  --bond_amount 1000000000
```

### `deregister_verifier`

Initiates withdrawal of a verifier's bond. Can be called by the verifier themselves or by the admin.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source verifier1 \
  --network testnet \
  -- deregister_verifier \
  --caller $(stellar keys address verifier1) \
  --verifier $(stellar keys address verifier1)
```

### `withdraw_bond`

Withdraws the bond back to the verifier once the withdrawal delay has elapsed after deregistration.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source verifier1 \
  --network testnet \
  -- withdraw_bond \
  --verifier $(stellar keys address verifier1)
```

### `slash_verifier`

Called by the admin or a `Slasher` role holder to slash a bonded verifier's deposit for misconduct, routing slashed funds into the specified campaign escrow.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- slash_verifier \
  --caller $(stellar keys address deployer) \
  --verifier $(stellar keys address verifier1) \
  --campaign_id 0 \
  --slash_amount 500000000
```

---

## Sponsor Matching Pools & Pledges

### `deposit_sponsor_pool`

Creates or tops up a sponsor matching pool for a campaign. `ratio_bps` specifies the matching ratio in basis points (100% = 10,000 bps).

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source sponsor1 \
  --network testnet \
  -- deposit_sponsor_pool \
  --sponsor $(stellar keys address sponsor1) \
  --campaign_id 0 \
  --ratio_bps 5000 \
  --cap 2000000000 \
  --deposit 2000000000
```

### `apply_matching`

Applies matching funds from a sponsor pool against a specific donor contribution.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source sponsor1 \
  --network testnet \
  -- apply_matching \
  --sponsor $(stellar keys address sponsor1) \
  --campaign_id 0 \
  --donor $(stellar keys address donor1) \
  --donation_amount 1000000000
```

### `return_sponsor_pool`

Returns remaining unmatched funds from a sponsor pool back to the sponsor after the campaign deadline has passed.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source sponsor1 \
  --network testnet \
  -- return_sponsor_pool \
  --sponsor $(stellar keys address sponsor1) \
  --campaign_id 0
```

### `create_pledge`

Registers a conditional pledge by a donor with a pre-approved allowance. Pledged funds are pulled into escrow automatically when milestones are approved.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source donor1 \
  --network testnet \
  -- create_pledge \
  --donor $(stellar keys address donor1) \
  --campaign_id 0 \
  --max_amount 2000000000
```

---

## Administration

### `set_admin`

Transfers administrative control of the contract to a new address.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- set_admin \
  --new_admin $(stellar keys address deployer)
```

---

## Read-Only Views & Maintenance

Read-only views simulate execution locally without submitting an on-chain transaction.

### `get_campaign`

Returns campaign details (creator, beneficiary, verifier, kind, raised, released, status, milestones, etc.).

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- get_campaign \
  --campaign_id 0
```

### `campaign_count`

Returns the total number of campaigns created.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- campaign_count
```

### `contribution_of`

Returns the total token amount contributed by a donor to a specific campaign.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- contribution_of \
  --campaign_id 0 \
  --donor $(stellar keys address donor1)
```

### `is_refundable`

Checks whether donors can currently claim refunds on a campaign (returns `true` for cancelled, expired, or overdue campaigns).

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- is_refundable \
  --campaign_id 0
```

### `is_verifier`

Checks whether an address is currently a registered verifier.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- is_verifier \
  --who $(stellar keys address verifier1)
```

### `admin`

Returns the contract admin address.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- admin
```

### `token`

Returns the address of the token used for escrow and payouts.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- token
```

### `get_verifier_bond`

Returns the bond record for a verifier (amount, status, deregistered_at).

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- get_verifier_bond \
  --verifier $(stellar keys address verifier1)
```

### `bond_requirement`

Returns the minimum bond required to register as a verifier.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- bond_requirement
```

### `bond_withdraw_delay`

Returns the delay period (in seconds) required before withdrawing a bond after deregistration.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- bond_withdraw_delay
```

### `get_sponsor_pool`

Returns the matching pool record for a sponsor on a campaign (cap, remaining, matched, ratio_bps).

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- get_sponsor_pool \
  --campaign_id 0 \
  --sponsor $(stellar keys address sponsor1)
```

### `get_pledge`

Returns the details of a pledge by ID.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- get_pledge \
  --pledge_id 0
```

### `bump_campaign_ttl`

Extends the storage TTL of a campaign, its due dates and other campaign-wide entries, and the contract instance. Needs no authorization, so a keeper can call it for long-running campaigns.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source keeper \
  --network testnet \
  -- bump_campaign_ttl \
  --campaign_id 0
```

### `extend_campaign_ttl`

Same as [`bump_campaign_ttl`](#bump_campaign_ttl), kept under its earlier name.

```sh
stellar contract invoke \
  --id $AIDLINE_CONTRACT_ID \
  --source deployer \
  --network testnet \
  -- extend_campaign_ttl \
  --campaign_id 0
```
