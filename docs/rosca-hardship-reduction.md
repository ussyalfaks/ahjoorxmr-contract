# Hardship Contribution Reduction in ROSCA

A member going through temporary hardship can ask to pay less than their full
contribution for **one round**. The group votes on the request. If the group
approves it, the gap (the *shortfall*) is covered from the group's emergency
reserve, so the round pot and the payout stay the same.

Source: `contracts/ahjoor-rosca/src/hardship.rs`. Tests:
`contracts/ahjoor-rosca/src/test_hardship_reduction.rs`.

## Entry points

| Function | Caller | Description |
|---|---|---|
| `request_hardship_reduction(group_id, member, round, reduced_amount) -> u32` | the member (`member.require_auth()`) | Opens a `HardshipReduction` governance proposal and returns its id. |
| `get_hardship_request(proposal_id) -> Option<HardshipRequest>` | anyone | The request attached to a proposal, pending or approved. |
| `get_hardship_reduction(member, round) -> Option<HardshipRequest>` | anyone | The **approved** reduction for `member` in `round`, if any. |

`group_id` is accepted for consistency with other group-scoped calls. Each
contract instance manages a single group.

`HardshipRequest` fields:

| Field | Meaning |
|---|---|
| `proposal_id` | The `HardshipReduction` proposal that decides the request. |
| `member` | The member asking for the reduction. |
| `round` | The single round the reduction applies to. |
| `cycle` | The cycle that `round` belongs to (used for the once-per-cycle limit). |
| `reduced_amount` | What the member will pay for that round. |
| `shortfall` | `full contribution - reduced_amount`, covered by the reserve. |
| `approved` | `true` once the proposal has executed and the reserve has been drawn. |

The *full contribution* is the member's tier-adjusted amount:
`contribution_amount * tier_bps / 10_000`. Without a tier it is the base
contribution amount.

## Lifecycle

1. **Request.** The member calls `request_hardship_reduction`. The contract
   checks the request, records a `HardshipRequest`, opens a
   `HardshipReduction` proposal with a fixed 3-day voting window, and emits
   `HardshipReductionRequested`.
2. **Vote.** Members vote with `vote_on_proposal`, as for any other proposal.
3. **Execute.** After the voting window closes, anyone calls
   `execute_proposal`. If the proposal passes, the shortfall moves from the
   emergency reserve into a hardship *earmark*. The reduction is then stored
   under `(member, round)` and `HardshipReductionApproved` is emitted.
4. **Contribute.** For that round, the member's required contribution is
   `reduced_amount` instead of the full amount. This applies to `contribute`,
   to late contributions, and to the default/penalty checks.
5. **Settle.** When the round ends (payout or round close), the earmark is
   settled once and `HardshipShortfallSettled` is emitted for each member:
   - if the member paid, the shortfall joins the round pot;
   - if the member did not pay, the shortfall goes back to the emergency
     reserve.

## Approval rules

### When the request is made

| Check | Error |
|---|---|
| Caller is a member of the group | `NotAMember` |
| Caller has not exited | `MemberHasExited` |
| `round` is the current round or a future round | `HardshipRoundPassed` |
| `0 < reduced_amount < full contribution` | `InvalidHardshipAmount` |
| The member has no other pending or approved request in the same cycle | `HardshipAlreadyUsedThisCycle` |
| The emergency reserve is enabled and holds at least `shortfall` | `InsufficientReserveForHardship` |
| The group is not paused or frozen | pause / freeze errors |

A rejected request does not use up the member's one request for the cycle, so
they can ask again. A request that is pending or approved does use it up.

### The vote

`HardshipReduction` proposals use the standard governance resolution:

- **Quorum:** the per-type override set with
  `set_quorum_per_type(HardshipReduction, bps)`. Without an override, the
  group's default quorum percentage applies (51% unless changed). See
  [ROSCA Governance Quorum Requirements](rosca-governance-quorum.md).
- **Majority:** `votes_for` must be greater than `votes_against`. A tie fails.

### When the proposal executes

The checks run again, because things can change during the 3-day vote:

| Check | Error |
|---|---|
| `round` has not already passed | `HardshipRoundPassed` |
| If `round` is the current round, the member has not already paid `reduced_amount` or more for it | `AlreadyContributed` |
| The reserve still covers `shortfall` | `InsufficientReserveForHardship` |

If any check fails, the execution reverts and no reserve funds move.

## Effect on payouts

### The member asking for the reduction

- They pay `reduced_amount` for one round and are treated as fully paid for
  that round. They are not marked as a defaulter and are not charged a late
  penalty for the shortfall.
- **Their own payout is not reduced.** When their payout round comes, they
  receive the same pot as every other recipient.
- The shortfall is not a loan. There is no repayment schedule and no later
  deduction from the member's payout.
- If they pay nothing in the reduced round, they are treated like any other
  member who misses a round, measured against `reduced_amount`. The shortfall
  goes back to the reserve.

### The round's payout recipient

The earmarked shortfall is added to the pot when the round settles, so the
recipient gets the same amount they would have received if every member had
paid in full. Settlement runs **before** the payout is calculated.

### Other members

- **Contributions do not change.** No one else pays more to cover the gap.
- **The emergency reserve shrinks by `shortfall`** once the proposal
  executes. That money is no longer available for emergency loans, other
  hardship requests, or anything else the reserve pays for. The funds are
  returned only if the member fails to pay in the reduced round.
- While a reduction is approved but its round has not ended, the earmarked
  amount is excluded from both the reserve balance and the round pot. It
  cannot be counted twice.

## Example

The contribution is 100 and there are 5 members. The reserve holds 300.
Member A asks to pay 40 in round 3:

| Step | A pays | Reserve | Earmark | Round 3 pot |
|---|---|---|---|---|
| Request opened | — | 300 | 0 | — |
| Proposal executed | — | 240 | 60 | — |
| Everyone contributes | 40 | 240 | 60 | 440 |
| Round settles | — | 240 | 0 | **500** |

If A had paid nothing, the pot would be 400 (as with any missed payment), the
earmark would return to the reserve (back to 300), and A would be marked as a
defaulter for round 3.

## Events

| Event | When |
|---|---|
| `HardshipReductionRequested(proposal_id, member, round, reduced_amount, shortfall)` | Request opened |
| `HardshipReductionApproved(proposal_id, member, round, reduced_amount, shortfall)` | Proposal executed and reserve drawn |
| `HardshipShortfallSettled(member, round, shortfall, added_to_pot)` | Round ended; `added_to_pot` is `true` if the shortfall went to the pot and `false` if it went back to the reserve |
