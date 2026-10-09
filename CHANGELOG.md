# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Emergency fast track: a partial first tranche for Emergency campaigns (#23, @oladayo2222)
- Per milestone due dates with partial refunds when a milestone is overdue (#26, @oladayo2222)
- Sponsor matching pools that match donations up to a cap (#27, @Feyisara2108)
- Fuzz testing harness, run for 30 seconds in CI (#28, @oladayo2222)
- Verifier bonds with slashing and a withdrawal delay (#29, @oladayo2222)
- Pledges that fund future milestones on approval (#30, @oladayo2222)
- Admin can reassign a campaign's verifier (#4, @RaymondAbiola)
- `AdminChanged` and `VerifierReassigned` events (#2, @RaymondAbiola)
- Doc comments on every public function (#1, @RaymondAbiola), a resource cost table and exact event assertions in tests (#24, #25, @oladayo2222)

### Fixed

- Fuzz harness now builds and the CI fuzz job runs from the repository root

## [0.1.0] - 2026-10-04

First public testnet release.

### Added

- Milestone based escrow contract with `Emergency` and `Climate` campaign types
- Verifier registry managed by an admin
- Donations capped at the campaign goal
- Milestone release by the assigned verifier, with a proof URI emitted on chain
- Cancellation by the creator or admin
- Pro rata refunds of unreleased funds after cancellation or expiry
- Events for every state change, for indexers
- Unit tests, CI, testnet deploy script and architecture docs
