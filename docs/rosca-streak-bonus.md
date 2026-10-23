# ROSCA Streak Bonus

> **Status:** Implemented in `contracts/ahjoor-rosca/src/lib.rs`
> (`fund_streak_bonus_pool`, `set_streak_bonus_bps`, `claim_streak_bonus`,
> `get_streak_status`, `get_streak_allocation`, `get_streak_bonus_pool`) and exercised in
> `contracts/ahjoor-rosca/src/test.rs`.

---

## Overview

`ahjoor-rosca` includes a streak bonus mechanism that rewards members who consistently contribute
on time for every round of a cycle. This positive incentive encourages reliable participation and
strengthens the ROSCA group's financial discipline.

The streak bonus is funded from a community pool. When a cycle completes, a portion of the pool
(`streak_bonus_bps`) is allocated equally among all members who maintained a perfect streak.

| Fact | Value |
| --- | --- |
| Contract | `ahjoor-rosca` |
| Entry points | `fund_streak_bonus_pool`, `set_streak_bonus_bps`, `claim_streak_bonus` |
| Query functions | `get_streak_status`, `get_streak_allocation`, `get_streak_bonus_pool` |
| Storage keys | `DataKey5::StreakBonusPool`, `DataKey5::StreakBonusBps`, `DataKey5::StreakRoster`, `DataKey5::StreakBroken`, `DataKey5::StreakAllocation`, `DataKey5::StreakClaimed` |

---

## Eligibility

A member is **eligible** for a cycle's streak bonus if they are **on track** throughout the entire
cycle:

1. **Roster Requirement:** The member must be on the cycle's roster (members present at the start
   of the cycle). Members who join mid-cycle cannot qualify.
2. **No Breaks:** The member must not have any streak-breaking actions during the cycle
3. **Active Membership:** The member must still be an active member (not exited) at cycle end

### Who Tracks Streaks

The system records a **roster** for each cycle — the list of members present at the start. Only
these members can qualify for that cycle's bonus. This prevents new members from joining just
before cycle end to claim a bonus they didn't earn.

For groups created before this feature existed, the system falls back to current membership as
the roster.

---

## Lifecycle

```text
        +----------------------------------------------------+
        | CYCLE IN PROGRESS                                  |
        | - Members make contributions each round           |
        | - On-time: streak continues                        |
        | - Late/Skip/Missed/Exit: streak breaks for cycle  |
        +----------------------------------------------------+
                              |
                              v
        +----------------------------------------------------+
        | CYCLE COMPLETES                                    |
        | admin calls finalize_round                         |
        | - streak::on_cycle_completed(cycle) is invoked     |
        | - Identifies eligible on-track members             |
        | - Allocates pool * bps / 10000 split among them    |
        | - Emits StreakBonusAllocated                       |
        +----------------------------------------------------+
                              |
                              v
        +----------------------------------------------------+
        | CLAIM PHASE                                        |
        | eligible member calls claim_streak_bonus(cycle)    |
        | - Transfers per-member allocation to member        |
        | - Marks as claimed for the cycle                   |
        | - Emits StreakBonusClaimed                         |
        +----------------------------------------------------+
```

### 1. Fund the Pool — `fund_streak_bonus_pool`

```rust
pub fn fund_streak_bonus_pool(env: Env, funder: Address, amount: i128)
```

**Authentication:** None — anyone can fund the pool

**Behaviour:**
1. Transfers `amount` tokens from `funder` to the contract
2. Adds to the `StreakBonusPool` balance
3. Emits `StreakBonusPoolFunded` with the funder, amount, and new pool balance

The pool balance is reserved from the round pot — it does not get distributed as part of normal
payouts.

### 2. Configure — `set_streak_bonus_bps`

```rust
pub fn set_streak_bonus_bps(env: Env, admin: Address, bps: u32)
```

**Authentication:** `admin.require_auth()` (caller must equal `DataKey::Admin`)

**Parameters:**
- `bps`: Percentage of the pool to distribute per cycle, in basis points (1 bp = 0.01%)

**Validation:**
- `bps` must be between 0 and 10,000 (panics with `ExtError3::InvalidStreakBonusBps`)

| Configuration | Effect |
| --- | --- |
| `bps = 0` | No bonus is distributed (feature effectively disabled) |
| `bps = 1000` | 10% of the pool is distributed each cycle |
| `bps = 10000` | 100% of the pool is distributed each cycle (capped at pool size) |

### 3. Claim — `claim_streak_bonus`

```rust
pub fn claim_streak_bonus(env: Env, member: Address, cycle: u32)
```

**Authentication:** None — permissionless (anyone can trigger for any member)

**Preconditions:**
- The cycle must be completed (allocation exists)
- The member must be in the eligible list for that cycle
- The member must not have already claimed for that cycle

**Behaviour:**
1. Checks eligibility and unclaimed status
2. Marks the member as having claimed
3. Transfers the allocated amount from the contract to the member
4. Emits `StreakBonusClaimed`

---

## Pool Split Calculation

When a cycle completes, the pool is split as follows:

```rust
let n = eligible.len() as i128;           // Number of eligible members
let pool = pool_balance(env);               // Current pool balance
let bps = bonus_bps(env) as i128;           // Configured bps

let desired_total = pool * bps * n / 10_000;
let allocated_total = desired_total.min(pool);  // Cap at pool size
let per_member_amount = allocated_total / n;
```

**Example: Mixed on-time and late members**

Consider a group of 5 members (A, B, C, D, E) with:
- Pool balance: 1,000 tokens
- `streak_bonus_bps`: 1,000 (10%)

During the cycle:
- **A**: On time every round → eligible
- **B**: On time every round → eligible
- **C**: Made a late contribution → streak broken
- **D**: Submitted a skip request → streak broken
- **E**: Missed a round → streak broken

**Calculation:**
- Eligible members: 2 (A, B)
- Desired total: 1,000 × 1,000 × 2 / 10,000 = 200 tokens
- Pool cap: 200 ≤ 1,000, so use 200
- Per member: 200 / 2 = **100 tokens each**

Each eligible member (A and B) can claim 100 tokens. The remaining 800 tokens stay in the pool
for future cycles.

---

## Actions That Break a Streak

A member's streak for a cycle breaks if any of these actions occur:

| Action | Reason String | Description |
| --- | --- | --- |
| **Late contribution** | `"late"` | Member contributed after the round deadline |
| **Skip request** | `"skip"` | Member requested to skip their contribution |
| **Missed round** | `"missed"` | Member did not contribute at all |
| **Emergency exit request** | `"exit"` | Member requested emergency exit from the group |

Once broken, the member cannot qualify for that cycle's bonus, even if all subsequent rounds are
on time. The streak is per-cycle — a new cycle starts fresh.

---

## Configuration — `set_streak_bonus_bps`

| Field | Default | Description |
| --- | --- | --- |
| `streak_bonus_bps` | 0 | Percentage of pool to distribute per cycle (0 = disabled) |

When set to 0, no bonus is allocated when cycles complete. The pool continues to accumulate
when members call `fund_streak_bonus_pool`.

---

## Events

| Event | Emitted by | Fields |
| --- | --- | --- |
| `StreakBonusPoolFunded` | `fund_streak_bonus_pool` | `funder`, `amount`, `pool_balance` |
| `StreakBroken` | Internal (any break action) | `member`, `cycle`, `reason` (late/skip/missed/exit) |
| `StreakBonusAllocated` | `on_cycle_completed` | `cycle`, `eligible_count`, `per_member_amount` |
| `StreakBonusClaimed` | `claim_streak_bonus` | `member`, `cycle`, `amount` |

---

## Error Codes

| Code | Name | Raised by | Description |
| --- | --- | --- | --- |
| 151 | `InvalidStreakBonusBps` | `set_streak_bonus_bps` | bps > 10_000 |
| 152 | `StreakCycleNotCompleted` | `claim_streak_bonus` | No allocation exists for cycle |
| 153 | `StreakNotEligible` | `claim_streak_bonus` | Member not in eligible list |
| 154 | `StreakBonusAlreadyClaimed` | `claim_streak_bonus` | Member already claimed for that cycle |

---

## Query Functions

| Function | Returns | Description |
| --- | --- | --- |
| `get_streak_status(member)` | `StreakStatus` | Current cycle, on_track flag, claimable cycle and amount |
| `get_streak_allocation(cycle)` | `StreakCycleAllocation` | Eligible members, per-member amount, claimed count |
| `get_streak_bonus_pool()` | `i128` | Current pool balance |

### StreakStatus Structure

```rust
pub struct StreakStatus {
    pub current_cycle: u32,          // Cycle currently in progress (0-based)
    pub on_track: bool,              // true while member is on track with no breaks
    pub claimable_cycle: Option<u32>, // Last eligible unclaimed cycle
    pub claimable_amount: i128,      // Amount claimable (0 when none)
}
```

### StreakCycleAllocation Structure

```rust
pub struct StreakCycleAllocation {
    pub cycle: u32,
    pub eligible: Vec<Address>,
    pub per_member_amount: i128,
    pub claimed_count: u32,
}
```

---

## Storage Reference

| Key | Storage | Data stored |
| --- | --- | --- |
| `DataKey5::StreakBonusPool` | Instance | `i128` pool balance |
| `DataKey5::StreakBonusAllocated` | Instance | `i128` allocated but unclaimed balance |
| `DataKey5::StreakBonusBps` | Instance | `u32` basis points to distribute |
| `DataKey5::StreakRoster(u32)` | Persistent | `Vec<Address>` members at cycle start |
| `DataKey5::StreakBroken(u32)` | Persistent | `Vec<Address>` members whose streak broke |
| `DataKey5::StreakAllocation(u32)` | Persistent | `StreakCycleAllocation` for completed cycle |
| `DataKey5::StreakClaimed(u32, Address)` | Persistent | `bool` claim status |

---

## Integration with Round Lifecycle

The streak system integrates with the normal round finalization flow:

1. **During each round:** Track on-time vs late contributions via `mark_broken_current`
2. **On round close/finalize:** Track skipped members and missed rounds
3. **On emergency exit:** Mark streak as broken with reason `"exit"`
4. **On cycle completion:** Invoke `streak::on_cycle_completed` to allocate the pool

The `reserved_balance()` function ensures the pool (funded + allocated but unclaimed) is excluded
from round pot calculations.