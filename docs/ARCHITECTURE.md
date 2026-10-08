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
| Admin | Adds and removes verifiers, can cancel any campaign, can hand admin to another address, can slash verifier bonds |
| Creator | Opens campaigns, can cancel their own campaign |
| Beneficiary | Receives milestone payouts. Often the same as the creator, but can be a separate NGO wallet |
| Verifier | Approves milestones for campaigns assigned to them, while registered by the admin. Must post a bond to register. |
| Donor | Donates, and claims refunds when a campaign is cancelled or expired |
| Sponsor | Deposits a matching pool for a specific campaign |
| Pledgor | Creates a pledge backed by a token allowance; funds are pulled when a milestone is approved |

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
| `SponsorPool(u64, Address)` | persistent | `SponsorPool` struct (Issue #27) |
| `VerifierBond(Address)` | persistent | `VerifierBond` struct (Issue #29) |
| `BondRequirement` | instance | `i128` required bond amount (Issue #29) |
| `BondWithdrawDelay` | instance | `u64` withdrawal delay in seconds (Issue #29) |
| `PledgeCount` | instance | `u64` next pledge id (Issue #30) |
| `Pledge(u64)` | persistent | `Pledge` struct (Issue #30) |

TTLs are extended on every read and write (instance: 30 days, persistent: 120 days). Long running climate campaigns may need an off chain keeper to extend TTLs; this is tracked as an open issue.

## Refund math

Once a campaign is cancelled or expired, `raised` and `released` can no longer change, because both `donate` and `approve_milestone` require an open campaign. Each donor's refund is:

```
refund = contribution * (raised - released) / raised
```

Integer division rounds down, so a few stroops of dust can remain in the contract per campaign. This is deliberate: rounding in favor of the contract means the last donor can never be short.

Sponsors are also included in the refund pool (their `Contribution` entry tracks the matched amount), so unused sponsor matching funds can be partially reclaimed through the standard refund path in addition to `return_sponsor_pool`.

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
| `admin_changed` | `old_admin` | `new_admin` |
| `sponsor_pool_deposited` | `campaign_id`, `sponsor` | `amount`, `ratio_bps`, `cap` |
| `matching_applied` | `campaign_id`, `sponsor` | `donor`, `donation_amount`, `matched_amount` |
| `sponsor_pool_returned` | `campaign_id`, `sponsor` | `amount` |
| `verifier_bond_posted` | `verifier` | `amount` |
| `verifier_deregistered` | `verifier` | `deregistered_at` |
| `bond_withdrawn` | `verifier` | `amount` |
| `bond_slashed` | `verifier` | `campaign_id`, `slashed_amount` |
| `pledge_created` | `pledge_id`, `campaign_id` | `donor`, `pledged_amount` |
| `pledge_pulled` | `pledge_id`, `campaign_id` | `donor`, `pulled_amount` |
| `pledge_skipped` | `pledge_id`, `campaign_id` | `donor`, `reason` |
| `pledge_settlement` | `campaign_id` | `milestone_index`, `total_pledged_pulled` |
| `verifier_reassigned` | `campaign_id` | `old_verifier`, `new_verifier` |

The backend indexer reads these to build campaign pages, donor histories and impact reports without scanning contract storage.

## Design decisions

**Donations are capped at the goal.** This keeps milestone math exact. Allowing overfunding (with the surplus going to a reserve or back to donors) is a planned improvement.

**Milestones are released strictly in order.** Partial or parallel milestones add complexity that the MVP does not need.

**Proofs live off chain.** Only the proof URI is emitted. Photos and receipts belong on IPFS or the backend, not in contract storage.

**One token per deployment.** Multi asset support is a planned improvement.

**Removing a verifier freezes their campaigns.** Funds are not lost: the creator or admin can cancel, which opens refunds. The admin can also reassign the verifier on an active campaign using `reassign_verifier`.

---

## Issue #27 — Sponsor Matching Funds

### What it is

Diaspora associations and companies can pledge to match community donations at a configured ratio up to a configured maximum cap. The matched funds come from a pool the sponsor deposits into the contract ahead of time, so they are always real tokens already held in escrow.

### How the ratio works

The sponsor specifies a ratio in **basis points** (bps): 100 bps = 1 %, 5 000 bps = 50 %, 10 000 bps = 100 %. For a donation of `D` tokens the raw match is:

```
raw_match = D * ratio_bps / 10_000
```

Integer division is used, so tiny donations may produce 0 match (not an error).

### How the cap works

The sponsor sets a `cap` at deposit time equal to the deposited `amount`. For the lifetime of the pool, the total matched can never exceed `cap`. After the cap is reached, further calls to `apply_matching` return 0.

### Matching constraints

The effective match for a single call to `apply_matching` is the minimum of:
1. `raw_match` (ratio-based)
2. `pool.cap - pool.matched` (cap headroom)
3. `pool.remaining` (undepleted pool balance)
4. `campaign.goal - campaign.raised` (campaign headroom — cannot overfund)

### When the pool is exhausted

When `pool.remaining == 0`, `apply_matching` returns 0. No error is raised; the caller just receives no match. The sponsor pool record remains until `return_sponsor_pool` is called.

### How unused funds are returned

After the campaign is no longer active (Completed, Cancelled, or past its deadline), the sponsor calls `return_sponsor_pool`. The remaining pool balance is transferred back to the sponsor and the pool record is deleted.

### Why `apply_matching` is separate from `donate`

Soroban persistent storage does not support prefix-scanning (iterating all keys with a given prefix). Because sponsors are not enumerable inside `donate`, the matching must be triggered explicitly. Any account — the donor, the sponsor, or a keeper — may call `apply_matching` at any time while the campaign is active.

### Accounting and security trade-offs

- **Funds are always real.** The pool is funded up front, so `apply_matching` never mints tokens — it only moves accounting entries.
- **Sponsors share in refunds.** The sponsor's matched amount is recorded as a `Contribution` entry, so they receive a pro-rata refund of unmatched/unreleased funds when a campaign is cancelled or expired. Additionally, `return_sponsor_pool` returns any tokens that were deposited but never matched.
- **No overfunding.** The campaign-headroom cap ensures that matching cannot push `raised` above `goal`.
- **One pool per sponsor per campaign.** Prevents accidental double-deposits. A sponsor must call `return_sponsor_pool` to clear a pool before creating a new one.
- **Integer truncation.** All matching uses integer division truncating toward zero. This means dust can accumulate in the pool but no funds are ever created from nothing.

---

## Issue #29 — Verifier Bonds and Slashing

### Why verifier bonds exist

Under the baseline design verifiers risk nothing when approving false milestones. A bond makes approval costly for bad actors: if a verifier approves fraudulent work, the admin can slash their bond and distribute the slashed amount to the affected campaign's donors.

### Bond requirement

The admin sets a required bond amount (`set_bond_requirement`) and withdrawal delay (`set_bond_withdraw_delay`) before bonds are enforced. Verifiers must call `register_with_bond` (which replaces `add_verifier` for bonded registrations) and transfer at least the required bond. If the transferred amount is less than the requirement, registration fails.

### How registration works

`register_with_bond(verifier, bond_amount)`:
1. Requires auth from the verifier.
2. Transfers `bond_amount` tokens from the verifier to the contract.
3. Stores a `VerifierBond` record with status `Active`.
4. Marks the verifier as active in the verifier registry.
5. Emits `VerifierBondPosted`.

### How deregistration works

`deregister_verifier(verifier)`:
1. Requires auth from the verifier or the admin.
2. Sets bond status to `PendingWithdrawal` and records `deregistered_at`.
3. Removes the verifier from the active registry.
4. Emits `VerifierDeregistered`.

### Withdrawal delay semantics

After deregistration the verifier must wait at least `bond_withdraw_delay` seconds before calling `withdraw_bond`. This delay gives the admin time to slash if a fraud dispute is raised.

### How disputes and slashing work

`slash_verifier(admin, verifier, campaign_id, slash_amount)`:
1. Requires auth from the admin.
2. Checks that `slash_amount <= bond.remaining`.
3. Reduces `bond.remaining` by `slash_amount`.
4. Distributes the slashed tokens back to the affected campaign's donors by adding `slash_amount` to `campaign.raised` and crediting the contract as the "donor", so the existing pro-rata refund formula automatically distributes the recovered funds to all contributors when they call `refund`.
5. Emits `BondSlashed`.

Preventing double-slash for the same dispute is enforced by tracking slashed amounts; the admin cannot slash more than `bond.remaining`.

### Security and trust assumptions

- Only the admin can slash.
- A verifier cannot withdraw a slashed portion.
- The withdrawal delay ensures time for the admin to act on disputes.
- Slashing distributes through the existing refund formula — no new distribution logic needed.

### Trade-offs

- A single admin can slash, so the design trusts the admin. A future improvement is a multisig or DAO-controlled slash authority.
- The withdrawal delay is a fixed parameter; campaigns that run longer than the delay may not be fully protected.

---

## Issue #30 — Pledges That Fund Future Milestones on Approval

### Why pledges exist

Some donors prefer to commit funds only when work is verified, rather than transferring tokens up front. Pledges allow a donor to pre-authorize a token allowance; the contract pulls funds proportionally when each milestone is approved.

### How pledges differ from escrow

With normal donations, tokens move into the contract immediately. With pledges, tokens stay in the donor's wallet until a milestone is approved. The donor must maintain a sufficient token allowance pointing at the contract; if the allowance is insufficient at pull time, the pledge is skipped for that milestone.

### Token allowance semantics

Before creating a pledge the donor must call `approve` on the token contract to grant the Aidline contract an allowance of at least `pledged_amount`. The contract reads the allowance at pull time via `token::Client::allowance`. If the allowance is lower than the donor's remaining pledge, the contract pulls only what is available (or skips the pledge entirely if nothing is available).

### When funds are actually pulled

Funds are pulled inside `approve_milestone` after the verifier's signature is verified and the milestone is confirmed funded (by direct donations). Active pledges for the campaign are iterated and pulled proportionally.

### Proportional allocation

Given a set of active pledges `[p1, p2, …, pN]` and a milestone amount `M`, each pledge's share is:

```
share_i = M * p_i.remaining / total_remaining
```

where `total_remaining = sum of all p_i.remaining` for active pledges.

Integer division truncates toward zero. Any unallocated remainder stays in the milestone's existing funded balance (direct donations). A pledge that cannot cover its full share is pulled for as much as the allowance permits; the rest is skipped without reverting.

### What happens when allowance is unavailable

If a donor's token allowance is 0 or insufficient for their share:
- The pledge is skipped for this milestone.
- A `PledgeSkipped` event is emitted.
- Other pledges are still processed.
- The milestone approval does not revert.

### Rounding and accounting behavior

- All arithmetic uses `i128` with integer truncation.
- No pledge pull can exceed the donor's remaining `pledged_amount`.
- No pledge pull can exceed the donor's current token allowance.
- The total pledged pulled is added to `campaign.raised` and recorded as a `Contribution` so pledgors share in refunds if the campaign is cancelled before completion.

### Donor/user experience

1. Donor approves the token contract: `token.approve(donor, aidline_contract, pledged_amount, expiry)`.
2. Donor calls `create_pledge(donor, campaign_id, pledged_amount)`.
3. When each milestone is approved, the contract automatically pulls the donor's proportional share.
4. If the donor revokes their allowance, the pledge is silently skipped.

### Security and trust trade-offs

- **No custody until pull.** Donor retains control of tokens until milestone approval.
- **Allowance can be revoked.** Donors can back out by revoking the allowance; this is a feature, not a bug.
- **No guaranteed funding.** If all pledges are revoked, milestone approval may still succeed if direct donations cover the milestone (pledges are supplementary, not substitutive).
- **Deterministic allocation.** The proportional formula is deterministic given the same on-chain state; there is no randomness or ordering advantage.

### Advantages versus escrow

| Property | Pledge | Escrow (donate) |
| --- | --- | --- |
| When tokens leave donor wallet | At milestone approval | At donation time |
| Donor retains control until | Milestone approval | Never |
| Guaranteed funding | No (allowance may lapse) | Yes (tokens already in contract) |
| Refund needed if campaign fails | No (tokens never left) | Yes |

---

## Known limitations and roadmap

- Verifier reassignment for a live campaign
- Multisig verification (m of n verifiers per milestone)
- Overfunding and stretch goals
- Multi token campaigns
- TTL keeper for very long climate campaigns
- Enumerate sponsor pools per campaign (requires off-chain indexer)
- Enumerate pledges per campaign (requires off-chain indexer)
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
