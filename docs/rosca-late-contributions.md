# ROSCA Catch-Up Contributions and Late-Payment Demotion

> **Status:** Implemented in `contracts/ahjoor-rosca/src/lib.rs`
> (`configure_late_demotion`, `get_late_contrib_threshold`,
> `get_late_contribution_counts`, `pay_catch_up_contribution`,
> `get_catch_up_debt`) and exercised in
> `contracts/ahjoor-rosca/src/test_waitlist.rs` and
> `contracts/ahjoor-rosca/src/test_succession.rs`.

---

## Overview

Late money in a ROSCA is handled in two separate places, and the two are easy to
confuse:

| Mechanism | Question it answers | Entry points |
| --- | --- | --- |
| **Catch-up contribution** | A member joined after round 0 and "owes" the rounds they missed. How is that settled? | `pay_catch_up_contribution`, `get_catch_up_debt` |
| **Late-payment demotion** | A member keeps paying after the round deadline. What happens to their place in the payout queue? | `configure_late_demotion`, `get_late_contrib_threshold`, `get_late_contribution_counts` |

Both are stored on the `ahjoor-rosca` contract instance and are independent: a
late payment never creates catch-up debt, and catch-up debt never affects the
late-contribution counter.

| Fact | Value |
| --- | --- |
| Contract | `ahjoor-rosca` |
| Catch-up entry point | `pay_catch_up_contribution(member)` |
| Catch-up query | `get_catch_up_debt(member) -> i128` |
| Demotion configuration | `configure_late_demotion(admin, late_threshold, grace_period_seconds)` |
| Demotion queries | `get_late_contrib_threshold() -> u32`, `get_late_contribution_counts() -> Map<Address, u32>` |
| Storage keys | `DataKey2::CatchUpDebt`, `DataKey3::LateContributionCount`, `DataKey3::LateContribThreshold`, `DataKey3::GracePeriodSeconds` |
| Events | `WaitEnroll`, `MemberDemoted`, `LateCountReset`, `SuccessionClaimed` |

---

## Part 1 — Catch-up contributions

### Where the amount comes from

When a member is promoted off the waitlist into a vacancy
(`try_promote_from_waitlist`, reached from a member exit or a default-driven
suspension), the contract computes what the joiner missed in earlier rounds:

```text
catch_up_amount = CurrentRound × ContributionAmt
```

Only rounds already elapsed are counted — a promotion during round 0 owes
nothing, so `catch_up_amount` is `0` and no transfer is attempted. The formula
uses the group's *current* contribution amount, which means a mid-cycle
contribution rebalance (`try_rebalance_contribution`) also changes the size of
the catch-up charge.

The amount is **collected immediately** during the promotion call: the token
transfer happens inside `try_promote_from_waitlist`, before the
`MemberEnrolledFromWaitlist` event is emitted with the round and the collected
amount. No debt entry is created by a waitlist promotion, so
`get_catch_up_debt` stays `0` for a freshly promoted member.

### Where debt actually builds up

`DataKey2::CatchUpDebt` only becomes non-zero when a slot changes hands through
**succession** (`claim_succession`). The successor inherits the inactive
member's outstanding catch-up balance:

1. `debts.get(member)` is read; if it is positive, the entry is removed from the
   inactive member and **added to** the successor's existing balance.
2. `SuccessionClaimed` is emitted with the transferred `debt` amount.
3. The successor is now the debtor and must call `pay_catch_up_contribution` to
   clear it.

This is why the debt can be larger than a single round's contribution: every
transfer adds to whatever the successor already owes.

| Scenario | Debt recorded? | Paid when? |
| --- | --- | --- |
| Promoted from waitlist in round 0 | No (`catch_up_amount = 0`) | — |
| Promoted from waitlist in round ≥ 1 | No debt entry; the full amount is transferred at promotion | Immediately, during promotion |
| Slot taken over via `claim_succession` | Yes — predecessor's balance added to successor's | Successor calls `pay_catch_up_contribution` |

### `pay_catch_up_contribution`

```rust
pub fn pay_catch_up_contribution(env: Env, member: Address)
```

**Authentication:** `member.require_auth()`

**Preconditions:**
- Contract is not paused (`check_not_paused`)
- The group cycle is not completed (`check_not_completed`)

**Behaviour:**
1. Reads `DataKey2::CatchUpDebt` and looks up the member's balance.
2. Panics with `"No catch-up contribution owed"` when the balance is `0`.
3. Transfers the full balance from `member` to the contract using the group's
   base token.
4. Removes the member's entry from the debt map and persists it.
5. Extends the instance storage TTL.

The call always settles the **whole** outstanding balance — there is no
partial-payment parameter. Repeating the call after a successful settlement
panics, which makes it safe to treat as a one-shot obligation.

### `get_catch_up_debt`

```rust
pub fn get_catch_up_debt(env: Env, member: Address) -> i128
```

Returns the member's outstanding catch-up debt, or `0` when the member has no
entry in the map. This is the value `pay_catch_up_contribution` will attempt to
transfer.

---

## Part 2 — Late payments and payout-slot demotion

### The two deadlines

```
round deadline            hard deadline
      |                        |
      |<-- grace period ------>|
      |                        |
   on time                   late              rejected
```

- **Round deadline** — `DataKey::RoundDeadline` (ledger) or
  `DataKey::RoundDeadlineTimestamp` (when `use_timestamp_schedule` is on).
  Contributions at or before this point are on time.
- **Hard deadline** — the round deadline **plus** `GracePeriodSeconds`. A
  contribution after the round deadline but at or before the hard deadline is
  accepted and classified as **late**. Any contribution after the hard deadline
  panics with `ContributionWindowClosed` (`contribute`).

With the default `grace_period_seconds = 0`, the grace window is empty and any
payment after the deadline is rejected — late tracking is effectively off until
an admin configures a window.

### How the late counter moves

Late tracking only happens on a **fully paid** round:

| Situation | Effect on `LateContributionCount[member]` |
| --- | --- |
| Partial late payment, member's required amount not yet reached | Unchanged |
| Late payment that completes the required amount | `+1` (plus `-5` reputation, credit-score `"late"` update) |
| On-time payment that completes the required amount while count > 0 | Reset to `0` (emits `LateCountReset`, plus `+10` reputation) |
| On-time payment that completes the required amount while count is already 0 | No reset event |

Two consequences follow from this design:

- The counter is **consecutive**, not cumulative. A single on-time full payment
  wipes the whole history, so a member who is late in rounds 1, 3 and 5 with
  on-time payments in between never reaches a threshold of 3.
- A late contribution also calls `streak::mark_broken_current(..., "late")`,
  breaking the member's on-time streak independent of the demotion counter.

### What demotion does to the payout slot

Demotion is evaluated by `finalize_round`, after defaulters have been recorded
for the round. For every member in `DataKey::Members`:

1. Read `LateContributionCount[member]` and `LateContribThreshold`.
2. If `late_count >= late_threshold`, remove the member from their current
   position in `DataKey::PayoutOrder` and **append them to the end** of the
   order.
3. Record the new zero-based position (`new_slot_index = len - 1`) and emit
   `MemberDemoted { member, new_slot_index, late_count }`.

Because the queue is the list of remaining payouts, appending means the member
moves to the **last unpaid slot** — they keep their turn in the queue, but it
now comes after every member still ahead of them. The demoted slot applies to
the next payout that would have been theirs; already-received payouts are not
reversed.

Behaviour worth noting:

- The counter is **not** reset when a demotion fires. A member who remains at or
  above the threshold stays demoted, and each later `finalize_round` that sees
  them still at the threshold appends them again (no-op if they are already last)
  and emits `MemberDemoted` again.
- Multiple members can be demoted in the same `finalize_round`; each is appended
  in member iteration order, so the last one processed ends up at the very back.
- Demotion is skipped entirely if the member is no longer in `PayoutOrder`
  (`found == false` writes nothing for them).
- A successor who claims a slot inherits its position but **not** the
  predecessor's late count — `claim_succession` removes
  `LateContributionCount[member]`. The successor starts at `0` and can be
  demoted again on its own merits.
- Defaults and late payments are tracked separately: a member can be late
  without defaulting (the money arrived inside the grace window), and a defaulter
  who never paid is not demoted for lateness.

---

## Configuration — `configure_late_demotion`

```rust
pub fn configure_late_demotion(
    env: Env,
    admin: Address,
    late_threshold: u32,
    grace_period_seconds: u64,
)
```

**Authentication:** `admin.require_auth()` (caller must equal `DataKey::Admin`)

**Preconditions:**
- Contract is not paused
- `late_threshold` must be greater than `0` (panics with
  `Error::InvalidMaxDefaults`, the reused validation error)

**Behaviour:**
1. Stores `late_threshold` under `DataKey3::LateContribThreshold`.
2. Stores `grace_period_seconds` under `DataKey3::GracePeriodSeconds`, which the
   contribution path reads to compute the hard deadline.
3. Extends the instance storage TTL.

`init` already writes `GracePeriodSeconds` from the group config, so an admin
who only wants to enable late tracking can call `configure_late_demotion` with
the desired threshold and a non-zero grace window. Setting
`grace_period_seconds = 0` disables late acceptance (and therefore late
tracking) without changing the stored threshold.

| Field | Default | Description |
| --- | --- | --- |
| `late_threshold` | `3` | Late full payments needed before demotion (`get_late_contrib_threshold`) |
| `grace_period_seconds` | `0` from `init`, unless the group config sets one | Seconds after the round deadline during which payments are still accepted as late |

---

## Query Functions

| Function | Returns | Description |
| --- | --- | --- |
| `get_late_contrib_threshold()` | `u32` | Configured demotion threshold, default `3`. |
| `get_late_contribution_counts()` | `Map<Address, u32>` | Current consecutive late counts for every tracked member. Members absent from the map are at `0`. |
| `get_catch_up_debt(member)` | `i128` | Outstanding catch-up balance, `0` when none. |

`get_grace_period_seconds()` is the companion query for the configured window.

---

## Worked example

Group of 4, contribution amount 100, `late_threshold = 3`,
`grace_period_seconds = 3600`, payout order `[A, B, C, D]`.

1. Round 0 is created for a 1000-second deadline. A and B pay at t=900 (on
   time), C pays at t=1500 (inside the 3600s grace window) and completes its
   requirement, D misses the round.
2. C is late: `LateContributionCount[C] = 1`, C's streak is marked broken, C is
   not demoted (`1 < 3`). D is counted as a defaulter.
3. C is late in rounds 1 and 2 as well. The count reaches 3.
4. `finalize_round` for round 2 sees `3 >= 3`, removes C from
   `[A, B, C, D]` and appends it, leaving `[A, B, D, C]`. `MemberDemoted` is
   emitted with `new_slot_index = 3` and `late_count = 3`.
5. In round 3 C pays before the deadline. The count resets to `0` and
   `LateCountReset` is emitted — but C's slot is **not** restored; the demotion
   already applied to the payout order.
6. Separately, if C had been promoted off the waitlist in round 3, C would have
   been charged `3 × 100 = 300` at promotion. If D had instead taken over a
   slot via succession while owing 200 in catch-up debt, `get_catch_up_debt(D)`
   would return `200` until D calls `pay_catch_up_contribution`.

---

## Storage Reference

| Key | Storage | Data stored |
| --- | --- | --- |
| `DataKey2::CatchUpDebt` | Instance | `Map<Address, i128>` outstanding catch-up balance per member |
| `DataKey3::LateContributionCount` | Instance | `Map<Address, u32>` consecutive late *full* contributions per member |
| `DataKey3::LateContribThreshold` | Instance | `u32` demotion threshold (default `3`) |
| `DataKey3::GracePeriodSeconds` | Instance | `u64` grace window added to the round deadline |

---

## Events

| Event | Emitted by | Fields |
| --- | --- | --- |
| `WaitEnroll` (`emit_member_enrolled_from_waitlist`) | `try_promote_from_waitlist` | `member`, `vacated_by`, `round`, `catch_up_amount` |
| `MemberDemoted` | `finalize_round` | `member`, `new_slot_index`, `late_count` |
| `LateCountReset` | `contribute` (on-time full payment) | `member` |
| `SuccessionClaimed` | `claim_succession` | `member`, `successor`, `missed_rounds`, `debt_transferred` |

---

## Error Codes

| Error / message | Raised by | Description |
| --- | --- | --- |
| `Error::ContributionWindowClosed` | `contribute` | Payment arrived after the round deadline plus grace period. |
| `Error::InvalidMaxDefaults` | `configure_late_demotion` | `late_threshold` was `0` (existing validation error reused). |
| `"No catch-up contribution owed"` | `pay_catch_up_contribution` | The member has no outstanding catch-up balance. |
| `ExtError::OnlyAdminAllowed` | `configure_late_demotion` | Caller is not the group admin. |
