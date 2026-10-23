# Escrow Time-Based Release Modes

> **Status:** Implemented in `contracts/ahjoor-escrow/src/lib.rs`
> (`create_timelocked_escrow`, `claim_timelocked`, `cancel_timelocked`,
> `create_escrow_with_inactivity`, `claim_inactivity_release`,
> `create_escrow_with_schedule`, `claim_scheduled_release`, `get_release_schedule`).

---

## Overview

`ahjoor-escrow` provides three distinct time-based release mechanisms that determine when and how
funds are released to the seller/beneficiary. Each mode serves different use cases:

| Mode | Trigger | Best For |
| --- | --- | --- |
| **Time-Locked** | Fixed timestamp (`unlock_at`) | Simple hold until a specific date |
| **Inactivity Release** | Buyer inactivity for X seconds | Auto-release if buyer disappears |
| **Scheduled (Tranches)** | Multiple timestamps | Milestone-based progressive release |

---

## Comparison Table

| Feature | Time-Locked | Inactivity | Scheduled |
| --- | --- | --- | --- |
| **Creation Function** | `create_timelocked_escrow` | `create_escrow_with_inactivity` | `create_escrow_with_schedule` |
| **Configuration** | `unlock_at: u64` (timestamp) | `buyer_inactivity_secs: u64` | `Vec<ReleaseTranche>` |
| **Who Can Claim** | Beneficiary only | Seller only | Beneficiary only |
| **Release Count** | Single lump-sum | Single lump-sum | Multiple (staged) |
| **Cancel Window** | Before `unlock_at` | Anytime if buyer inactive | Anytime before tranches |
| **Use Case** | Escrow hold | Buyer no-show protection | Milestone payments |

---

## Mode 1: Time-Locked Escrow

A simple mechanism where funds are locked until a specific timestamp, after which the beneficiary
can claim them.

### Creation — `create_timelocked_escrow`

```rust
pub fn create_timelocked_escrow(
    env: Env,
    buyer: Address,
    arbiter: Address,
    amount: i128,
    token: Address,
    deadline: u64,
    unlock_at: u64,           // Unix timestamp when funds become available
    beneficiary: Address,
    metadata_hash: Option<BytesN<32>>,
) -> u32
```

**Parameters:**
- `unlock_at`: Unix timestamp (seconds since epoch) when funds become claimable

**Validation:**
- `unlock_at` must be in the future (at creation time)
- `unlock_at` is stored in both `EscrowExtensions.min_lock_until` and separate `TimeLockData`

### Claim — `claim_timelocked`

```rust
pub fn claim_timelocked(env: Env, beneficiary: Address, escrow_id: u32)
```

**Who can call:** Only the `beneficiary` (seller)

**When:** After `env.ledger().timestamp() >= lock_data.unlock_at`

**Behaviour:**
- Transfers the entire escrow amount to the beneficiary
- Marks the escrow as claimed

### Cancel — `cancel_timelocked`

```rust
pub fn cancel_timelocked(env: Env, buyer: Address, escrow_id: u32)
```

**Who can call:** Only the buyer

**When:** Before `unlock_at` has passed

After `unlock_at`, the buyer cannot cancel — they must wait for the beneficiary to claim.

### Query — `get_timelock_data`

```rust
pub fn get_timelock_data(env: Env, escrow_id: u32) -> Option<TimeLockData>
```

Returns:
```rust
pub struct TimeLockData {
    pub unlock_at: u64,      // Unix timestamp when funds unlock
    pub beneficiary: Address,
    pub claimed: bool,
}
```

---

## Mode 2: Inactivity Release

Funds are released to the seller if the buyer becomes inactive (stops taking actions) for a
configurable period. This protects sellers from buyers who abandon transactions.

### Creation — `create_escrow_with_inactivity`

```rust
pub fn create_escrow_with_inactivity(
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
    buyer_inactivity_secs: u64,  // Seconds of inactivity before seller can claim
) -> u32
```

**Parameters:**
- `buyer_inactivity_secs`: Seconds of inactivity before the seller can claim. If `0`, the feature
  is disabled.

**Inactivity Tracking:**
- On escrow creation, `LastBuyerAction` is initialized to the creation timestamp
- Any buyer action (top-up, acknowledgment, etc.) resets the timer via `update_last_buyer_action`

### Claim — `claim_inactivity_release`

```rust
pub fn claim_inactivity_release(env: Env, seller: Address, escrow_id: u32)
```

**Who can call:** Only the seller

**When:** After `now - last_action >= buyer_inactivity_secs`

**Validation:**
- Requires `buyer_inactivity_secs > 0` (feature enabled)
- Requires the inactivity period to have elapsed

### Data Storage

| Key | Description |
| --- | --- |
| `EscrowExtensions.buyer_inactivity_secs` | Configured inactivity timeout |
| `LastBuyerAction(escrow_id)` | Timestamp of the last buyer action |

---

## Mode 3: Scheduled Release (Tranches)

Funds are released in multiple tranches at specified timestamps. This enables milestone-based
payments where the seller receives funds incrementally as work progresses.

### Creation — `create_escrow_with_schedule`

```rust
pub fn create_escrow_with_schedule(
    env: Env,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    token: Address,
    deadline: u64,
    schedule: Vec<ReleaseTranche>,
) -> u32
```

**Parameters:**
- `schedule`: Vector of `ReleaseTranche` objects

```rust
pub struct ReleaseTranche {
    pub unlock_at: u64,   // Unix timestamp when this tranche unlocks
    pub amount: i128,     // Token amount for this tranche
    pub claimed: bool,    // Whether already claimed
}
```

**Validation:**
- Each tranche's `unlock_at` must be in the future
- Each tranche's `amount` must be positive
- Sum of tranche amounts must equal the escrow amount

### Claim — `claim_scheduled_release`

```rust
pub fn claim_scheduled_release(env: Env, beneficiary: Address, escrow_id: u32)
```

**Who can call:** Only the beneficiary/seller

**When:** After each tranche's `unlock_at` timestamp has passed

**Behaviour:**
- Claims ALL currently-unlocked tranches in a single call
- Skips tranches that have already been claimed
- Marks each claimed tranche as `claimed: true`

### Query — `get_release_schedule`

```rust
pub fn get_release_schedule(env: Env, escrow_id: u32) -> Vec<ReleaseTranche>
```

Returns the full release schedule, including which tranches have been claimed.

---

## Error Codes

| Mode | Error | Description |
| --- | --- | --- |
| Time-Locked | `UNLOCK_TIME_HAS_NOT_PASSED` | Claim called before `unlock_at` |
| Time-Locked | `PAST_UNLOCK_TIME_USE_CLAIM_TIMELOCKED` | Cancel called after `unlock_at` |
| Time-Locked | `ONLY_BUYER_CAN_CANCEL` | Non-buyer attempted to cancel |
| Inactivity | `ONLY_ESCROW_SELLER_CAN_CLAIM_INACTIVITY_RELEASE` | Non-seller attempted to claim |
| Inactivity | `BUYER_INACTIVITY_WINDOW_HAS_NOT_ELAPSED` | Inactivity period not yet passed |
| Scheduled | `ONLY_BENEFICIARY_SELLER_CAN_CLAIM_SCHEDULED_RELEASES` | Non-beneficiary attempted to claim |
| Scheduled | `NO_TRANCHES_ARE_CURRENTLY_CLAIMABLE` | No tranches have unlocked yet |

---

## Use Case Recommendations

### Time-Locked
- **Escrow for a specific date:** Hold funds until a known future event (e.g., contract signing date)
- **Simple buyer protection:** Funds locked until buyer confirms goods/services received

### Inactivity Release
- **Abandoned transaction protection:** Auto-release to seller if buyer stops responding
- **Recurring services:** Release funds if buyer doesn't renew

### Scheduled Release
- **Milestone payments:** Progressive release as work stages complete
- **Subscription-like flows:** Regular payouts over time
- **Construction/development:** Funds released at project phase completion

---

## Interactions with Other Features

All three time-based release modes interact with the standard escrow lifecycle:

- **Funding:** Standard `fund_escrow` applies to all modes
- **Release:** Mode-specific claim functions replace the standard release
- **Dispute:** Standard dispute flow applies; time-based releases are paused during disputes
- **Cancellation:** Mode-specific cancel functions (or standard cancel before release)
- **Extension:** The round deadline extension feature does not affect time-based release timestamps