# Contributing to Aidline Contracts

Thanks for helping build transparent funding for disaster relief and climate action. This guide gets you from zero to a merged pull request.

## Finding something to work on

- Issues labelled `good first issue` are small and well scoped. Great for your first contribution.
- Every issue lists acceptance criteria and the files you will likely touch. If anything is unclear, ask in the issue before starting.
- Comment on an issue to get it assigned before you begin, so two people do not build the same thing.

## Working on a Stellar Wave issue

Issues labelled `Stellar Wave` are part of the [Stellar Wave program on Drips](https://docs.drips.network/wave/). Contributors earn points for resolved issues and share the Wave reward pool in proportion to their points.

| Complexity | Label                | Points |
| ---------- | -------------------- | ------ |
| Trivial    | `complexity: trivial` | 100    |
| Medium     | `complexity: medium`  | 150    |
| High       | `complexity: high`    | 200    |

How it works:

1. Pick an open issue and comment to ask for it. A maintainer assigns it to you. Please take one issue at a time and check nobody else is already assigned.
2. Read the acceptance criteria. Every box needs to be true for the issue to count as resolved.
3. Open a pull request that links the issue with `Closes #123`. Keep it focused on that one issue.
4. Respond to review comments. A maintainer merges the pull request and marks the issue resolved, which awards the points.

Quality matters more than speed. Pull requests are reviewed for understanding, not just for passing checks:

- Write and run the tests for your change. Do not submit code you have not run or cannot explain.
- Do not open pull requests that only fix typos or reformat files to collect points.
- If you use an AI assistant, you are still responsible for every line. Pull requests that look untested or not understood will be closed.
- Do not claim several issues at once or comment on assigned issues asking to be reassigned.

See the Drips [Terms and rules](https://docs.drips.network/wave/terms-and-rules/) for the full conduct rules.

## Local setup

```sh
git clone https://github.com/aidline-org/aidline-contracts
cd aidline-contracts
rustup target add wasm32v1-none
cargo install --locked stellar-cli
make test
```

If `make test` passes, you are ready.

## Making a change

1. Fork the repo and create a branch: `git checkout -b feat/short-description`
2. Make your change. Keep pull requests focused on one issue.
3. Add or update tests in `contracts/aidline/src/test.rs`. Contract changes without tests will not be merged.
4. Run the same checks CI runs:
   ```sh
   make fmt
   make lint
   make test
   make build
   ```
5. If you changed behavior, update `README.md` or `docs/ARCHITECTURE.md`.
6. Open a pull request and fill in the template. Link the issue with `Closes #123`.

## Code guidelines

- Return a typed `Error` instead of panicking for anything a caller can trigger.
- Every function that moves funds or changes state must call `require_auth` on the right address.
- Emit an event for every state change the frontend or indexer might care about.
- Prefer clear code over clever code. Reviewers need to be able to reason about where money goes.
- Comments explain why, not what.

## Commit messages

Use a short prefix: `feat:`, `fix:`, `test:`, `docs:`, `refactor:`, `chore:`. Example: `feat: add verifier reassignment`.

## Getting help

Open a discussion or ask in the issue thread. No question is too small.
