# 0002: Campaign expiry is computed, not stored

Status: Accepted

## Context

A campaign has a deadline. After it passes, donations and approvals must stop and donors must be able to refund. One way to model this is a stored `Expired` status that someone has to set with a transaction.

## Decision

Do not store an expired status. Compare the deadline with the ledger timestamp whenever it matters. A campaign past its deadline is treated as closed by `donate`, `approve_milestone` and the other calls that need an open campaign, and `refund` allows refunds.

## Consequences

- Nobody has to pay to "close" a campaign, and no keeper is needed for expiry.
- An expired campaign still reads as `Active` in storage. Clients, including the backend indexer, must compute expiry from the deadline.
- Because `raised` and `released` cannot change after the deadline, refund amounts are stable and can be computed by anyone.
