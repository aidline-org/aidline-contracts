# 0001: Release funds by milestone, approved by a verifier

Status: Accepted

## Context

Diaspora donors give to relief and climate work far from where they live and usually cannot check what happened to the money. A normal transfer gives them no leverage after the gift. Paying everything up front invites waste and fraud. Requiring every donor to review evidence themselves does not scale.

## Decision

Hold donations in the contract and release them one milestone at a time. A registered verifier approves each milestone with a proof URI, and only then does the next milestone's amount go to the beneficiary. Milestones are released strictly in order. If a campaign is cancelled or passes its deadline, donors can refund whatever was not released.

Emergency campaigns also allow a single advance of up to 20% of the goal, so money can move in the first hours of a disaster.

## Consequences

- Donors risk only the funds not yet released.
- Trust moves to the verifier. Verifier bonds and slashing reduce but do not remove that risk. See the [threat model](../THREAT_MODEL.md).
- Ordering and a single token keep the accounting simple enough to audit.
- Parallel or partial milestones, multi token campaigns and multisig verification are possible later but are out of scope for the MVP.
