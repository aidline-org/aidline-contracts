# 0003: Refunds round down in favor of the contract

Status: Accepted

## Context

When a campaign is cancelled or expires, each donor gets back a share of what was never released:

```
refund = contribution * (raised - released) / raised
```

Integer division cannot split every amount exactly, so some stroops are left over.

## Decision

Use integer division, which rounds down. The leftover dust stays in the contract.

## Consequences

- The contract can always pay every donor, including the last one. If it rounded up, the final refund could exceed the balance.
- A few stroops of dust may remain per campaign. This is deliberate and has no meaningful value.
- Sponsor matched amounts are recorded as contributions, so they share the same formula.
- Tests should assert the sum of all refunds is less than or equal to `raised - released`, never more.
