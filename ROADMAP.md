# Roadmap

Where the Aidline contract is heading. GitHub is the source of truth for status: see [open issues](https://github.com/aidline-org/aidline-contracts/issues) and [ISSUES.md](ISSUES.md). Design details live in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Done

- Milestone escrow with verifier approval and pro rata refunds
- Emergency fast track advance for disaster campaigns
- Sponsor matching pools
- Verifier bonds and slashing
- Pledges that fund future milestones on approval
- Verifier reassignment on live campaigns
- Per milestone due dates
- Fuzz target running in CI

## Now: close known gaps

- Reject campaigns where the verifier is also the beneficiary
- Limit the length of metadata and proof URIs
- Read only views such as `is_refundable` and a contract version
- Document every contract call with Stellar CLI examples
- Regenerate the resource cost table

## Next: stronger trust model

- Multisig verification, with m of n verifiers per milestone
- Admin and slash authority held by a multisig or DAO
- A keeper or documented process to extend TTLs on long campaigns
- Overfunding and stretch goals

## Later

- Multi token campaigns
- Enumeration of sponsor pools and pledges through the indexer
- Upgrade and migration plan

## Before mainnet

- External security audit
- All open rows in the [threat model](docs/THREAT_MODEL.md) closed or accepted in writing
- Tagged release with a recorded WASM hash

## How to influence this

Open an issue describing the problem, or comment on an existing one. Contract changes need tests, and changes to behavior need an update to the architecture doc. See [CONTRIBUTING.md](CONTRIBUTING.md).
