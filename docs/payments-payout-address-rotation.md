# Payout Address Rotation

> **Status:** Implemented in `contracts/ahjoor-payments/src/lib.rs`
> (`request_payout_address_change`, `apply_payout_address_change`, `cancel_payout_address_change`,
> `set_payout_change_delay_ledgers`, `get_payout_change_delay_ledgers`) and exercised in
> `contracts/ahjoor-payments/src/test.rs`.

---

## Overview

`ahjoor-payments` allows merchants to change their payout address (the destination for settlement
funds) through a secure rotation mechanism. The key security feature is a configurable delay
period between requesting and applying the change, which protects merchants against stolen key
attacks. If a malicious actor obtains a merchant's key and attempts to redirect funds to a new
address, the legitimate merchant has a window to detect the unauthorized change and cancel it
before any funds are sent to the attacker's address.

| Fact | Value |
| --- | --- |
| Contract | `ahjoor-payments` |
| Entry points | `request_payout_address_change`, `apply_payout_address_change`, `cancel_payout_address_change`, `set_payout_change_delay_ledgers` |
| Query functions | `get_payout_change_delay_ledgers` |
| Storage key | `DataKey3::PendingPayoutChange`, `DataKey3::PayoutAddress`, `DataKey3::PayoutChangeDelayLedgers` |

---

## Security Rationale

The rotation mechanism exists to protect merchants whose keys may be compromised:

1. **Stolen Key Attack Vector:** An attacker with a compromised merchant key could call
   `request_payout_address_change` to set a new payout address under their control.
2. **Defense:** The delay window gives the legitimate merchant time to:
   - Notice the unauthorized change (via event monitoring)
   - Call `cancel_payout_address_change` to invalidate the request
3. **Post-Delay Protection:** Once the delay passes, the merchant must still call
   `apply_payout_address_change` to finalize the change. If the merchant's key was stolen,
   they can simply not call `apply`, and the change never takes effect.

---

## Lifecycle

```text
        +----------------------------------------------------+
        | merchant calls request_payout_address_change       |
        | (merchant, new_payout_address)                     |
        | - Requires merchant authentication                 |
        | - Stores PendingPayoutChange with effective_ledger |
        | - Emits PayoutAddressChangeRequested               |
        +----------------------------------------------------+
                              |
                              v
        +----------------------------------------------------+
        | Delay period passes (get_payout_change_delay_      |
        | ledgers() ledgers, default ~24 hours)              |
        |                                                    |
        | During this window:                                |
        | - Funds continue to go to OLD address              |
        | - Merchant can call cancel_payout_address_change   |
        +----------------------------------------------------+
                              |
                              v
        +----------------------------------------------------+
        | anyone calls apply_payout_address_change(merchant) |
        | - Checks current_ledger >= effective_ledger        |
        | - Updates PayoutAddress to new address             |
        | - Removes pending change record                    |
        | - Emits PayoutAddressChangeApplied                 |
        +----------------------------------------------------+
```

### 1. Request — `request_payout_address_change`

```rust
pub fn request_payout_address_change(env: Env, merchant: Address, new_payout: Address)
```

**Authentication:** `merchant.require_auth()`

**Behaviour:**
1. Computes `effective_ledger = current_ledger_sequence + delay_ledgers`
2. Stores a `PendingPayoutChange` with:
   - `new_payout`: The requested new address
   - `effective_ledger`: The earliest ledger when the change can be applied
3. Emits `PayoutAddressChangeRequested` with the merchant, new address, and effective ledger
4. If a pending change already exists, it is replaced (merchant can update their request)

**Note:** This function can be called multiple times. Each new request resets the delay timer.

### 2. Cancel — `cancel_payout_address_change`

```rust
pub fn cancel_payout_address_change(env: Env, merchant: Address)
```

**Authentication:** `merchant.require_auth()`

**Behaviour:**
1. Requires that a pending change exists (`ExtError::NoPendingPayoutChange` otherwise)
2. Removes the `PendingPayoutChange` record
3. Emits `PayoutAddressChangeCancelled`

The merchant's payout address remains unchanged.

### 3. Apply — `apply_payout_address_change`

```rust
pub fn apply_payout_address_change(env: Env, merchant: Address)
```

**Authentication:** None — permissionless (anyone can call)

**Behaviour:**
1. Requires that a pending change exists
2. Checks `current_ledger >= pending.effective_ledger` (`ExtError::PayoutChangeNotReady` otherwise)
3. Removes the pending change record
4. Updates `PayoutAddress(merchant)` to the new address
5. Emits `PayoutAddressChangeApplied`

**Fund Flow During Pending Period:**
- Funds continue to be sent to the **old/current** payout address
- The new address only receives funds after `apply_payout_address_change` is successfully executed
- The `resolve_payout_address` function (used by settlement logic) returns the current address,
  not any pending change

---

## Configuration — `set_payout_change_delay_ledgers`

```rust
pub fn set_payout_change_delay_ledgers(env: Env, admin: Address, delay_ledgers: u32)
```

**Authentication:** `admin.require_auth()` (caller must equal `DataKey::Admin`)

**Behaviour:**
1. Stores `delay_ledgers` under `DataKey3::PayoutChangeDelayLedgers`
2. This value applies to all merchants in the contract

| Field | Meaning | Default |
| --- | --- | --- |
| `delay_ledgers` | Number of ledgers to wait before a payout address change can be applied | 17,280 (~24 hours at 5 sec/ledger) |

The default of 17,280 ledgers provides approximately 24 hours for merchants to detect and cancel
unauthorized address change requests.

---

## Events

| Event | Emitted by | Fields |
| --- | --- | --- |
| `PayoutAddressChangeRequested` | `request_payout_address_change` | `merchant`, `new_payout`, `effective_ledger` |
| `PayoutAddressChangeCancelled` | `cancel_payout_address_change` | `merchant` |
| `PayoutAddressChangeApplied` | `apply_payout_address_change` | `merchant`, `new_payout` |

---

## Error Codes

| Code | Name | Raised by | Description |
| --- | --- | --- | --- |
| — | `NoPendingPayoutChange` | `cancel_payout_address_change`, `apply_payout_address_change` | No pending change exists for this merchant. |
| — | `PayoutChangeNotReady` | `apply_payout_address_change` | The delay period has not yet elapsed. |

---

## Function Reference

| Function | Caller | Purpose |
| --- | --- | --- |
| `request_payout_address_change(merchant, new_payout)` | Merchant | Requests a payout address change, starting the delay timer. |
| `cancel_payout_address_change(merchant)` | Merchant | Cancels a pending change before it takes effect. |
| `apply_payout_address_change(merchant)` | Anyone | Finalizes a pending change after the delay has passed. |
| `set_payout_change_delay_ledgers(admin, delay_ledgers)` | Admin | Sets the delay period for all merchants. |
| `get_payout_change_delay_ledgers()` | Anyone | Returns the current delay period in ledgers. |

---

## Storage Reference

| Key | Storage | Data stored |
| --- | --- | --- |
| `DataKey3::PendingPayoutChange(Address)` | Persistent | `PendingPayoutChange { new_payout: Address, effective_ledger: u32 }` |
| `DataKey3::PayoutAddress(Address)` | Persistent | `Address` — the current payout address (falls back to merchant address if not set) |
| `DataKey3::PayoutChangeDelayLedgers` | Instance | `u32` delay in ledgers |

---

## Integration Note

When processing settlements, the contract uses `resolve_payout_address` to determine where to send
funds:

```rust
fn resolve_payout_address(env: &Env, merchant: &Address) -> Address {
    env.storage()
        .persistent()
        .get(&DataKey3::PayoutAddress(merchant.clone()))
        .unwrap_or(merchant.clone())
}
```

This function always returns the **currently active** payout address — never a pending change.
This ensures that funds are never sent to an address that has not yet been applied.