<p align="center">
  <img src="https://raw.githubusercontent.com/aidline-org/aidline-frontend/main/public/brand/aidline-wordmark.png" alt="Aidline" width="420">
</p>

# Aidline Contracts

**Live demo:** [aidline-frontend.vercel.app](https://aidline-frontend.vercel.app) · **API:** [aidline-api.onrender.com](https://aidline-api.onrender.com/stats) · Stellar testnet

Soroban smart contracts for **Aidline**, where diaspora communities fund disaster relief and climate work back home, with proof it landed.

When a flood hits home or a reforestation project needs funding, people abroad are often the first to give, and they usually have no idea where their money ends up. Aidline fixes that by holding donations in an on chain escrow and releasing them to the people doing the work one milestone at a time, only after an independent verifier confirms each milestone was delivered. If a campaign stalls, donors get their unreleased money back.

This repo is one of three:

| Repo | What it does |
| --- | --- |
| [aidline-contracts](https://github.com/aidline-org/aidline-contracts) | Escrow, milestones, verifiers and refunds (this repo) |
| [aidline-backend](https://github.com/aidline-org/aidline-backend) | Indexes contract events, stores campaign metadata and proofs, verifier registry API |
| [aidline-frontend](https://github.com/aidline-org/aidline-frontend) | Web app for donors, campaign creators and verifiers |

## How it works

```
 donor ──donate──▶ ┌──────────────────────┐
 donor ──donate──▶ │   Aidline escrow     │ ──milestone 1──▶ beneficiary
 donor ──donate──▶ │  (funds held here)   │ ──milestone 2──▶ beneficiary
                   └──────────┬───────────┘ ──milestone 3──▶ beneficiary
                              │                    ▲
                   cancelled or expired?           │ approves with proof
                              │                 verifier
                              ▼
                   donors claim pro rata refunds
```

1. The **admin** registers trusted **verifiers** (NGOs, auditors, local partners).
2. A **creator** opens a campaign: type (`Emergency` or `Climate`), beneficiary, verifier, deadline and a list of milestone amounts. The goal is the sum of the milestones.
3. **Donors** give in the campaign token (for example USDC). Funds stay in the contract.
4. The **verifier** reviews evidence and calls `approve_milestone` with a link to the proof. The next milestone is paid to the beneficiary.
5. When every milestone is paid, the campaign is `Completed`.
6. If the campaign is cancelled or passes its deadline first, each donor can call `refund` to get back their share of whatever was never released.

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for storage layout, events, and design decisions.

## Contract interface

| Function | Who can call | Description |
| --- | --- | --- |
| `__constructor(admin, token)` | deployer | Sets the admin and the token all campaigns raise in |
| `add_verifier(verifier)` | admin | Registers a verifier |
| `remove_verifier(verifier)` | admin | Unregisters a verifier and freezes their approvals |
| `set_admin(new_admin)` | admin | Transfers admin rights |
| `create_campaign(creator, beneficiary, verifier, kind, metadata_uri, deadline, milestones)` | anyone | Opens a campaign, returns its id |
| `donate(donor, campaign_id, amount)` | anyone | Sends tokens into escrow |
| `approve_milestone(campaign_id, proof_uri)` | campaign verifier | Releases the next milestone |
| `cancel_campaign(caller, campaign_id)` | creator or admin | Stops the campaign and opens refunds |
| `refund(donor, campaign_id)` | donor | Claims a pro rata refund after cancel or expiry |
| `get_campaign`, `campaign_count`, `contribution_of`, `is_verifier`, `admin`, `token` | anyone | Read only views |

Error codes are listed in [`errors.rs`](contracts/aidline/src/errors.rs).

## Getting started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (stable) with the `wasm32v1-none` target
  ```sh
  rustup target add wasm32v1-none
  ```
- [Stellar CLI](https://developers.stellar.org/docs/tools/cli/install-cli) v25.2 or newer
  ```sh
  cargo install --locked stellar-cli
  ```

### Build and test

```sh
make test     # run unit tests
make build    # build the optimized WASM with stellar contract build
make fmt      # format
make lint     # clippy with warnings as errors
```

### Deploy to testnet

```sh
# one time: create and fund a testnet identity
stellar keys generate alice --network testnet --fund

# deploy (uses native XLM as the campaign token by default)
./scripts/deploy-testnet.sh alice
```

The script prints the contract id and writes the ids to `.env.testnet`. Copy them into the backend and frontend `.env` files.

### Current testnet deployment

| | |
| --- | --- |
| Contract | [`CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK`](https://stellar.expert/explorer/testnet/contract/CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK) |
| Token | Native XLM (`CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`) |

## Project layout

```
contracts/aidline/src/
  lib.rs       contract entry points
  types.rs     Campaign, CampaignKind, CampaignStatus, storage keys
  storage.rs   storage helpers and TTL management
  events.rs    events emitted for indexers
  errors.rs    contract error codes
  test.rs      unit tests
docs/          architecture and design notes
scripts/       deployment helpers
```

## Contributing

We welcome contributions of every size, from typo fixes to new contract features. Start with [CONTRIBUTING.md](CONTRIBUTING.md) and look for issues labelled `good first issue`.

Browse open work by complexity in [ISSUES.md](ISSUES.md), including good first issues for newcomers.

## Security

This code has not been audited yet. Please do not use it with real funds on mainnet. If you find a vulnerability, follow [SECURITY.md](SECURITY.md) instead of opening a public issue.

## License

[MIT](LICENSE)
