# Escrow Dispute Flow

This document describes the multi-step dispute flow used by the ahjoor-escrow contract.

Summary

- Default timeout: 7 days (604,800 seconds).
- Per-escrow override: `create_escrow_w_timeout( ... , timeout_seconds )`.
- Status transitions: `Active` → `Disputed` → `Resolved` / `TimedOut`.

Step-by-step flow

1. Buyer calls `dispute_escrow(escrow_id)`
   - Marks the escrow as `Disputed` and records a dispute deadline: `now + escrow.timeout_seconds` (or default 604,800s).
   - Starts the dispute timer; arbiter must be assigned and act before the deadline.

2. Admin assigns an arbiter via `assign_arbiter(escrow_id, arbiter)`
   - Sets the `arbiter` for the escrow. The arbiter is responsible for resolving the dispute.

3. Arbiter calls `resolve_dispute(escrow_id, winner)` before the deadline
   - If the arbiter resolves in time, the contract transfers funds to the declared `winner` and sets status `Resolved`.

4. If arbiter misses deadline → anyone calls `enforce_dispute_timeout(escrow_id)`
   - If current time > dispute deadline and escrow still `Disputed`, this call:
     - Releases funds to the `default winner` (as defined by contract logic or dispute invocation parameters).
     - Marks escrow as `TimedOut` (a terminal state similar to `Resolved`).
     - Increments the arbiter's timeout counter (used for tracking arbiter inactivity/misbehavior).

Notes and implementation details

- Default Timeout: The contract uses a default of 604_800 seconds (7 days). When `dispute_escrow` is called and no per-escrow timeout is set, the dispute deadline is computed using that default.
- Per-escrow Override: Use `create_escrow_w_timeout(...)` to create an escrow whose `timeout_seconds` differs from the default. This value is persisted on the escrow and used for its dispute deadline.
- Status Transitions:
  - `Active` — normal escrow life before dispute.
  - `Disputed` — raised by the buyer calling `dispute_escrow`; a deadline is set.
  - `Resolved` — arbiter resolved the dispute before the deadline and funds were distributed accordingly.
  - `TimedOut` — arbiter failed to resolve before deadline; `enforce_dispute_timeout` was called and funds were distributed to the default winner; arbiter timeout counter incremented.

- Arbiter timeout counter: Whenever a dispute is forced via `enforce_dispute_timeout` because the arbiter missed the deadline, the contract increments a per-arbiter counter. This counter can be used by off-chain governance or admin logic to suspend or penalize repeatedly inactive arbiters.

## Timeout Resolution & Fund Distribution Mechanism

When an arbiter fails to resolve a dispute within the allocated deadline, the contract provides an automatic resolution mechanism via `enforce_dispute_timeout(escrow_id)`.

### 1. Timeout Deadline Calculation
- **Deadline Start**: Recorded in persistent storage (`DataKey::DisputeDeadlineStart(escrow_id)`) as `ledger.timestamp()` when `dispute_escrow` is called.
- **Effective Timeout Duration**:
  - **Per-escrow override**: Specified via `create_escrow_w_timeout(...)` or `dispute_timeout_seconds` in `EscrowExtensions`.
  - **Global default**: Configured via `update_default_dispute_timeout(admin, timeout_seconds)` (queried via `get_default_dispute_timeout()`), defaulting to 7 days (`604,800` seconds).
- **Expiration Threshold**: Enforceable when `current_ledger_timestamp - deadline_start >= effective_timeout`.

### 2. Default Winner Determination
- **Global Configuration**: Admin configures global default via `set_default_dispute_winner(admin, winner)` and queries via `get_default_dispute_winner()`. Values are `DisputeDefaultWinner::Buyer` (`0`) or `DisputeDefaultWinner::Seller` (`1`). Defaults to `Buyer`.
- **Per-escrow Override**: Configured at escrow creation via `dispute_default_winner` in `EscrowCreateRequest` or stored in `escrow.extensions.dispute_default_winner`.

### 3. Fund Distribution & State Execution
- **Buyer Default Winner (`DisputeDefaultWinner::Buyer`)**:
  - The remaining disputed escrow amount is refunded to the buyer(s) via `Self::transfer_to_buyers(...)`.
  - Escrow status transitions to `EscrowStatus::Refunded`.
- **Seller Default Winner (`DisputeDefaultWinner::Seller`)**:
  - The remaining disputed escrow amount is released to the seller via token transfer.
  - Escrow status transitions to `EscrowStatus::Released`.
- **Partial Dispute Timeout Handling**:
  - For partial disputes (`dispute_amount < total_amount`), the undisputed portion was already transferred to the seller immediately when `dispute_escrow` was called.
  - On timeout enforcement, only the locked disputed portion is transferred to the default winner, and the escrow status transitions from `PartiallyDisputed` to `Refunded` (or `Released`).
- **Receipt NFT Burning**: If an NFT receipt was minted for the escrow, it is burned via `burn_receipt_if_exists`.
- **Dispute Settlement**: The dispute record (`DataKey::Dispute(escrow_id)`) is updated with `resolved = true`.
- **Arbiter Reputation Penalty**: The assigned arbiter's penalty counter (`DataKey::ArbiterTimeoutCount(escrow.arbiter)`) is incremented by `1`.
- **Events Emitted**:
  - `DisputeTimedOut(escrow_id, arbiter, default_winner, elapsed_seconds)`
  - `ArbitersTimeoutPenaltyApplied(arbiter, new_timeout_count)`

## Timeout Enforcement Caller & Execution Requirements

- **Permissionless Trigger**: `enforce_dispute_timeout(escrow_id)` is a **public, unauthenticated** function (`pub fn`). Any account—including the buyer, seller, admin, or an automated off-chain bot/keeper—can trigger timeout enforcement once the deadline has passed.
- **No Authentication Required**: The function does not call `require_auth()` for any role.
- **Enforcement Validation Checks**:
  1. **Pause Check**: Contract must not be paused (`require_not_paused`).
  2. **Escrow Existence**: Escrow record must exist (`DataKey::Escrow(escrow_id)`).
  3. **Active Dispute**: A dispute record must exist (`DataKey::Dispute(escrow_id)`) and `dispute.resolved` must be `false`.
  4. **Valid Status**: Escrow status must be `EscrowStatus::Disputed` or `EscrowStatus::PartiallyDisputed`. Calls on `Active` or terminal states (`Resolved`, `Released`, `Refunded`) will panic with `"Escrow is not disputed"`.
  5. **Deadline Expiration**: Ledger timestamp check requires `current_timestamp - deadline_start >= effective_timeout`. Invocations prior to deadline expiry revert with `"Dispute timeout deadline has not passed yet"`.

## Edge Cases & Auto-Resolution Special Behavior

### 1. Single-Dispute Lifecycle & Prevention of Repeated Disputes
- **Strict State Machine**: Calling `dispute_escrow` requires that the escrow be in an open state (`is_open_escrow_status`).
- **Terminal Transitions**: Resolution by an arbiter (`resolve_dispute`) or auto-resolution via timeout (`enforce_dispute_timeout`) moves the escrow into terminal statuses (`Resolved`, `Released`, or `Refunded`).
- **No Repeated Disputes**: Once an escrow reaches a terminal state, further calls to `dispute_escrow` revert with `"Escrow is not active"`. An escrow can only be disputed once in its lifecycle.
- **Partial Dispute Edge Case**: When a partial dispute is raised, the undisputed portion is released to the seller immediately, leaving only the disputed portion in escrow under `EscrowStatus::PartiallyDisputed`. When this dispute resolves or times out, the disputed portion is transferred to the winner, transitioning the escrow to a terminal state (`Refunded` or `Released`). Neither party can initiate a second dispute for either the released or refunded funds.

### 2. Arbiter Reassignment & Pool Removal Near Deadline
- **Timer Continuity**: When an arbiter is removed from the pool via `remove_arbiter(admin, arbiter, escrow_ids)`, active escrows using that arbiter are flagged with `DataKey::ArbiterNeedsReplacement(escrow_id)`. **Removing or reassigning an arbiter does NOT pause, reset, or extend `DataKey::DisputeDeadlineStart(escrow_id)`**.
- **Deadline Strictness**: The dispute deadline timer runs uninterrupted from the original `dispute_escrow` timestamp. If an arbiter is reassigned near the deadline, the newly assigned arbiter must issue a ruling before the original deadline expires.
- **Enforcement Window**: If the deadline passes without resolution—even if arbiter reassignment was requested or pending—`enforce_dispute_timeout(escrow_id)` can be invoked by anyone to execute default fund distribution.
- **Penalty Attribution**: When `enforce_dispute_timeout` is executed, the timeout penalty (`ArbiterTimeoutCount`) is credited against the address recorded in `escrow.arbiter` at the time of enforcement.

Examples

- Typical flow with default timeout:
  1. Buyer calls `dispute_escrow(escrow_id)` → `dispute_deadline = now + 604800`.
  2. Admin calls `assign_arbiter(escrow_id, arbiter_addr)`.
  3. If arbiter calls `resolve_dispute(escrow_id, buyer)` before `dispute_deadline`, escrow becomes `Resolved` and funds go to `buyer`.
  4. If arbiter does not act and `dispute_deadline` passes, any caller can call `enforce_dispute_timeout(escrow_id)` to release funds and increment arbiter timeout counter.

Make sure to consider these points when integrating with frontends or off-chain tooling:

- Show a clear dispute state and a countdown until `dispute_deadline` to users.
- Surface arbiter identity and a link to arbiter reputation or timeout count.
- Allow admins to create escrows with shorter/longer `timeout_seconds` via `create_escrow_w_timeout` for special cases.

If you need this doc expanded with code snippets from the contract (field names, exact method signatures), tell me which file you'd like referenced and I will extract and include the precise signatures.

## Cooling-Off Period

After an arbiter resolves a dispute via `resolve_dispute`, the escrow may enter a **Cooling-Off Period** rather than immediately releasing funds. This gives the losing party a window to flag potential resolution errors.

### Configuration
- **Default Duration**: The default cooling-off period is `0` seconds (immediate execution/release).
- **Custom Duration**: An admin can configure the cooling-off duration globally for the contract by calling `set_resolution_cooloff_secs(admin, cooling_off_secs)`.

### Behavior & Restrictions
- **Entering Cooling-Off**: When `resolve_dispute` is called and a cooling-off period > 0 is set, the escrow status changes to `CoolingOff`. Funds are **not** moved yet.
- **Flagging Errors**: During the cooling-off window, the losing party can call `flag_resolution_error(party, escrow_id, reason_hash)`. Attempting to flag after the window expires is rejected.
- **Blocked Actions**: Calling `finalize_resolution` is blocked if the cooling-off window has not elapsed, or if a flag has been raised and not yet cleared by an admin.
- **Admin Review**: If a resolution is flagged, an admin must review and clear the flag using `clear_resolution_flag(admin, escrow_id)` before the resolution can be finalized.

### Finalization
Once the cooling-off window has expired (and assuming no pending flags remain), anyone can call `finalize_resolution(escrow_id)`. This action executes the actual fund transfer according to the arbiter's decision and updates the escrow status to a terminal state (e.g., `Refunded` or `Released`).
