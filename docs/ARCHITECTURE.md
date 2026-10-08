# Architecture

This document explains how the Aidline contract is put together and why. If you are about to change contract behavior, read this first.

## Goals

- **Donors can verify where money went.** Every movement of funds emits an event and is tied to a proof link.
- **Nobody holds funds on trust.** Money only leaves escrow through a verifier approval or a donor refund.
- **Donors are never stuck.** If a campaign stalls, unreleased funds can always be reclaimed.
- **Simple enough to audit.** One contract, one token, no upgradability tricks in the MVP.

## Roles

| Role | Powers |
| --- | --- |
| Admin | Adds and removes verifiers, can cancel any campaign, can hand admin to another address |
| Creator | Opens campaigns, can cancel their own campaign |
| Beneficiary | Receives milestone payouts. Often the same as the creator, but can be a separate NGO wallet |
| Verifier | Approves milestones for campaigns assigned to them, while registered by the admin |
| Donor | Donates, and claims refunds when a campaign is cancelled or expired |

## Campaign lifecycle

```
              create_campaign
                    │
                    ▼
               ┌─────────┐   approve last milestone   ┌───────────┐
   donate ───▶ │ Active  │ ─────────────────────────▶ │ Completed │
   approve ──▶ │         │                            └───────────┘
               └────┬────┘
        cancel      │      deadline passes
     ┌──────────────┴──────────────┐
     ▼                             ▼
┌───────────┐              Active but expired
│ Cancelled │              (no stored status change)
└─────┬─────┘                      │
      └──────────┬─────────────────┘
                 ▼
          donors call refund
```

Expiry is computed from the ledger timestamp rather than stored, so no one needs to send a transaction to "close" a campaign.

## Emergency Fast Track

For `Emergency` campaigns, the assigned verifier can release an emergency fast-track advance immediately, before milestone one is completed. 
This provides critical funds in the first hours of a disaster without waiting for a full milestone to be funded or proven.

- **Cap:** The advance is hard-capped at 20% of the campaign's total goal.
- **Escrow requirements:** The advance cannot exceed the currently available unreleased escrow.
- **Accounting:** The advance is recorded against milestone one (`milestones[0]`). When the verifier later formally approves milestone one, the contract only transfers the remaining balance (if any) of that milestone to the beneficiary. The total funds released for milestone one never exceed its scheduled amount.
- **Security:** Only the registered verifier can trigger the fast track. It is only available for `Emergency` campaigns, and only once per campaign.

## Storage

| Key | Storage type | Value |
| --- | --- | --- |
| `Admin` | instance | `Address` |
| `Token` | instance | `Address` of the Stellar Asset Contract used for all campaigns |
| `CampaignCount` | instance | `u64`, also the next campaign id |
| `Verifier(Address)` | persistent | `true` when registered, removed when unregistered |
| `Campaign(u64)` | persistent | `Campaign` struct |
| `Contribution(u64, Address)` | persistent | `i128` total donated by that donor, removed after refund |

TTLs are extended on every read and write (instance: 30 days, persistent: 120 days). Long running climate campaigns may need an off chain keeper to extend TTLs; this is tracked as an open issue.

## Refund math

Once a campaign is cancelled or expired, `raised` and `released` can no longer change, because both `donate` and `approve_milestone` require an open campaign. Each donor's refund is:

```
refund = contribution * (raised - released) / raised
```

Integer division rounds down, so a few stroops of dust can remain in the contract per campaign. This is deliberate: rounding in favor of the contract means the last donor can never be short.

## Events

| Event | Topics | Data |
| --- | --- | --- |
| `campaign_created` | `campaign_id` | `creator`, `kind`, `goal`, `deadline` |
| `donated` | `campaign_id`, `donor` | `amount` |
| `emergency_advance_released` | `campaign_id`, `verifier` | `milestone_index`, `amount` |
| `milestone_released` | `campaign_id` | `index`, `amount`, `proof_uri` |
| `campaign_cancelled` | `campaign_id` | none |
| `refunded` | `campaign_id`, `donor` | `amount` |
| `verifier_updated` | `verifier` | `active` |

The backend indexer reads these to build campaign pages, donor histories and impact reports without scanning contract storage.

## Design decisions

**Donations are capped at the goal.** This keeps milestone math exact. Allowing overfunding (with the surplus going to a reserve or back to donors) is a planned improvement.

**Milestones are released strictly in order.** Partial or parallel milestones add complexity that the MVP does not need.

**Proofs live off chain.** Only the proof URI is emitted. Photos and receipts belong on IPFS or the backend, not in contract storage.

**One token per deployment.** Multi asset support is a planned improvement.

**Removing a verifier freezes their campaigns.** Funds are not lost: the creator or admin can cancel, which opens refunds. Reassigning a verifier is a planned improvement.

## Known limitations and roadmap

- Verifier reassignment for a live campaign
- Multisig verification (m of n verifiers per milestone)
- Overfunding and stretch goals
- Multi token campaigns
- TTL keeper for very long climate campaigns
- External security audit before mainnet

## Resource Costs (Issue #24)

To help contributors understand the impact of contract changes, the following table tracks the resource costs (CPU instructions and memory usage) for the main entry points.

| Function | CPU Instructions | Memory | Notes |
| -------- | ---------------: | -----: | ----- |
| `create_campaign` | (Requires Regeneration) | (Requires Regeneration) | 2 milestones |
| `donate` | (Requires Regeneration) | (Requires Regeneration) | Standard donation |
| `emergency_fast_track` | (Requires Regeneration) | (Requires Regeneration) | First-time advance |
| `approve_milestone` | (Requires Regeneration) | (Requires Regeneration) | Milestone 1 |
| `cancel_campaign` | (Requires Regeneration) | (Requires Regeneration) | By creator |
| `refund` | (Requires Regeneration) | (Requires Regeneration) | Single donor |

*Note: The values above currently require regeneration.*

### How to regenerate the table

Maintainers can regenerate these measurements using the built-in test environment. The test `test_measure_resource_costs` in `contracts/aidline/src/test.rs` executes each entry point and resets the budget.

1. Ensure you have the Soroban CLI and test tools installed.
2. Run the test and capture the output:
   ```bash
   cargo test test_measure_resource_costs -- --nocapture
   ```
3. Read the printed budget costs for each step and update the table above.
