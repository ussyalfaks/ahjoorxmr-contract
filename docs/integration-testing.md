# Integration Test Suite

This document covers the cross-contract integration tests under
`contracts/integration-tests/`. These tests live in a dedicated crate, separate
from the per-contract unit tests, so they can exercise multi-contract flows
end-to-end without reaching into private contract internals.

> **Note:** This document covers *documenting* the existing suite. Issue #458
> is the related ticket for *expanding* the integration test coverage itself;
> new scenarios added there should follow the authoring guide in
> [§3 Adding a new scenario](#3-adding-a-new-integration-test-scenario).

---

## Table of Contents

- [1. What the suite covers today](#1-what-the-suite-covers-today)
- [2. Running the integration test suite locally](#2-running-the-integration-test-suite-locally)
- [3. Adding a new integration test scenario](#3-adding-a-new-integration-test-scenario)
- [4. Layout & harness at a glance](#4-layout--harness-at-a-glance)

---

## 1. What the suite covers today

The single existing scenario file,
[`contracts/integration-tests/src/rosca_payout_flow.rs`](../contracts/integration-tests/src/rosca_payout_flow.rs),
exercises a full ROSCA payout flow end-to-end against the deployed
[`ahjoor-rosca`](../contracts/ahjoor-rosca) contract plus a Stellar Asset
Contract token used as the group currency. It is intentionally written against
the **public** `ahjoor-rosca` API (`contribute`, `finalize_round`, `get_state`,
`get_member_status`, `get_round_history`, ...) so the suite stays valid across
internal refactors and tracks real user-visible behaviour rather than module
layout.

### `test_rosca_round_completes_payout_on_full_contribution`

Verifies the *happy path* of a ROSCA round:

1. Initialises a 4-member ROSCA group with `PayoutStrategy::RoundRobin`, a
   1-day (`86_400` second) round duration, and zero fees/penalties.
2. Every member calls `contribute` for their full share.
3. Asserts that:
   - `CurrentRound` advances from `0` to `1` (the final `contribute` call
     triggers `complete_round_payout` internally).
   - `PaidMembers` is empty for the new round.
   - The expected recipient's on-chain token balance nets the full pot minus
     their own contribution (they also contribute that round).
   - `get_round_history()` records exactly one `RoundHistory` entry for round
     `0` with the correct `recipient` and `amount` (`contribution * members`).

This implicitly exercises `contribute` → state update → automatic payout
trigger inside `internals::complete_round_payout` without the test ever naming
that internal function.

### `test_rosca_finalize_round_flags_defaulter_and_pays_partial_pot`

Verifies the *partial contribution + missed deadline* path:

1. Initialises a 3-member ROSCA group with the same configuration as above.
2. Only the first two members contribute; the third defaults.
3. Advances the ledger past the round deadline.
4. Calls admin `finalize_round`.
5. Asserts that:
   - The defaulter's `MemberStatus` shows `has_paid_this_round == false` and
     `default_count == 1`.
   - `get_round_history()` still records a completed payout for round `0`
     (the partial pot is still paid out before suspensions are applied).
   - `CurrentRound` advances to `1`.

Together, the two tests cover the two main round-completion triggers:
auto-completion on full in-round contribution and admin-driven completion
after a missed deadline with a partial pot.

---

## 2. Running the integration test suite locally

### Prerequisites

You need a working Rust toolchain with the Soroban target installed. These are
the same prerequisites as the rest of the workspace — see
[CONTRIBUTING.md — Prerequisites](../CONTRIBUTING.md#prerequisites).

```bash
# 1. Install Rust (stable)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Add the Soroban/WASM target used by all Ahjoor contracts
rustup target add wasm32-unknown-unknown
```

No additional services or network access are required — integration tests run
entirely inside the Soroban in-process test environment (`Env::default()`),
which simulates ledger state, auth, and token contracts in memory.

### Run only the integration tests

The integration suite ships as a standalone workspace member at
`contracts/integration-tests/`. Run **only** that crate to keep iteration
fast and the output focused:

```bash
cargo test --manifest-path contracts/integration-tests/Cargo.toml
```

You should see both existing test cases pass:

```text
running 2 tests
test rosca_payout_flow::test_rosca_round_completes_payout_on_full_contribution ... ok
test rosca_payout_flow::test_rosca_finalize_round_flags_defaulter_and_pays_partial_pot ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Run a single scenario

To iterate on one case without running the other, pass the test name as a
filter to `cargo test`:

```bash
cargo test \
  --manifest-path contracts/integration-tests/Cargo.toml \
  test_rosca_round_completes_payout_on_full_contribution
```

### Run with output

When a test fails and you want to see `println!` output or assertion context,
add `-- --nocapture`:

```bash
cargo test \
  --manifest-path contracts/integration-tests/Cargo.toml \
  -- --nocapture
```

> Tip: the crate is configured with `publish = false` and pulls in
> `soroban-sdk` with the `testutils` feature only as a **dev-dependency**, so
> building it for release/WASM (`wasm32-unknown-unknown`) is intentionally
> unnecessary and `cargo build --release` on it will produce no WASM artifact.

---

## 3. Adding a new integration test scenario

The integration crate is intentionally light-weight; new scenarios should
mirror the existing structure so they stay easy to read and review.

### Step 1 — confirm the scenario fits "integration"

The integration crate exists to test behaviour that requires **more than one
contract** wired together, or end-to-end user flows that the per-contract
unit tests don't have the wiring to set up. If your scenario only exercises a
single `ahjoor-*` contract in isolation, add it under that contract's own
`src/test_*.rs` module instead and re-export it from the crate's `test`
module. Use the integration crate when:

- You need to deploy two or more Ahjoor contracts and let them call each
  other.
- You want to assert an observable user outcome (final balances, emitted
  events, recorded history) rather than internal storage values.

### Step 2 — add a new file in `contracts/integration-tests/src/`

Each scenario lives in its own `*.rs` file so it can be reviewed and
navigated independently. Use a snake_case name describing the user flow, for
example `multi_group_payout_flow.rs` or `escrow_dispute_lifecycle.rs`.

Then register the new module in
[`contracts/integration-tests/src/lib.rs`](../contracts/integration-tests/src/lib.rs):

```rust
#![cfg(test)]
#![no_std]

extern crate std;

mod rosca_payout_flow;
mod my_new_scenario; // ← add this line
```

### Step 3 — reuse the existing harness pattern

Follow the structure of `rosca_payout_flow.rs`:

1. **Use `Env::default()` + `mock_all_auths()`** so you don't need real keypair
   auth in tests.
2. **Set a deterministic ledger** via `env.ledger().set(LedgerInfo { ... })`
   with a fixed timestamp and `protocol_version`. This keeps test output
   stable across machines and SDK upgrades.
3. **Deploy the contracts you need** with `env.register_stellar_asset_contract_v2`
   for tokens and `env.register(MyContract, ())` for Ahjoor contracts. For
   Ahjoor contracts, wire the corresponding `*Client` to drive interactions.
4. **Build a `TestEnvironment` (or similarly named) helper struct** that
   encapsulates env + clients + funded member accounts. Each test should
   start from a clean, fully-funded state — fund members via the token admin
   client at ~10× the contribution amount so balance issues never mask the
   behaviour under test.
5. **Drive the scenario through public entrypoints only.** Don't reach into
   `ahjoor-rosca`'s `internals` module or any other crate's private modules
   — the point of the integration suite is to validate observable behaviour.
6. **Assert on observable outcomes**: token balances via `token::Client`,
   `get_state()` / `get_member_status()` / `get_round_history()` from the
   relevant Ahjoor client, and emitted events where relevant.

### Step 4 — name tests descriptively

Use a `test_<scenario>_<expected_outcome>` naming convention so failures are
self-explanatory in CI logs, e.g.:

- `test_rosca_round_completes_payout_on_full_contribution` ✅
- `test_rosca_finalize_round_flags_defaulter_and_pays_partial_pot` ✅
- `test_<scenario>_succeeds_when_<preconditions>`
- `test_<scenario>_reverts_with_<error_name>_when_<preconditions>`

Document each test's purpose in a `///` doc comment immediately above the
`#[test]` attribute — the file-level `TestEnvironment` doc is not enough.

### Step 5 — document the scenario in this file

After landing a new scenario file:

1. Update [§1 What the suite covers today](#1-what-the-suite-covers-today)
   with a subsection for each new `#[test]`, following the same structure
   (what it sets up, what it drives, what it asserts, and which
   public/internal paths that exercise).
2. Make sure the new scenario's expected output appears in the snippet in
   [§2 Running the integration test suite locally](#2-running-the-integration-test-suite-locally)
   so readers know a successful run looks like.
3. Reference any related issue (e.g. `#458` for the integration test
   coverage expansion ticket) so reviewers can trace the change back to
   intent.

### Step 6 — run locally before opening a PR

Before pushing, verify the new scenario:

```bash
# Format
cargo fmt --all

# Lint (warnings are errors in CI)
RUSTFLAGS="-Dwarnings" cargo clippy \
  --manifest-path contracts/integration-tests/Cargo.toml \
  --all-targets --all-features

# Run the integration suite (will include your new test)
cargo test --manifest-path contracts/integration-tests/Cargo.toml
```

All three steps must pass locally; CI runs them on every PR.

> Note: `--all-features` is only useful for crates whose features change
> production behaviour. `contracts/integration-tests/Cargo.toml` only defines
> the `testutils` feature and gates it behind a dev-dependency, so
> `cargo clippy --all-targets --all-features` works but produces no extra
> signals beyond `--all-targets`. Prefer `--all-targets` alone for this crate
> unless you are intentionally validating feature-gated code paths.

---

## 4. Layout & harness at a glance

```
contracts/integration-tests/
├── Cargo.toml          # crate-type = ["lib"], publish = false;
│                       # depends on ahjoor-rosca + soroban-sdk[testutils]
└── src/
    ├── lib.rs          # #![cfg(test)] entrypoint; registers each scenario
    │                   #   module here (`mod rosca_payout_flow;` etc.)
    └── rosca_payout_flow.rs   # End-to-end ROSCA payout scenarios.
                                # Owns the reusable `TestEnvironment` struct
                                # that deploys the ROSCA contract + a mock
                                # Stellar Asset Contract token and funds
                                # the configured member accounts.
```

- `crate-type = ["lib"]` keeps the crate non-binary; tests run as
  `cargo test`'s default `lib` test target.
- `#![cfg(test)]` and `#![no_std] \nextern crate std;` mirror the rest of
  the Ahjoor workspace — production code stays `no_std`, while tests opt
  into `std` for ergonomics.
- `publish = false` in `Cargo.toml` prevents this crate from being
  released to crates.io; it is workspace-internal tooling only.
