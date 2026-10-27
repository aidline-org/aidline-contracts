# Deployment

How to build and deploy the Aidline contract. Everything here targets **testnet**. Do not deploy to mainnet before an external audit. See [THREAT_MODEL.md](THREAT_MODEL.md).

## Prerequisites

- Rust with the `wasm32v1-none` target: `rustup target add wasm32v1-none`
- The [Stellar CLI](https://developers.stellar.org/docs/tools/cli) (CI uses version 28.1.0)
- A funded testnet identity:

  ```sh
  stellar keys generate deployer --network testnet --fund
  ```

## Build and test

```sh
make fmt     # format
make lint    # clippy with warnings as errors
make test    # unit tests
make build   # builds target/wasm32v1-none/release/aidline.wasm
```

These are the same checks CI runs.

## Deploy to testnet

```sh
./scripts/deploy-testnet.sh deployer
```

The script:

1. Builds the contract.
2. Deploys it with the identity as **admin** and native XLM as the token. Pass a token contract id as the second argument to use another token.
3. Registers the admin as the first verifier so you can test the whole flow.
4. Writes the result to `.env.testnet`.

`.env.testnet` looks like this:

```
AIDLINE_CONTRACT_ID=C...
AIDLINE_TOKEN_ID=C...
AIDLINE_ADMIN=G...
AIDLINE_NETWORK=testnet
```

Copy `AIDLINE_CONTRACT_ID` and `AIDLINE_TOKEN_ID` into the [backend](https://github.com/aidline-org/aidline-backend) `.env` and the [frontend](https://github.com/aidline-org/aidline-frontend) `.env.local`. The backend should also get `INDEXER_START_LEDGER` set to the deployment ledger.

## Current testnet deployment

|          |                                                                                                                             |
| -------- | --------------------------------------------------------------------------------------------------------------------------- |
| Contract | `CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK`                                                                  |
| Token    | Native XLM (`CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC`)                                                     |
| Explorer | [stellar.expert](https://stellar.expert/explorer/testnet/contract/CALNXTTPKPTCCWSTN3NHQNZHPXM2IXQZQMFBCCK7LI3N5FRB6DB5NLHK) |

## Try it from the command line

```sh
# Is an address a verifier?
stellar contract invoke --id $AIDLINE_CONTRACT_ID --network testnet --source deployer \
  -- is_verifier --who $(stellar keys address deployer)

# How many campaigns exist?
stellar contract invoke --id $AIDLINE_CONTRACT_ID --network testnet --source deployer \
  -- campaign_count
```

## Keys and secrets

- The deployer identity becomes the admin. Treat its key as the most sensitive thing in the project.
- Never commit secret keys. `.env.testnet` holds only public values.
- For mainnet, use a hardware wallet or a multisig account as admin.

## Checklist before a mainnet deploy

- [ ] External audit completed and findings resolved
- [ ] Open items in [THREAT_MODEL.md](THREAT_MODEL.md) closed or accepted in writing
- [ ] Admin is a multisig, not a single key
- [ ] Token contract verified (native XLM or a well known issuer)
- [ ] Resource costs regenerated (see [ARCHITECTURE.md](ARCHITECTURE.md))
- [ ] Deployed WASM hash recorded and matches a tagged release
- [ ] Backend and frontend point at the new contract id
