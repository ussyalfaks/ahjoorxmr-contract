# Escrow Dispute Timeout

> **Status:** Implemented in `contracts/ahjoor-escrow/src/lib.rs`
> (`create_escrow_w_timeout`, `update_default_dispute_timeout`,
> `enforce_dispute_timeout`, `set_default_dispute_winner`,
> `get_default_dispute_winner`, `get_default_dispute_timeout`,
> `get_arbiter_timeout_count`)
> and tested in `contracts/ahjoor-escrow/src/test_dispute_timeout.rs`.

---

## Overview

When a dispute is raised in `ahjoor-escrow` an arbiter is expected to resolve it
within a configurable window. If the arbiter fails to act before the deadline
expires, **anyone** may call `enforce_dispute_timeout` to auto-resolve the
stalled dispute according to the contract's configured default winner policy.

This mechanism prevents funds from being locked indefinitely and creates an
on-chain accountability record for arbiters through a per-arbiter timeout
counter.

| Fact | Value |
| --- | --- |
| Default timeout | 7 days (`DEFAULT_DISPUTE_TIMEOUT_SECONDS = 7 * 24 * 60 * 60`) |
| Per-escrow override entry point | `create_escrow_w_timeout` |
| Global timeout entry point | `update_default_dispute_timeout` |
| Enforcement entry point | `enforce_dispute_timeout` |
| Default auto-resolve winner | `DisputeDefaultWinner::Buyer` |
| Query functions | `get_default_dispute_timeout`, `get_default_dispute_winner`, `get_arbiter_timeout_count` |

---

## Creating an Escrow with a Custom Timeout

Standard escrows use the global default timeout. To set a per-escrow override
use `create_escrow_w_timeout`:

```rust
pub fn create_escrow_w_timeout(
    env: Env,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    amount: i128,
    token: Address,
    deadline: u64,
    metadata_hash: Option<BytesN<32>>,
    sellers: Vec<(Address, u32)>,
    renewal_count: u32,
    dispute_timeout_seconds: u64,
) -> u32
```

The `dispute_timeout_seconds` parameter overrides the global default for this
escrow only. It must be greater than zero — passing `0` panics with
`DisputeTimeoutSecondsMustBePositive`.

The `renewal_count` parameter doubles as an auto-renew flag: `0` disables
auto-renew, any positive value enables it and sets `max_renewals`.

---

## Configuring the Global Default Timeout

The admin can update the global fallback timeout that applies to all escrows
created without a per-escrow override:

```rust
pub fn update_default_dispute_timeout(env: Env, admin: Address, timeout_seconds: u64)
```

**Authentication:** `admin.require_auth()` — caller must match the stored admin.

**Preconditions:**
- `timeout_seconds` must be greater than `0` (`TimeoutMustBePositive`).

```rust
pub fn get_default_dispute_timeout(env: Env) -> u64
```

Returns the current global timeout. Returns `DEFAULT_DISPUTE_TIMEOUT_SECONDS`
(7 days) if no override has been set by the admin.

### Configuring the Default Winner

When a timeout is enforced, funds are distributed according to the configured
default winner:

```rust
pub fn set_default_dispute_winner(env: Env, admin: Address, winner: DisputeDefaultWinner)
pub fn get_default_dispute_winner(env: Env) -> DisputeDefaultWinner
```

`DisputeDefaultWinner` is an enum with two variants:
- `Buyer` — disputed funds are refunded to the buyer (escrow status →
  `EscrowStatus::Refunded`). This is the default.
- `Seller` — disputed funds are released to the seller (escrow status →
  `EscrowStatus::Released`).

---

## Enforcing a Stalled Dispute

```rust
pub fn enforce_dispute_timeout(env: Env, escrow_id: u32)
```

**Authentication:** None — permissionless. Any account may call this function.

### What happens before the deadline

Calling `enforce_dispute_timeout` when the timeout has **not yet elapsed**
panics with `DisputeTimeoutDeadlineHasNotPassedYet`. The timeout clock starts
when `dispute_escrow` is called, not when the escrow is created.

### What happens after the deadline

Once `env.ledger().timestamp() >= dispute_raised_at + effective_timeout`:

1. The escrow's dispute record is marked as `resolved = true`.
2. Funds are disbursed according to the default winner setting:
   - `Buyer` → funds transferred back to buyer; status set to `Refunded`.
   - `Seller` → funds transferred to seller; status set to `Released`.
3. The arbiter's timeout counter is incremented by 1 and stored under
   `DataKey::ArbiterTimeoutCount(arbiter)`.
4. A `DisputeEscalated` event is emitted.

### Effective timeout resolution

The effective timeout for an escrow is resolved in this order:
1. Per-escrow override (`escrow.extensions.dispute_timeout_seconds`) if set.
2. Global default (`DataKey::DefaultDisputeTimeout`) if set by admin.
3. Hardcoded constant `DEFAULT_DISPUTE_TIMEOUT_SECONDS` (7 days).

### Error cases

| Condition | Error |
| --- | --- |
| Escrow is not in a disputed state | `EscrowIsNotDisputed` |
| Dispute is already resolved | `DisputeAlreadyResolved` |
| Deadline has not passed yet | `DisputeTimeoutDeadlineHasNotPassedYet` |

---

## Arbiter Accountability

Every time a timeout is enforced against an arbiter, their counter increments:

```rust
pub fn get_arbiter_timeout_count(env: Env, arbiter: Address) -> u32
```

Returns `0` for arbiters with no recorded timeouts. This counter is permanent
and can be queried by anyone to evaluate arbiter reliability before using them
in future escrows.

---

## Partial Disputes

`dispute_escrow` supports partial disputes where only a portion of the escrow
amount is contested. When a partial dispute times out, only the disputed portion
is subject to the default winner rule; the non-disputed portion was already
released to the seller at the time of the partial dispute.

---

## Flow Diagram

```text
dispute_escrow(buyer/seller, escrow_id, reason, disputed_amount)
       │
       │  stores dispute_raised_at = ledger timestamp
       ▼
 EscrowStatus::Disputed  (or PartiallyDisputed for partial amount)
       │
       ├──► [arbiter resolves within deadline]
       │        └── resolve_dispute(arbiter, escrow_id, seller_bps)
       │                 arbiter_timeout_count unchanged
       │
       └──► [deadline passes — no arbiter action]
                enforce_dispute_timeout(escrow_id)  ← anyone can call
                    │
                    ├── default_winner == Buyer  → Refunded + funds → buyer
                    └── default_winner == Seller → Released + funds → seller
                              arbiter_timeout_count += 1
```

---

## Events

| Event | Emitted By | Payload | Description |
| --- | --- | --- | --- |
| `DisputeEscalated` | `enforce_dispute_timeout` | `(escrow_id, effective_timeout)` | Emitted when a stalled dispute is auto-resolved after the deadline. |

---

## Error Codes

| Code | Name | Description |
| --- | --- | --- |
| 29 | `DisputeTimeoutDeadlineHasNotPassedYet` | Timeout called before deadline elapsed. |
| 31 | `DisputeTimeoutSecondsMustBePositive` | `create_escrow_w_timeout` passed `0` for timeout. |
| — | `EscrowIsNotDisputed` | `enforce_dispute_timeout` called on a non-disputed escrow. |
| — | `DisputeAlreadyResolved` | `enforce_dispute_timeout` called on an already-resolved dispute. |
| — | `TimeoutMustBePositive` | `update_default_dispute_timeout` passed `0`. |

---

## Function Reference

| Function | Caller | Purpose |
| --- | --- | --- |
| `create_escrow_w_timeout(...)` | Buyer | Creates an escrow with a per-escrow dispute timeout override. |
| `update_default_dispute_timeout(admin, timeout_seconds)` | Admin | Sets the global fallback dispute timeout. |
| `get_default_dispute_timeout()` | Anyone | Reads the current global timeout (seconds). |
| `set_default_dispute_winner(admin, winner)` | Admin | Configures whether Buyer or Seller wins a timed-out dispute. |
| `get_default_dispute_winner()` | Anyone | Reads the current default winner setting. |
| `enforce_dispute_timeout(escrow_id)` | Anyone | Permissionlessly auto-resolves a stalled dispute after the deadline. |
| `get_arbiter_timeout_count(arbiter)` | Anyone | Returns the number of disputes an arbiter has let time out. |

---

## Storage Reference

| Key | Storage Type | Data Stored |
| --- | --- | --- |
| `DataKey::DefaultDisputeTimeout` | Instance | `u64` global timeout in seconds. |
| `DataKey::DefaultDisputeWinner` | Instance | `DisputeDefaultWinner` enum value. |
| `DataKey::ArbiterTimeoutCount(arbiter)` | Persistent | `u32` cumulative timeout count for the arbiter. |
| `DataKey::Escrow(escrow_id).extensions.dispute_timeout_seconds` | Persistent | `Option<u64>` per-escrow override, set by `create_escrow_w_timeout`. |
