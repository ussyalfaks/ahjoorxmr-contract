# Round Extension and Penalty Waiver in ROSCA

Members can use two governance proposals to give each other more time or to
cancel a late fee:

- **`ExtendRound`** pushes the current round's contribution deadline later.
- **`WaivePenalty`** cancels a member's late penalty, or refunds it if it was
  already charged.

Both are normal governance proposals: a member opens one, members vote with
`vote_on_proposal`, and anyone calls `execute_proposal` after the voting
window closes.

Source: `contracts/ahjoor-rosca/src/round_extension.rs` and
`contracts/ahjoor-rosca/src/penalty_waiver.rs`. Tests:
`test_round_extension.rs` and `test_penalty_waiver.rs`.

---

## Round extension

### Entry points

| Function | Caller | Description |
|---|---|---|
| `set_max_round_extension(admin, max_extra_seconds)` | admin, or a co-admin with `ManageRoundConfig` | Sets the largest extension one proposal may request. `0` (the default) turns the feature off. Emits `MaxRoundExtensionSet`. |
| `get_max_round_extension() -> u64` | anyone | The current maximum in seconds (`0` = off). |
| `propose_round_extension(proposer, extra_seconds, voting_duration) -> u32` | any member | Opens an `ExtendRound` proposal and returns its id. |
| `get_last_extended_cycle() -> Option<u32>` | anyone | The cycle in which an extension was last applied, or `None` if there has never been one. |

### Limits

The same checks run when the proposal is **opened** and again when it is
**executed**:

| Rule | Error |
|---|---|
| The feature is on (`max_extra_seconds > 0`) | `RoundExtensionDisabled` |
| `0 < extra_seconds <= max_extra_seconds` | `InvalidRoundExtension` |
| The current round deadline has not passed | `RoundDeadlineAlreadyPassed` |
| No extension has been applied in the current cycle | `RoundAlreadyExtended` |

**Maximum extension.** One proposal can add at most `max_extra_seconds`. The
limit is checked against the value in force at execution time. If the admin
lowers the maximum while a vote is in progress, a proposal that now exceeds it
fails at execution.

**Once per cycle.** When an extension is applied, its cycle is stored as
`LastExtendedCycle`. Any later extension in the same cycle is rejected, both
at proposal time and at execution. This applies to the whole group, not to
each proposer. The limit resets when the next cycle starts. Proposals that
fail the vote do not use up the cycle's extension.

**Voting must finish before the deadline.** An extension can only run while
the current deadline is still in the future. Choose a `voting_duration` that
ends before the round deadline, or the proposal will pass but fail to execute
with `RoundDeadlineAlreadyPassed`.

### Effect

On execution, the stored round deadline (and the timestamp deadline, if one is
set) moves later by `extra_seconds`. Because the stored deadline itself
changes, the following all use the new deadline:

- contribution and late-contribution checks;
- default detection and the late-penalty grace window (`finalize_round` /
  `close_round` copy it into `LastRoundDeadline`);
- `get_upcoming_payouts`.

Emits `RoundDeadlineExtended(proposal_id, round, cycle, extra_seconds, new_deadline)`.

---

## Penalty waiver

### Entry points

| Function | Caller | Description |
|---|---|---|
| `propose_penalty_waiver(proposer, member, round, voting_duration) -> u32` | any member | Opens a `WaivePenalty` proposal for `member`'s penalty in `round` and returns its id. |
| `get_collected_penalty(member, round) -> Option<CollectedPenaltyRecord>` | anyone | The penalty charged to `member` for `round`, if any. |

Use `get_pending_penalties` or `get_collected_penalty` to find the `round` to
pass.

### Limits

| Rule | Error |
|---|---|
| `member` has a **pending** penalty for `round` (queued during the grace window), or a **collected** penalty for `round` that has not been refunded | `PenaltyNotFound` |
| For a collected penalty: the contract's free balance of the penalty token covers the refund | `PenaltyRefundUnavailable` |

The penalty check runs at proposal time and again at execution. Each penalty
can be waived only once: after a refund the record is marked `refunded`, and
any further waiver proposal for that round fails with `PenaltyNotFound`.

*Free balance* is the contract's balance of the token. For the base token, it
excludes the reward pool, the insurance pool, vesting-locked funds and
streak-bonus reserves, so a refund cannot take money set aside for those.

### Effect

**Pending penalty (not yet charged):**

- removed from `PendingPenalties`, so it is never charged;
- the member is removed from the defaulters list, so `penalise_defaulter`
  cannot charge it again;
- emits `PenaltyWaived(proposal_id, member, round, was_collected = false, refunded_amount = 0)`.

**Collected penalty (already charged):**

- the full amount charged for that round is sent back to the member, in the
  token it was paid in (all charges, if the penalty was applied more than
  once);
- the member's default count goes down by the number of times the penalty was
  applied;
- the reputation change the penalty caused is reversed;
- the record is marked `refunded`;
- emits `PenaltyWaived(proposal_id, member, round, was_collected = true, refunded_amount)`.

---

## Quorum

Both proposal types use the standard governance resolution in
`execute_proposal`:

1. **Quorum.** `votes_for + votes_against` must be at least
   `ceil(total_possible_votes * required_quorum / 10_000)`.
   `total_possible_votes` is the member count in `Equal` voting mode, or the
   sum of member contributions in `WeightedByContributions` mode.
2. **Majority.** `votes_for > votes_against`. A tie fails.

`required_quorum` is fixed when the proposal is created:

| Proposal type | `required_quorum` |
|---|---|
| `ExtendRound` | `set_quorum_per_type(ExtendRound, bps)` if set; otherwise the group default quorum percentage × 100 (51% → 5,100 bps unless changed) |
| `WaivePenalty` | `set_quorum_per_type(WaivePenalty, bps)` if set; otherwise the group default quorum percentage × 100 (51% → 5,100 bps unless changed) |

Neither type has a built-in quorum of its own (unlike `NoConfidence`, which
defaults to 67%). Changing the quorum later does not affect proposals that
already exist. See
[ROSCA Governance Quorum Requirements](rosca-governance-quorum.md) for how
overrides work.

A proposal that misses quorum or majority is marked `Rejected`. Neither the
deadline nor the penalty changes, and for `ExtendRound` the cycle's one
extension is not used up.
