# ROSCA Succession

> **Status:** Implemented in `contracts/ahjoor-rosca/src/lib.rs`
> (`designate_successor`, `accept_succession`, `claim_succession`,
> `get_successor`, `set_succession_trigger_rounds`, `get_succession_trigger_rounds`,
> `get_consecutive_misses`, `get_succeeded_from`) and exercised in
> `contracts/ahjoor-rosca/src/test.rs`.

---

## Overview

`ahjoor-rosca` provides a succession mechanism that allows a designated successor to take over an
inactive member's slot in the ROSCA group. This ensures continuity when a member becomes
inactive — their contributions, payout slot, and community standing can be transferred to a
trusted successor rather than leaving the group understaffed.

The succession flow requires three steps: designation, acceptance, and claim after inactivity.

| Fact | Value |
| --- | --- |
| Contract | `ahjoor-rosca` |
| Entry points | `designate_successor`, `accept_succession`, `claim_succession`, `set_succession_trigger_rounds` |
| Query functions | `get_successor`, `get_succession_trigger_rounds`, `get_consecutive_misses`, `get_succeeded_from` |
| Storage keys | `DataKey5::SuccessionTriggerRounds`, `DataKey5::ConsecutiveMisses`, `DataKey5::Successor(Address)`, `DataKey5::SucceededFrom(Address)` |

---

## Lifecycle

```text
        +----------------------------------------------------+
        | STEP 1: Designation                                |
        | member calls designate_successor(member, successor)|
        | - Member must be in the group                      |
        | - Successor must not already be a member           |
        | - Successor must not be banned                     |
        | - Creates designation with accepted: false         |
        | - Emits SuccessorDesignated                        |
        +----------------------------------------------------+
                              |
                              v
        +----------------------------------------------------+
        | STEP 2: Acceptance                                 |
        | successor calls accept_succession(successor, member)|
        | - Must match the designated successor              |
        | - Sets accepted: true                              |
        | - Emits SuccessorAccepted                          |
        +----------------------------------------------------+
                              |
                              v
        +----------------------------------------------------+
        | STEP 3: Inactivity Trigger                         |
        | - Member misses consecutive contributions          |
        | - Each missed round increments ConsecutiveMisses   |
        | - Default trigger: 3 missed rounds                 |
        | - Configurable via set_succession_trigger_rounds   |
        +----------------------------------------------------+
                              |
                              v
        +----------------------------------------------------+
        | STEP 4: Claim                                      |
        | successor calls claim_succession(successor, member)|
        | - Requires accepted designation                    |
        | - Requires member missed >= trigger_rounds         |
        | - Successor inherits slot, contributions, debt    |
        | - Emits SuccessionClaimed                          |
        +----------------------------------------------------+
```

### 1. Designate — `designate_successor`

```rust
pub fn designate_successor(env: Env, member: Address, successor: Address)
```

**Authentication:** `member.require_auth()`

**Preconditions:**
- Caller must be a member of the group
- `successor` must not already be a member of the group
- `successor` must not be the same as `member`
- `successor` must not be banned

**Behaviour:**
1. Creates a `SuccessorDesignation` record with `accepted: false`
2. Stores it under `DataKey5::Successor(member)`
3. Emits `SuccessorDesignated`

**Note:** Calling `designate_successor` again replaces any previous designation, but the new
successor must still accept.

### 2. Accept — `accept_succession`

```rust
pub fn accept_succession(env: Env, successor: Address, member: Address)
```

**Authentication:** `successor.require_auth()`

**Preconditions:**
- Must be the designated successor for `member`
- Designation must not already be accepted

**Behaviour:**
1. Updates the designation to `accepted: true`
2. Emits `SuccessorAccepted`

### 3. Claim — `claim_succession`

```rust
pub fn claim_succession(env: Env, successor: Address, member: Address)
```

**Authentication:** `successor.require_auth()`

**Preconditions:**
- Must be the accepted successor for `member`
- Member must have missed at least `succession_trigger_rounds` consecutive contributions

**Behaviour:**
1. Validates the acceptance and inactivity thresholds
2. Successor must acknowledge the group charter (`require_charter_acknowledged`)
3. Transfers membership count if the group has a concurrency cap
4. **Transfers to successor:**
   - Slot position in member list and payout order (`DataKey::Members`, `DataKey::PayoutOrder`)
   - Per-member contribution state:
     - `MemberContributions` (total contributed)
     - `MemberParticipation` (rounds participated)
     - `MemberCollected` (amounts collected)
     - `MemberGoals` (savings goals)
     - `ClaimedRewards` (rewards claimed)
     - `RewardWeights` (voting/reward weight)
     - `MemberTiers`, `MemberTierIndex` (tier data)
     - `PrepaidBalances` (prepaid contributions)
   - Outstanding catch-up debt (`DataKey2::CatchUpDebt`)
5. **Clears from inactive member:**
   - Defaulters entry, SuspendedMembers entry
   - DefaultCount, PendingPenalties, LateContributionCount, ConsecutiveMisses
6. Links the successor to the original member via `DataKey5::SucceededFrom`
7. Emits `SuccessionClaimed` with the number of missed rounds and debt transferred

---

## Configuration — `set_succession_trigger_rounds`

```rust
pub fn set_succession_trigger_rounds(env: Env, admin: Address, rounds: u32)
```

**Authentication:** `admin.require_auth()` (caller must equal `DataKey::Admin`)

**Parameters:**
- `rounds`: Number of consecutive missed contributions required before a successor can claim

**Validation:**
- `rounds` must be greater than 0 (panics with `ExtError2::InvalidSuccessionTrigger`)

| Field | Default | Description |
| --- | --- | --- |
| `succession_trigger_rounds` | 3 | Consecutive missed rounds before succession is allowed |

The default of 3 means a member must miss 3 consecutive rounds before their successor can claim
the slot.

---

## What Transfers to the Successor

| Category | Transferred | Not Transferred (Cleared) |
| --- | --- | --- |
| **Slot** | Member list position, payout order position | — |
| **Contributions** | Total contributed, participation count, collected amounts | — |
| **Goals & Rewards** | Savings goals, claimed rewards, reward weights | — |
| **Tiers** | Member tier, tier index | — |
| **Prepaid** | Prepaid balances | — |
| **Debt** | Catch-up debt (added to successor's existing debt) | — |
| **Status** | — | Defaulter status, suspension, default count, pending penalties, late contribution count, consecutive misses |

The inactive member's penalty history is **not** transferred — the successor starts fresh in
terms of defaults and penalties.

---

## Events

| Event | Emitted by | Fields |
| --- | --- | --- |
| `SuccessorDesignated` | `designate_successor` | `member`, `successor` |
| `SuccessorAccepted` | `accept_succession` | `member`, `successor` |
| `SuccessionClaimed` | `claim_succession` | `member`, `successor`, `missed_rounds`, `debt_transferred` |

---

## Error Codes

| Code | Name | Raised by | Description |
| --- | --- | --- | --- |
| — | `InvalidSuccessor` | `designate_successor`, `accept_succession`, `claim_succession` | Successor is same as member, or successor is already a member. |
| — | `SuccessorNotDesignated` | `accept_succession`, `claim_succession` | No valid designation exists for this member-successor pair. |
| — | `SuccessionAlreadyAccepted` | `accept_succession` | The designation has already been accepted. |
| — | `MembershipCapReached` | `claim_succession` | Successor would exceed the group's MaxConcurrentMemberships cap. |
| — | `SuccessionThresholdNotMet` | `claim_succession` | Member hasn't missed enough consecutive contributions. |
| — | `SuccessionNotAccepted` | `claim_succession` | Successor hasn't accepted the designation yet. |
| 8 | `NotAMember` | `designate_successor`, `claim_succession` | The member is not in the group. |

---

## Query Functions

| Function | Returns | Description |
| --- | --- | --- |
| `get_successor(member)` | `Option<SuccessorDesignation>` | Current successor designation for a member, if any. |
| `get_succession_trigger_rounds()` | `u32` | Configured trigger threshold (default: 3). |
| `get_consecutive_misses(member)` | `u32` | Number of consecutive missed contributions for a member. |
| `get_succeeded_from(successor)` | `Option<Address>` | The original member whose slot was taken over. |

---

## Storage Reference

| Key | Storage | Data stored |
| --- | --- | --- |
| `DataKey5::SuccessionTriggerRounds` | Instance | `u32` trigger threshold |
| `DataKey5::ConsecutiveMisses` | Instance | `Map<Address, u32>` missed round count per member |
| `DataKey5::Successor(Address)` | Persistent | `SuccessorDesignation` for each member |
| `DataKey5::SucceededFrom(Address)` | Persistent | Link from successor to original member |

---

## Data Structures

```rust
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SuccessorDesignation {
    pub member: Address,           // The member who designated a successor
    pub successor: Address,        // The designated successor
    pub accepted: bool,            // Whether successor has accepted
    pub designated_at_ledger: u32, // Ledger sequence when designated
}
```

---

## Inactivity Tracking

The `ConsecutiveMisses` counter is updated when a round is closed or finalized:

1. Members who are defaulters (missed their contribution) have their count incremented
2. Members who contributed (even if late) have their count removed (reset to 0)
3. This ensures only **consecutive** misses count toward the threshold

This is the same mechanism used for member suspension after too many defaults.