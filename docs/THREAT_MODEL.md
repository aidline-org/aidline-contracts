# Threat model

This document describes what each role in the Aidline contract can and cannot do, which attacks we have considered, and which risks are still open. It is meant for contributors, reviewers and anyone preparing an audit. It is not a substitute for an external audit, which is required before mainnet.

Read [ARCHITECTURE.md](ARCHITECTURE.md) first for the roles, storage and lifecycle.

## What we protect

1. **Donor funds held in escrow.** They may leave the contract only through a verifier approved milestone payout, an emergency advance within its cap, or a donor refund.
2. **Honest accounting.** `raised`, `released` and each donor's contribution must always add up, so refunds are fair.
3. **Verifiable payouts.** Every release emits an event that links the payout to its proof URI.

## Trust assumptions

- **The admin is trusted.** One address can add and remove verifiers, cancel any campaign, reassign a campaign's verifier, set bond rules and slash bonds. A compromised admin key is the largest single risk in the MVP. Moving admin rights to a multisig is on the [roadmap](../ROADMAP.md).
- **Verifiers are trusted per campaign, within limits.** A verifier decides whether a milestone was delivered. Bonds and slashing make a false approval costly, but a verifier can still approve work that was not done. Donors are protected only for funds not yet released.
- **The token contract is trusted.** One token is set at deploy time. A malicious or upgradable token could break escrow, so deploy only with a well known asset such as native XLM or USDC.
- **Soroban and the host environment behave as documented.** This includes authorization checks (`require_auth`), ledger timestamps and storage TTL rules.

## What each role can and cannot do

| Role        | Can                                                                                                                       | Cannot                                                                                                                  |
| ----------- | ------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Admin       | Add and remove verifiers, cancel any campaign, reassign a campaign's verifier, hand over admin, set bond rules, slash bonds | Withdraw donor funds, approve milestones, change a campaign's goal or milestones, or refund on behalf of a donor         |
| Creator     | Open campaigns, cancel their own campaign                                                                                 | Release funds, choose a verifier who is not registered, or change milestones after creation                              |
| Beneficiary | Receive payouts                                                                                                           | Trigger a payout                                                                                                        |
| Verifier    | Approve the next milestone and take the emergency advance, for campaigns assigned to them while registered               | Approve milestones out of order, release more than a milestone's amount, or act on campaigns assigned to someone else     |
| Donor       | Donate, refund after cancel or expiry, create pledges and revoke allowances                                              | Withdraw before the campaign is cancelled or expired                                                                     |
| Sponsor     | Deposit a matching pool and reclaim what was not matched                                                                  | Match beyond the pool cap or push a campaign over its goal                                                               |
| Anyone      | Call `apply_matching` while a campaign is active, and read any view function                                              | Move funds except through the paths above                                                                                |

## Attacks considered

| Threat                                             | Mitigation today                                                                                                            | Status         |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- | -------------- |
| Someone acts as another address                    | Every state changing call uses `require_auth` on the acting address                                                         | Mitigated      |
| Release more than was raised                       | Donations are capped at the goal. A milestone needs to be funded before approval, and the emergency advance is capped at 20% of the goal and at available escrow | Mitigated      |
| Release milestones out of order                    | Milestones are released strictly in order                                                                                   | Mitigated      |
| Double take of the emergency advance               | One advance per campaign, tracked on the campaign record                                                                    | Mitigated      |
| Last donor is short on refund                      | Refund math rounds down, so dust stays in the contract and the last refund is always payable                                | Mitigated      |
| Integer overflow in sums and ratios                | Goal is summed with `checked_add`, and release builds enable `overflow-checks`                                              | Mitigated      |
| Removed verifier locks funds                       | Creator or admin can cancel, which opens refunds. Admin can also reassign the verifier                                      | Mitigated      |
| Verifier approves false work                       | Verifier bonds, withdrawal delay and admin slashing, with slashed funds flowing to the campaign's donors                    | Partly open    |
| Verifier colludes with the beneficiary             | A campaign can currently name the same address as verifier and beneficiary. See [issue #14](https://github.com/aidline-org/aidline-contracts/issues/14) | Open           |
| Oversized metadata or proof URI bloats storage     | No length limit yet. See [issue #13](https://github.com/aidline-org/aidline-contracts/issues/13)                           | Open           |
| Pledge allowance revoked to dodge a commitment     | Skipped without reverting, by design. Pledges are supplementary, not guaranteed funding                                    | Accepted       |
| Storage entry expires on a long campaign           | TTLs are extended on every read and write. A very long campaign may need an off chain keeper                               | Open           |
| Admin key compromised                              | Single address today                                                                                                        | Open           |
| Contract bug                                       | Unit tests, a fuzz target run in CI, and a bounded fuzz session on every pull request                                       | Audit required |

## Out of scope

- The backend and frontend. See their own `SECURITY.md` files.
- Wallet and key management by users.
- Misbehavior of the underlying Stellar network or the token contract.
- Off chain proof content. The contract only stores and emits the URI.

## Reporting a problem

Do not open a public issue for a vulnerability. Follow [SECURITY.md](../SECURITY.md).

## Before mainnet

- External audit of the full contract
- Close the open rows above, or document why they are accepted
- Multisig or DAO controlled admin and slash authority
- A deploy record and a tested upgrade or migration plan
