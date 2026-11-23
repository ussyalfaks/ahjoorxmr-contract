# ROSCA Payout Holdback

## Overview

In a ROSCA, the members paid out first have the weakest incentive to keep
contributing: they already have the pot. Payout holdback protects the members
paid after them by withholding a share of each payout until the recipient has
made every remaining contribution for the cycle they were paid in.

- If the member keeps contributing, they claim the holdback back in full with
  `release_holdback`.
- If they miss a contribution, the holdback pays that contribution for them,
  and whatever is left of it is **forfeited** to the group's insurance pool.

The logic lives in `contracts/ahjoor-rosca/src/holdback.rs` and is exposed
through these entry points in `contracts/ahjoor-rosca/src/lib.rs`:

| Function | Caller | Purpose |
| --- | --- | --- |
| `set_payout_holdback_bps(admin, bps)` | Admin | Set the share of each payout withheld, in basis points (`0` disables, max `5_000` = 50%). |
| `get_payout_holdback_bps() -> u32` | Anyone | Read the configured share. |
| `release_holdback(group_id, member, cycle) -> i128` | Member | Claim the holdback for `cycle` once it is releasable. Returns the amount paid. |
| `get_holdback(member, cycle) -> Option<HoldbackRecord>` | Anyone | Read a member's holdback for a cycle. |

Only the group's base token is held back. Payouts in other approved tokens are
paid in full.

## Configuration

`set_payout_holdback_bps` is admin-only and blocked while the contract is
paused. It can only be called **before the group is activated** (the first
contribution is recorded), so every payout in the group's life is held back at
the same rate:

| Error | When |
| --- | --- |
| `InvalidHoldbackBps` (168) | `bps > 5_000`. |
| `ConfigLockedAfterActivation` (169) | The group is already activated. |

A `PayoutHoldbackSet { holdback_bps }` event is emitted on success.

## When a payout is held back

When a round is finalized, the payout for the base token is computed as
usual (pot minus reserved balances minus protocol fee). The holdback is then
taken from that amount:

```text
cycle_len  = payout_order.len()
cycle      = round / cycle_len
last_round = (cycle + 1) * cycle_len - 1
holdback   = payout_amount * holdback_bps / 10_000
```

Nothing is held back when:

- `holdback_bps` is `0`, or the computed amount rounds down to `0`;
- `round == last_round`, i.e. the recipient is the last member paid in the
  cycle. They owe no further contributions in this cycle, so there is nothing
  to protect.

The withheld amount stays in the contract and is tracked in `HoldbackLocked`,
which is excluded from future round pots in the same way as the insurance
pool and vesting balances. A `HoldbackRecord` is stored per `(member, cycle)`
and a `PayoutHeldBack { round, member, cycle, amount, last_round }` event is
emitted.

If a member is paid twice in the same cycle (for example after a skipped
round), the second holdback is added to the same record and `payout_round`
moves to the later round.

```rust
pub struct HoldbackRecord {
    pub member: Address,
    pub cycle: u32,
    pub amount: i128,       // total withheld for this cycle
    pub payout_round: u32,  // round the member was paid in
    pub last_round: u32,    // last round of the cycle
    pub covered: i128,      // part used to pay a missed contribution
    pub forfeited: i128,    // part sent to the insurance pool
    pub status: HoldbackStatus, // Held | Released | Forfeited
}
```

The member owes contributions for rounds `payout_round + 1 ..= last_round`.
Those are the rounds the holdback protects.

## When the holdback is released

`release_holdback` must be authorised by the member and fails while the
contract is paused. The holdback is releasable once every protected round has
been paid:

- the current round is past `last_round` (the cycle's last round has been
  settled), **or**
- the current round is `last_round` and the member has already paid it in
  full.

The full `amount` is then paid out, honouring payout splits or a nominated
payout beneficiary in the same way as a normal payout. The record becomes
`Released` and one `HoldbackReleased { member, cycle, recipient, amount }`
event is emitted per destination.

Release is never automatic: the member (or a client acting for them) must
call `release_holdback`. The `group_id` argument is accepted for interface
consistency and ignored, since each contract instance is one group.

| Error | When |
| --- | --- |
| `NoHoldbackRecord` (170) | Nothing was held back from this member in this cycle. |
| `HoldbackSettled` (171) | The holdback was already released or forfeited. |
| `HoldbackNotReleasable` (172) | The member still has protected rounds left to pay. |

## When the holdback is forfeited

### Missed contribution

When a round closes with defaulters, each defaulter whose holdback is still
`Held` and whose protected window contains that round is handled **before**
vesting, insurance or any reserve is touched:

```text
owed      = required contribution (tier-adjusted) - amount already paid this round
covered   = min(owed, holdback.amount)
forfeited = holdback.amount - covered
```

- `covered` is credited as the member's contribution for the round. If it pays
  the round in full, the member is no longer a defaulter for that round.
- `forfeited` is added to the group's insurance pool.
- The record becomes `Forfeited` and a
  `HoldbackForfeited { member, cycle, round, covered, forfeited }` event is
  emitted.

The holdback is settled the first time it is used. It does **not** stay
around to cover later misses in the same cycle; those fall through to vesting,
insurance and the usual default handling.

### Leaving the group

If a member exits or is removed while their holdback is still `Held`, and
they leave without paying the round being closed, the whole holdback is
forfeited to the insurance pool (`covered = 0`) when that round is reset.

## How holdback combines with payout vesting

Payout vesting (`set_payout_vesting_ledgers`)
locks a payout and releases it linearly over a number of ledgers. Both
features can be enabled at the same time and stack in a fixed order:

1. **Payout time.** The holdback is taken first, from the payout after
   fees. Only the remainder is vested. The holdback is never vested: when it
   is released it is paid in one lump sum.
2. **Default time.** When a member misses a contribution, the holdback
   covers it first. Only what the holdback could not cover is taken from the
   member's unvested balance. Then insurance and the normal default rules
   apply to anything still unpaid.
3. **Locked balances.** `HoldbackLocked` and `VestingLocked` are tracked
   separately and both are excluded from round pots.
4. **Claims.** `claim_vested` and `release_holdback` are independent. A member
   can claim vested funds during the cycle, but the holdback stays locked until
   the protected rounds are paid.

If the member chose to reinvest their payout, the holdback is still taken
first and only the remainder is reinvested.

## Worked example

A group of **5 members** (Alice, Bob, Carol, Dave, Eve, paid in that order),
contribution **100** tokens per round, no protocol fee,
`payout_holdback_bps = 3_000` (30%), and payout vesting over **1_000**
ledgers.

Each round's pot is `5 × 100 = 500`. One cycle is rounds `0..=4`, so
`last_round = 4` for everyone.

### Payouts

| Round | Recipient | Pot | Held back (30%) | Vested | Protected rounds |
| --- | --- | --- | --- | --- | --- |
| 0 | Alice | 500 | 150 | 350 | 1, 2, 3, 4 |
| 1 | Bob | 500 | 150 | 350 | 2, 3, 4 |
| 2 | Carol | 500 | 150 | 350 | 3, 4 |
| 3 | Dave | 500 | 150 | 350 | 4 |
| 4 | Eve | 500 | 0 (last round) | 500 | none |

After round 3, `HoldbackLocked = 4 × 150 = 600`. Those 600 tokens are not part
of any round pot.

### Scenario A: everyone keeps paying

- Dave pays his round-4 contribution. Because the current round is
  `last_round` and he has paid it, he can call `release_holdback(_, Dave, 0)`
  right away and receive **150**.
- Alice, Bob and Carol can do the same once they have paid round 4, or at any
  time after round 4 is finalized. Each receives **150**.
- Their 350 vested tokens are claimed separately with `claim_vested` as they
  vest.

Alice ends the cycle with the full 500 (350 vested + 150 released), the same
as without holdback, just received later.

### Scenario B: Alice misses round 2

Round 2 closes without Alice's 100. Her holdback is still `Held` and round 2
is in her protected window, so:

```text
owed      = 100
covered   = min(100, 150) = 100   → credited as Alice's round-2 contribution
forfeited = 150 - 100     = 50    → added to the insurance pool
```

- Alice is **not** a defaulter for round 2, and her vesting balance is not
  touched.
- Her holdback for cycle 0 is now `Forfeited` (`covered = 100`,
  `forfeited = 50`). `release_holdback(_, Alice, 0)` fails with
  `HoldbackSettled`.

If Alice also misses round 3, the holdback is already gone, so vesting takes
over: up to 100 of her still-unvested balance (out of the 350) covers round 3.
Only if that is not enough does she become a defaulter.

### Scenario C: Bob exits after round 1

Bob leaves the group before paying round 2. When round 2 is reset his holdback
is forfeited in full: `covered = 0`, `forfeited = 150` goes to the insurance
pool.

## Checklist for members

- Call `get_holdback(member, cycle)` to see how much is withheld and which
  rounds (`payout_round + 1 ..= last_round`) you still owe.
- Keep paying every round through `last_round`. Missing even one round
  forfeits whatever the holdback does not use.
- After paying `last_round`, call `release_holdback`. Nothing is released
  automatically.
