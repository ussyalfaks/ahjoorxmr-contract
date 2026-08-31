# Escrow Auto-Renewal (`ahjoor-escrow`)

This document describes the **auto-renewal** mechanism in the `ahjoor-escrow` contract:
how the buyer can pre-approve renewal cycles for recurring service agreements, how
auto-renewals are triggered on escrow release, and how the buyer can cancel future
renewals at any time.

> Implementation references: `contracts/ahjoor-escrow/src/lib.rs`
> (`set_renewal_allowance`, `cancel_auto_renew`, `cancel_auto_renewal`,
> `get_renewal_history`, `get_auto_renewal_cancelled`, …) and the test suite
> `contracts/ahjoor-escrow/src/test_auto_renewal.rs`.

## Overview

**Auto-renewal** enables a buyer to automatically extend an escrow for additional
cycles without manual re-creation. When an escrow configured with auto-renewal is
released, the contract creates a new escrow with the same terms (buyer, seller,
arbiter, token, amount) but a fresh deadline and incremented renewal counter.

Auto-renewal is particularly useful for:
- Subscription-based services with recurring payments
- Retainer agreements that extend automatically
- Ongoing service contracts where manual renewal is cumbersome

| Fact | Value |
| --- | --- |
| Primary entry points | `set_renewal_allowance`, `cancel_auto_renewal`, `cancel_auto_renew` |
| Query functions | `get_renewal_history`, `get_auto_renewal_cancelled` |
| Configuration | `AutoRenewConfig` at escrow creation (via `create_escrow_with_auto_renew`) |
| Storage keys | `RenewalAllowance(escrow_id)`, `AutoRenewalCancelled(escrow_id)`, `RenewalHistory(original_escrow_id)` |

## Escrow Creation with Auto-Renewal

Auto-renewal is configured at escrow creation time via `create_escrow_with_auto_renew`:

```rust
let cfg = AutoRenewConfig {
    max_renewals: 3,
    renewal_interval_ledgers: 100,
};

let escrow_id = client.create_escrow_with_auto_renew(
    &buyer, &seller, &arbiter, &200, &token_addr, &deadline, &None, &cfg,
);
```

The `AutoRenewConfig` struct specifies:
- `max_renewals`: The maximum number of renewal cycles allowed (0 = disabled)
- `renewal_interval_ledgers`: The renewal interval in ledger sequences

## Setting Renewal Allowance

The buyer must pre-approve the total amount for all renewals before any auto-renewal
can occur. This prevents front-running and ensures funds are available.

```rust
pub fn set_renewal_allowance(env: Env, buyer: Address, escrow_id: u32, total_renewals: u32)
```

**Authentication:** `buyer.require_auth()`

**Preconditions:**
1. Caller must be the escrow buyer (`OnlyBuyerCanSetRenewalAllowance`)
2. Escrow must have auto-renewal enabled (`AutoRenewIsNotEnabledEscrow`)
3. `total_renewals` must be within the configured `max_renewals`

**Behavior:**
1. Calculates `total_amount = escrow.amount * total_renewals`
2. Sets a token approval for the contract to spend that amount from the buyer
3. Stores `RenewalAllowance(escrow_id) = total_renewals`

**Error codes:**
| Code | Name | Raised when |
| --- | --- | --- |
| 3045 | `OnlyBuyerCanSetRenewalAllowance` | Non-buyer calls `set_renewal_allowance` |
| 3046 | `AutoRenewIsNotEnabledEscrow` | Escrow does not have auto-renewal configured |
| 3211 | `InsufficientRenewalAllowance` | `total_renewals` exceeds configured max |

## Canceling Auto-Renewal

The buyer can cancel future auto-renewals at any time before the current period's
release. Once cancelled, no new renewal will be triggered.

### Legacy Method (deprecated)

```rust
pub fn cancel_auto_renew(env: Env, buyer: Address, escrow_id: u32)
```

**Authentication:** `buyer.require_auth()`

**Effect:**
- Sets `escrow.extensions.auto_renew = false`
- Clears `escrow.extensions.renewals_remaining = 0`
- Removes the `RenewalAllowance` storage entry

**Error codes:**
| Code | Name | Raised when |
| --- | --- | --- |
| 3047 | `OnlyBuyerCanCancelAutoRenew` | Non-buyer calls `cancel_auto_renew` |

### V2 Method (recommended)

```rust
pub fn cancel_auto_renewal(env: Env, buyer: Address, escrow_id: u32)
```

**Authentication:** `buyer.require_auth()`

**Preconditions:**
1. Caller must be the escrow buyer (`OnlyBuyerCanCancelAutoRenewal`)
2. Escrow must have `AutoRenewConfig` set (`NoAutoRenewConfigSetEscrow`)

**Effect:**
1. Sets `AutoRenewalCancelled(escrow_id) = true`
2. Emits `AutoRenewalCancelled` event
3. No new renewal will be triggered on release

**Error codes:**
| Code | Name | Raised when |
| --- | --- | --- |
| 3048 | `OnlyBuyerCanCancelAutoRenewal` | Non-buyer calls `cancel_auto_renewal` |
| 3049 | `NoAutoRenewConfigSetEscrow` | Escrow has no `AutoRenewConfig` |

## How Renewals Are Triggered

Auto-renewals are triggered during the `release_escrow` flow. When an escrow with
auto-renewal is released:

1. If `RenewalAllowance(escrow_id) > 0`, a new escrow is created
2. The new escrow copies most fields from the original (buyer, seller, arbiter, token, amount)
3. A fresh deadline is set based on `renewal_interval_ledgers`
4. `renewals_remaining` is decremented
5. `RenewalAllowance` is updated
6. The `EscrowAutoRenewed` or `EscrowAutoRenewedV2` event is emitted

### Renewal Storage

```rust
/// Returns the ordered list of successor escrow IDs created by auto-renewals
/// for the given original escrow ID.
pub fn get_renewal_history(env: Env, escrow_id: u32) -> Vec<u32>
```

This query returns all escrow IDs created through auto-renewal from the specified
original escrow, in chronological order.

```rust
/// Returns whether the buyer has cancelled future auto-renewals for this escrow.
pub fn get_auto_renewal_cancelled(env: Env, escrow_id: u32) -> bool
```

Returns `true` if `cancel_auto_renewal` was called, `false` otherwise.

## Events

| Event | Emitted by | Notes |
| --- | --- | --- |
| `EscrowAutoRenewed` | Auto-renewal trigger | Legacy event with `renewals_remaining` |
| `EscrowAutoRenewedV2` | Auto-renewal trigger | New event with `renewal_index` for tracking |
| `RenewalFailed` | Auto-renewal trigger | When renewal cannot proceed (e.g., insufficient allowance) |
| `AutoRenewalCancelled` | `cancel_auto_renewal` | Carries `escrow_id` |

### Event Schemas

```rust
// Legacy event
pub struct EscrowAutoRenewed {
    pub old_escrow_id: u32,
    pub new_escrow_id: u32,
    pub renewals_remaining: u32,
}

// V2 event
pub struct EscrowAutoRenewedV2 {
    pub original_escrow_id: u32,
    pub new_escrow_id: u32,
    pub renewal_index: u32,
}

// Failure event
pub struct RenewalFailed {
    pub escrow_id: u32,
    pub renewal_index: u32,
    pub reason: String,
}

// Cancellation event
pub struct AutoRenewalCancelled {
    pub escrow_id: u32,
}
```

## End-to-End Example

```text
1. Buyer creates escrow with auto-renewal:
   - max_renewals = 3, renewal_interval_ledgers = 100
   - escrow_id = 1

2. Buyer sets renewal allowance for 2 renewals:
   - set_renewal_allowance(escrow_id=1, total_renewals=2)
   - Token approval set for 2 * escrow_amount
   - RenewalAllowance(1) = 2

3. Escrow expires and buyer releases:
   - Auto-renewal triggered
   - New escrow (id=2) created with fresh deadline
   - EscrowAutoRenewedV2 emitted: original=1, new=2, index=1
   - RenewalAllowance(1) decremented to 1

4. Buyer calls cancel_auto_renewal(escrow_id=1):
   - AutoRenewalCancelled(1) emitted
   - get_auto_renewal_cancelled(1) returns true
   - Future renewals blocked

5. Attempted renewal on release of escrow 2:
   - get_auto_renewal_cancelled(1) is true
   - No new escrow created
   - RenewalFailed emitted with reason
```

## Function Reference

| Function | Caller | Purpose |
| --- | --- | --- |
| `create_escrow_with_auto_renew(...)` | Buyer | Create escrow with auto-renewal configuration |
| `set_renewal_allowance(buyer, escrow_id, total_renewals)` | Buyer | Pre-approve renewal cycles and funds |
| `cancel_auto_renew(buyer, escrow_id)` | Buyer | Legacy: Disable auto-renewal |
| `cancel_auto_renewal(buyer, escrow_id)` | Buyer | V2: Cancel future auto-renewals |
| `get_renewal_history(escrow_id)` | Anyone | Query renewal chain for an escrow |
| `get_auto_renewal_cancelled(escrow_id)` | Anyone | Check if auto-renewal is cancelled |

## Storage Reference

| Key | Storage | Contents |
| --- | --- | --- |
| `DataKey::Escrow(escrow_id)` | persistent | Full escrow record including `extensions.auto_renew` and `extensions.auto_renew_max_renewals` |
| `DataKey::RenewalAllowance(escrow_id)` | persistent | `u32` — remaining approved renewals |
| `DataKey2::AutoRenewalCancelled(escrow_id)` | persistent | `bool` — whether buyer cancelled auto-renewal |
| `DataKey2::RenewalHistory(original_escrow_id)` | persistent | `Vec<u32>` — chain of renewed escrow IDs |