# Escrow Seller Veto

> **Status:** Implemented in `contracts/ahjoor-escrow/src/lib.rs`
> (`raise_seller_veto`, `admin_override_veto`, `set_veto_cooldown_seconds`,
> `get_veto_cooldown_seconds`, `transfer_seller_role`, `veto_seller_transfer`,
> `approve_seller_transfer`, `expire_seller_transfer_veto`,
> `get_seller_transfer_proposal`)
> and tested in `contracts/ahjoor-escrow/src/test_seller_veto.rs`.

---

## Overview

The seller veto mechanism gives the current seller a last line of defence when a
buyer initiates a transfer of escrow ownership to a new seller. Within a
configurable cooldown window the seller can call `raise_seller_veto` to block
a fund release. An admin can override an active veto with
`admin_override_veto`, which restarts the cooldown clock so the seller cannot
immediately re-veto.

Separately, `transfer_seller_role` allows a seller to propose handing off their
role to a new address. The buyer has a configurable veto window in which they
can accept or reject the transfer via `approve_seller_transfer` or
`veto_seller_transfer`.

| Fact | Value |
| --- | --- |
| Default cooldown | 7 days (`DEFAULT_VETO_COOLDOWN_SECONDS = 7 * 24 * 60 * 60`) |
| Veto entry point | `raise_seller_veto` |
| Admin override entry point | `admin_override_veto` |
| Cooldown configuration | `set_veto_cooldown_seconds` / `get_veto_cooldown_seconds` |
| Storage key (last veto ts) | `DataKey2::SellerVetoLastTimestamp(escrow_id)` |

---

## When a Seller May Call `raise_seller_veto`

```rust
pub fn raise_seller_veto(env: Env, seller: Address, escrow_id: u32)
```

**Authentication:** `seller.require_auth()`

**Preconditions:**
1. Caller must be the escrow's `seller` field (`OnlyEscrowSellerCanRaiseVeto`).
2. The escrow must be in an open (active) status (`EscrowIsNotActive`).
3. The cooldown window since the **last veto** on this escrow must have fully
   elapsed — see [Cooldown window](#cooldown-window) below.

**Effect:**
- Records `DataKey2::SellerVetoLastTimestamp(escrow_id) = current_timestamp`.
- While a veto timestamp is within the cooldown window, `release_escrow` called
  by the buyer is blocked (`SellerVetoActive` / `SellerVetoActive2`).
- Emits `SellerVetoRaised`.

---

## Cooldown Window

The cooldown window controls how long after a veto the seller must wait before
raising another veto on the same escrow.

### Reading the current cooldown

```rust
pub fn get_veto_cooldown_seconds(env: Env) -> u64
```

Returns the current cooldown in seconds. Defaults to `DEFAULT_VETO_COOLDOWN_SECONDS`
(7 days) if not configured.

### Configuring the cooldown (admin only)

```rust
pub fn set_veto_cooldown_seconds(env: Env, admin: Address, seconds: u64)
```

**Authentication:** `admin.require_auth()`

Sets the global veto cooldown. Stored in `DataKey2::VetoCooldownSeconds` on
instance storage. The new value applies to all future `raise_seller_veto` calls
immediately.

### Cooldown enforcement logic

When `raise_seller_veto` is called:
1. The stored `DataKey2::SellerVetoLastTimestamp(escrow_id)` is read.
2. If `current_timestamp < last_timestamp + cooldown_seconds`, the call panics
   with `VetoCooldownActive`.
3. If no prior timestamp exists (first veto on this escrow), the check passes.

---

## Admin Override (`admin_override_veto`)

```rust
pub fn admin_override_veto(env: Env, admin: Address, escrow_id: u32)
```

**Authentication:** `admin.require_auth()` — caller must be the stored admin.

**Preconditions:**
- Escrow must be in an open (active) status (`EscrowIsNotActive`).

**Effect:**
1. Resets `DataKey2::SellerVetoLastTimestamp(escrow_id)` to
   `current_timestamp`, starting a fresh cooldown window.
2. This clears the active veto: `release_escrow` is unblocked once the
   new cooldown window has elapsed.
3. Prevents the seller from immediately re-vetoing after the override —
   the seller must wait a full cooldown period before raising a new veto.
4. Emits `VetoOverridden`.

> [!NOTE]
> `admin_override_veto` does **not** transfer funds. It only resets the cooldown
> timestamp. To release funds after an override, the buyer (or arbiter) must
> still call `release_escrow` once the cooldown window has passed.

---

## Seller Role Transfer & Buyer Veto

These functions handle the separate flow where a seller wants to hand off their
role to another address:

### Initiating a transfer

```rust
pub fn transfer_seller_role(env: Env, seller: Address, escrow_id: u32, new_seller: Address)
```

**Authentication:** `seller.require_auth()` — only the current seller may
initiate. Calling this as the buyer panics.

**Effect:**
- Sets escrow status to `EscrowStatus::AwaitingBuyerVetoDecision`.
- Stores a `SellerTransferProposal` with `original_seller`, `new_seller`, and
  a `veto_deadline` calculated from the configured veto window.
- Emits `SellerTransferProposed`.

### Buyer accepts the transfer

```rust
pub fn approve_seller_transfer(env: Env, buyer: Address, escrow_id: u32)
```

**Authentication:** `buyer.require_auth()`

- Swaps `escrow.seller` to `new_seller`.
- Sets status back to `EscrowStatus::Active`.
- Clears the `SellerTransferProposal`.

### Buyer vetoes the transfer

```rust
pub fn veto_seller_transfer(env: Env, buyer: Address, escrow_id: u32)
```

**Authentication:** `buyer.require_auth()`

- Refunds the buyer (transfers escrow funds back to buyer).
- Sets status to `EscrowStatus::Refunded`.
- Clears the `SellerTransferProposal`.

### Transfer veto window expires (permissionless)

```rust
pub fn expire_seller_transfer_veto(env: Env, escrow_id: u32)
```

If the buyer takes no action within the veto window, anyone may call this
function to auto-approve the transfer (sets the new seller and restores Active
status).

### Querying a pending proposal

```rust
pub fn get_seller_transfer_proposal(env: Env, escrow_id: u32) -> Option<SellerTransferProposal>
```

Returns the proposal if one is pending, `None` otherwise. The proposal is
automatically cleared when the transfer is approved, vetoed, or expired.

---

## Flow Diagram

```text
┌─────────────────────────────────────────────────────────────────┐
│ Seller Veto (raise_seller_veto)                                  │
└─────────────────────────────────────────────────────────────────┘

  raise_seller_veto(seller, escrow_id)
          │
          ├──► [within cooldown]   → panic VetoCooldownActive
          │
          └──► [cooldown elapsed]
                  records SellerVetoLastTimestamp = now
                  release_escrow blocked while timestamp + cooldown > now
                          │
                          ├── buyer waits out cooldown → release_escrow unblocked
                          │
                          └── admin_override_veto(admin, escrow_id)
                                  resets SellerVetoLastTimestamp = now
                                  seller must wait full cooldown before re-veto

┌─────────────────────────────────────────────────────────────────┐
│ Seller Role Transfer (transfer_seller_role)                      │
└─────────────────────────────────────────────────────────────────┘

  transfer_seller_role(seller, escrow_id, new_seller)
          │
          └──► EscrowStatus::AwaitingBuyerVetoDecision
                          │
            ┌─────────────┼──────────────────────────────┐
            │             │                              │
            ▼             ▼                              ▼
     veto_seller_   approve_seller_             expire_seller_
     transfer()     transfer()                 transfer_veto()
     (buyer)        (buyer)                    (anyone, after deadline)
         │               │                              │
         ▼               ▼                              ▼
     Refunded         Active                         Active
     funds → buyer    seller = new_seller            seller = new_seller
```

---

## Events

| Event | Emitted By | Payload | Description |
| --- | --- | --- | --- |
| `SellerVetoRaised` | `raise_seller_veto` | `(escrow_id, seller, timestamp)` | Seller raised a veto blocking release. |
| `VetoOverridden` | `admin_override_veto` | `(escrow_id, admin, timestamp)` | Admin cleared the veto and restarted the cooldown. |
| `SellerTransferProposed` | `transfer_seller_role` | `(escrow_id, original_seller, new_seller)` | Seller proposed transferring their role. |

---

## Error Codes

| Code | Name | Description |
| --- | --- | --- |
| 34 | `OnlyAdminCanOverrideSellerVeto` | Non-admin called `admin_override_veto`. |
| 38 | `SellerVetoActive` | Release blocked because a veto is active. |
| 39 | `SellerVetoActive2` | Alternate check: release blocked due to active veto. |
| — | `OnlyEscrowSellerCanRaiseVeto` | Non-seller tried to call `raise_seller_veto`. |
| — | `VetoCooldownActive` | Second veto raised before cooldown elapsed. |
| — | `EscrowIsNotActive` | Veto or override called on a non-active escrow. |

---

## Function Reference

| Function | Caller | Purpose |
| --- | --- | --- |
| `raise_seller_veto(seller, escrow_id)` | Seller | Raises a veto to block fund release during the cooldown window. |
| `admin_override_veto(admin, escrow_id)` | Admin | Clears an active veto and resets the cooldown clock. |
| `set_veto_cooldown_seconds(admin, seconds)` | Admin | Configures the global veto cooldown duration. |
| `get_veto_cooldown_seconds()` | Anyone | Reads the current global veto cooldown in seconds. |
| `transfer_seller_role(seller, escrow_id, new_seller)` | Seller | Proposes handing the seller role to a new address. |
| `approve_seller_transfer(buyer, escrow_id)` | Buyer | Accepts the proposed seller role transfer. |
| `veto_seller_transfer(buyer, escrow_id)` | Buyer | Rejects the proposed transfer and refunds the buyer. |
| `expire_seller_transfer_veto(escrow_id)` | Anyone | Auto-approves a transfer whose buyer veto window has expired. |
| `get_seller_transfer_proposal(escrow_id)` | Anyone | Returns the pending transfer proposal, or `None`. |

---

## Storage Reference

| Key | Storage Type | Data Stored |
| --- | --- | --- |
| `DataKey2::VetoCooldownSeconds` | Instance | `u64` global cooldown duration in seconds. |
| `DataKey2::SellerVetoLastTimestamp(escrow_id)` | Persistent | `u64` timestamp of the most recent veto (or admin override) on this escrow. |
| `DataKey::SellerTransferProposal(escrow_id)` | Persistent | `SellerTransferProposal` with `original_seller`, `new_seller`, and `veto_deadline`. |
