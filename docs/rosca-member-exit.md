# ROSCA Member Exit Guide

A ROSCA member can leave the group through three distinct paths. This document
explains each path, when to use it, what happens to the member's contributions,
and what happens if the member has already received a payout.

---

## Overview

| Path | Who initiates | Admin action needed | Notice period |
|---|---|---|---|
| [Voluntary exit](#1-voluntary-exit) | Member | No | Configurable (0 = immediate) |
| [Emergency exit](#2-emergency-exit) | Member | Yes — admin approves or rejects | None |
| [Admin-approved exit](#3-admin-approved-exit) | Admin via emergency-exit flow | Yes — admin is the driver | None |

Use **voluntary exit** when you want to leave on your own schedule with no
urgency. Use **emergency exit** when circumstances require leaving immediately
and you are willing to have an admin review the request.

---

## 1. Voluntary Exit

### How it works

1. **Request:** the member calls `request_voluntary_exit(member)`.
   - Blocked if the member has already exited (`MemberAlreadyExited`).
   - Blocked if the member is not in the group (`NotAMember`).
   - Blocked if a round is currently in progress (`ExitNotAllowedMidRound` —
     i.e. `PaidMembers` is non-empty).
   - Blocked if a voluntary exit is already pending (`VoluntaryExitPending`).
   - Blocked if an emergency exit is also pending (`ExitRequestPending`).

2. **Notice period (optional):**
   - The admin configures a notice period via `set_exit_notice_ledgers(admin,
     ledgers)`. Default is `0`.
   - If `notice == 0` the exit executes immediately inside
     `request_voluntary_exit`; no pending request is created.
   - If `notice > 0` a `VoluntaryExitRequest { effective_ledger,
     rounds_contributed }` is written to instance storage. The member **remains
     active** during the notice window and is still liable to contribute each
     round that opens while they wait.

3. **Finalize (permissionless):** once `ledger().sequence() >=
   effective_ledger`, anyone may call `finalize_voluntary_exit(member)`.
   - Blocked if `sequence < effective_ledger` (`ExitNoticeNotElapsed`).
   - Blocked if a round is currently in progress (`ExitNotAllowedMidRound`).

4. **Cancel:** while a voluntary exit is pending the member may call
   `cancel_voluntary_exit(member)` to withdraw the request and remain active.

### Entry points

| Function | Auth | Description |
|---|---|---|
| `set_exit_notice_ledgers(admin, ledgers)` | Admin | Configure the global notice period in ledgers. `0` = immediate. |
| `get_exit_notice_ledgers()` | — | Read the current notice period. |
| `request_voluntary_exit(member)` | Member | Submit a voluntary exit (or exit immediately if notice = 0). |
| `cancel_voluntary_exit(member)` | Member | Withdraw a pending voluntary exit request. |
| `finalize_voluntary_exit(member)` | Permissionless | Execute the exit once the notice period has elapsed. |
| `get_voluntary_exit_request(member)` | — | Returns `Option<VoluntaryExitRequest>` for a pending request. |
| `get_exit_notice_ledgers()` | — | Returns the configured notice period. |
| `get_exited_members(offset, limit)` | — | Paginated list of all exited members. |

### Events emitted

| Event | When |
|---|---|
| `voluntary_exit_requested(member, effective_ledger)` | Notice period > 0 and request queued. |
| `exit_ok(member, refund_amount)` | Exit finalized (immediate or after notice). |
| `voluntary_exit_cancelled(member)` | Member cancels a pending request. |

---

## 2. Emergency Exit

An emergency exit allows a member to flag an urgent need to leave. Because it
bypasses the notice period, an **admin must review and approve or reject** the
request before anything changes.

### How it works

1. **Request:** the member calls `request_emergency_exit(member)`.
   - Same mid-round and already-exited guards as voluntary exit.
   - Also blocked if a voluntary exit is already pending (`VoluntaryExitPending`).
   - Writes an `ExitRequest { member, rounds_contributed, refund_amount: 0,
     approved: false }` to **temporary storage** (`DataKey2::ExitRequests`).
   - Marks the member's contribution streak as broken
     (`streak::mark_broken_current`).

2. **Admin decides:**
   - `approve_exit(member)` — executes the exit immediately (see
     [Shared exit logic](#shared-exit-logic) below).
   - `reject_exit(member)` — removes the pending request; the member remains
     active and their streak stays marked.

3. **Inspect pending requests:** `get_exit_requests()` returns the full map of
   pending emergency exit requests.

### Entry points

| Function | Auth | Description |
|---|---|---|
| `request_emergency_exit(member)` | Member | Submit an emergency exit request. |
| `approve_exit(member)` | Admin | Approve and execute the exit. |
| `reject_exit(member)` | Admin | Reject the request; member stays active. |
| `get_exit_requests()` | — | Returns `Map<Address, ExitRequest>` of all pending emergency requests. |

### Events emitted

| Event | When |
|---|---|
| `exit_req(member, round)` | Emergency exit requested. |
| `exit_ok(member, refund_amount)` | Admin approves and exit is executed. |
| `exit_no(member)` | Admin rejects the request. |

---

## 3. Admin-Approved Exit

This is the same as the emergency exit path but driven by admin action — the
admin can call `approve_exit` on any pending emergency exit request regardless
of who initiated the original request. There is no separate admin-only "force
exit" entry point; admin control is exercised through the approve/reject
decision on member-submitted emergency requests.

---

## Shared Exit Logic

Both `approve_exit` (emergency path) and `finalize_voluntary_exit` (voluntary
path) call the internal `execute_early_exit` function. It performs the following
steps in order:

1. **Compute refund:**
   ```
   contributed_total  = contribution_amount × rounds_contributed
   received_payout    = sum of all payouts the member has received (from RoundHistory)
   penalty            = contributed_total × exit_penalty_bps / 10_000
   net                = contributed_total − received_payout − penalty
   refund_amount      = max(net, 0)
   ```
2. **Transfer refund** from the contract to the member if `refund_amount > 0`.
3. **Remove** the member from the `Members` list and decrement `membership_count`.
4. **Append** the member to `ExitedMembers`.
5. **Clear** any prepaid balance the member held (`DataKey4::PrepaidBalances`).
6. **Compact** the payout order to remove the member's slot.
7. **Update** the member's credit score with reason `"early_exit"`.
8. **Rebalance** contribution amounts if the waitlist is empty.
9. **Promote** the next member from the waitlist to fill the vacated slot.

---

## What happens to contributions already made

Contributions are tracked cumulatively per member in `DataKey::MemberContributions`.
The exit refund formula uses `rounds_contributed` (the round count at exit time)
multiplied by `contribution_amount` as the gross contribution figure.

- **If the member contributed but never received a payout:** the net is
  `contributed_total − penalty`, which is refunded in full (assuming no payout
  received). The penalty (`exit_penalty_bps`) is the only deduction.
- **If the member contributed and received a partial payout:** the payout
  amount is subtracted from the refund. The member keeps any payout already
  received; only the remaining uncommitted contributions come back.
- **If the net is zero or negative** (the payout received exceeded
  contributions, which can happen when a member receives the pot early): the
  refund is `0`. The member retains the payout and no additional amount is owed
  back to the group at exit time — the penalty mechanism is the group's
  protection against premature exit after receipt.

---

## What happens when a payout has already been received

The `PersistentKey::RoundHistory` ledger records every payout disbursement.
`execute_early_exit` scans this ledger and sums the amounts paid to the exiting
member to compute `received_payout`.

- If `received_payout >= contributed_total`, the refund is `0`.
- The exiting member is **not** required to return any portion of a payout
  already received. The `exit_penalty_bps` is therefore an important
  group-level parameter: set it high enough to deter members from collecting
  the pot and immediately exiting.
- After exit the member's slot is removed from the payout order and cannot
  receive future payouts.

---

## Reading exited members

```
get_exited_members(offset: u32, limit: u32) → Vec<Address>
```

Returns a paginated slice of the `ExitedMembers` list. If `limit = 0` or
`offset ≥ total`, returns an empty vector. The slice is clamped to the end of
the list when `offset + limit > total`.

---

## Storage locations

| Data | Storage tier | Key |
|---|---|---|
| Active members | Instance | `DataKey::Members` |
| Exited members | Instance | `DataKey::ExitedMembers` |
| Voluntary exit requests | Instance | `DataKey4::VoluntaryExitRequests` |
| Emergency exit requests | Temporary | `DataKey2::ExitRequests` |
| Notice period (ledgers) | Instance | `DataKey4::ExitNoticeLedgers` |
| Round payout history | Persistent | `PersistentKey::RoundHistory` |

Voluntary exit requests use **instance** storage because the member remains
active during the notice window and the request must survive for potentially
many ledgers. Emergency exit requests use **temporary** storage because they
are short-lived pending-admin-action state that auto-expires if neglected.
