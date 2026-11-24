# ROSCA Payout Slot Swaps

## Overview

Slot swaps let two members of an `ahjoor-rosca` group trade payout positions
by mutual agreement. A member who needs funds sooner can offer their later
slot to a member who is due earlier (and vice versa), without changing anyone
else's position and without moving any tokens.

The feature is implemented in `contracts/ahjoor-rosca/src/lib.rs` (section
`#213: Payout Slot Swap`) and uses the following entry points:

| Function | Caller | Purpose |
| --- | --- | --- |
| `set_slot_swap_config(admin, requires_admin, expiry_seconds)` | Admin | Choose whether accepted swaps also need admin approval, and how long a request stays open. |
| `request_slot_swap(initiator, round_a, round_b, counterparty) -> u32` | Initiator (member) | Propose swapping the initiator's slot `round_a` with the counterparty's slot `round_b`. Returns the swap id. |
| `accept_slot_swap(counterparty, swap_id)` | Counterparty | Agree to the swap. Executes it immediately unless admin approval is required. |
| `reject_slot_swap(counterparty, swap_id)` | Counterparty | Decline the swap. |
| `approve_slot_swap(admin, swap_id)` | Admin | Execute an accepted swap when admin approval is required. |
| `get_slot_swap(swap_id) -> SlotSwap` | Anyone | Read a swap request and its current status. |

## Configuration

`set_slot_swap_config` is admin-only (`ExtError::OnlyAdminAllowed` otherwise)
and blocked while the contract is paused. It stores two values:

| Setting | Default | Meaning |
| --- | --- | --- |
| `requires_admin` | `false` | When `true`, an accepted swap waits for `approve_slot_swap` before the payout order changes. When `false`, acceptance executes the swap. |
| `expiry_seconds` | `86_400` (24 h) | How long after `request_slot_swap` the counterparty has to accept. |

The expiry is copied into each request (`expiry_at = created_at +
expiry_seconds`) when it is created, so changing the setting later does not
affect requests that are already open. The `requires_admin` flag, on the other
hand, is read when the counterparty accepts.

## The `SlotSwap` record

```rust
pub struct SlotSwap {
    pub id: u32,
    pub initiator: Address,
    pub counterparty: Address,
    pub round_a: u32,          // initiator's slot (index into the payout order)
    pub round_b: u32,          // counterparty's slot
    pub status: SlotSwapStatus,
    pub created_at: u64,       // ledger timestamp of the request
    pub expiry_at: u64,        // created_at + expiry_seconds
    pub admin_approved: bool,
}

pub enum SlotSwapStatus {
    Pending = 0,
    Accepted = 1,
    Rejected = 2,
    Executed = 3,
    Expired = 4,
}
```

Swap ids start at `1` and increase by one per request. All swaps are kept in
instance storage and remain readable through `get_slot_swap` after they are
settled. `get_slot_swap` panics with `ExtError2::SwapNotFound` for an unknown id.

## Request → accept → (approve) flow

```text
                request_slot_swap
                        │
                        ▼
                   ┌─────────┐   reject_slot_swap    ┌──────────┐
                   │ Pending │ ────────────────────▶ │ Rejected │
                   └─────────┘                       └──────────┘
                        │ accept_slot_swap
          ┌─────────────┼───────────────────────────┐
          │ after       │ requires_admin = true     │ requires_admin = false
          │ expiry_at   ▼                           ▼
          ▼        ┌──────────┐ approve_slot_swap ┌──────────┐
     ┌─────────┐   │ Accepted │ ────────────────▶ │ Executed │
     │ Expired │   └──────────┘                   └──────────┘
     └─────────┘
```

### 1. Request

`request_slot_swap` must be authorised by the initiator and fails while the
contract is paused. It checks that:

- both the initiator and the counterparty are group members
  (`Error::OnlyMembersAllowed`);
- both slots are inside the payout order and strictly in the future:
  `current_round < round_a, round_b < payout_order.len()`
  (`ExtError::InvalidAmount` otherwise). A member cannot trade the slot being
  paid out this round or one that has already been paid;
- the initiator currently holds slot `round_a` and the counterparty currently
  holds slot `round_b` (`Error::OnlyMembersAllowed` otherwise).

The request is stored as `Pending` and a `SlotSwapReq` event is emitted with
`(swap_id, initiator, counterparty, round_a, round_b)`.

### 2. Accept or reject

Only the named counterparty can answer, and only while the swap is `Pending`
(`Error::OnlyMembersAllowed` for anyone else, `Error::ProposalNotPending` once
the swap has moved on).

- **`reject_slot_swap`** marks the swap `Rejected` and emits `SlotSwapRej`
  `(swap_id, counterparty)`. The payout order is untouched.
- **`accept_slot_swap`** (blocked while paused) marks the swap `Accepted` and
  emits `SlotSwapAcc` `(swap_id, counterparty)`. If `requires_admin` is
  `false`, the swap is executed in the same call.

### 3. Admin approval (optional)

When `requires_admin` is `true`, an accepted swap waits for the admin.
`approve_slot_swap` is admin-only (`ExtError::OnlyAdminAllowed`) and requires
the swap to be `Accepted` (`Error::ProposalNotPending` otherwise). It then
executes the swap. There is no admin "deny" call: an accepted swap the admin
does not want simply stays `Accepted`.

### 4. Execution

Execution exchanges the two entries at positions `round_a` and `round_b` of
`PayoutOrder` and leaves every other position as it was. The swap is marked
`Executed` and `SlotSwapExec` `(swap_id, round_a, round_b)` is emitted. No
tokens move: each member keeps the contributions they have already made and
continues to owe contributions every round, they just receive the pot in each
other's round.

## Expiry

Expiry is checked lazily, only inside `accept_slot_swap`:

- If the counterparty calls `accept_slot_swap` after `expiry_at`, the call does
  not fail. Instead the swap is marked `Expired`, a `SlotSwapExp` `(swap_id)`
  event is emitted, and the payout order is left unchanged.
- A request that nobody touches keeps the `Pending` status in storage even
  after `expiry_at`. Clients should compare `expiry_at` with the current
  ledger timestamp rather than rely on the status alone.
- `reject_slot_swap` does not check expiry, so the counterparty can still
  reject a stale `Pending` request to close it explicitly.
- Expiry only applies to the counterparty's answer. Once a swap is `Accepted`,
  `approve_slot_swap` can execute it at any later time; it does not compare
  against `expiry_at`.

## Interaction with slot auctions

Slot auctions (see [ROSCA Slot Auctions](rosca-slot-auctions.md)) and slot
swaps both rewrite the same `PayoutOrder`, and neither feature locks the other
out:

- `resolve_slot_auction` and `settle_sealed_slot_auction` move the winner into
  their `desired_slot` by exchanging them with the slot's current occupant.
  Swaps can be requested, accepted and executed while an auction is open.
- A swap stores **slot indexes**, not the members' positions at execution
  time. The occupant check (`payout_order[round_a] == initiator` and
  `payout_order[round_b] == counterparty`) runs only in `request_slot_swap`.
  Execution exchanges whatever addresses sit at `round_a` and `round_b` at that
  moment.

This means that if an auction (or anything else that reorders slots) is
resolved between the request and the execution of a swap, the swap can move
different members than the two who agreed to it. To avoid surprises:

1. Settle any open slot auction before requesting swaps, or wait until it is
   resolved.
2. With `requires_admin = true`, the admin should check `get_payout_order()`
   against the swap's `initiator`/`counterparty` before calling
   `approve_slot_swap`, and leave the swap unapproved if the slots have
   changed hands.
3. A member who won a slot at auction and then swaps it away gives up the slot
   they paid for; the winning bid is not refunded.

## Interaction with a finalized payout order

When payout order randomization is enabled, the admin calls
`finalize_payout_order` once to shuffle `PayoutOrder` and set
`PayoutOrderFinalized`. Finalization only blocks a second shuffle; it does
**not** freeze the order against swaps. After finalization:

- swaps are still allowed, and they apply to the shuffled order. Members
  should read `get_payout_order()` after finalization to learn which slot index
  they hold before calling `request_slot_swap`;
- a swap requested *before* finalization refers to slot indexes that were
  reshuffled underneath it. Executing it afterwards exchanges whoever now holds
  those indexes, so stale requests should be rejected and re-requested against
  the finalized order.

## Other things to know

- Slot indexes are positions in the payout order and are only valid within
  the first pass of the order (`< payout_order.len()`). Once the current round
  reaches the last slot, no further swaps can be requested.
- Because a swap only exchanges payout positions, features keyed by the
  payout round (payout holdback, vesting, reinvestment) apply to whoever
  receives the pot in that round after the swap. See
  [ROSCA Payout Holdback](rosca-payout-holdback.md).
- `admin_approved` is part of the record but is not set by the current
  implementation; use `status == Executed` to tell whether a swap took effect.
