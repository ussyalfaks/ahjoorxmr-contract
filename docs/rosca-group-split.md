# ROSCA Group Split Flow

## Overview

The group split feature lets an admin divide one ROSCA group into two independent sub-groups, gated by member confirmation. Every current member must be assigned to exactly one of the two proposed sub-groups, and each member confirms their own participation before the proposal window closes.

The feature lives in `contracts/ahjoor-rosca/src/lib.rs` and is covered by `contracts/ahjoor-rosca/src/test_group_split.rs`. The proposal types and status enum are declared in `contracts/ahjoor-rosca/src/types.rs`; split errors live in `contracts/ahjoor-rosca/src/errors.rs`.

The lifecycle is three steps:

1. **Propose** — the admin submits two member lists (A and B) and a reason hash. The contract validates the partition and stores a `SplitProposal` with a confirmation deadline.
2. **Confirm** — each member confirms their own participation while the proposal is `Pending` and inside the confirmation window.
3. **Execute** — the admin executes the proposal while it is still inside the window. The source group is marked `Split`, members who never confirmed are refunded, and a `GroupSplitExecuted` event carries the derived sub-group identifiers.

---

## Data Model

```rust
pub enum SplitProposalStatus {
    Pending = 0,
    Executed = 1,
    Expired = 2,
}

pub struct SplitProposal {
    pub id: u32,
    pub group_a_members: Vec<Address>,
    pub group_b_members: Vec<Address>,
    pub split_reason_hash: BytesN<32>,
    pub confirmations: Vec<Address>,
    pub status: SplitProposalStatus,
    pub created_at_ledger: u32,
    pub expiry_ledger: u32,
}
```

Proposals are stored under `DataKey3::SplitProposals` as a `Map<u32, SplitProposal>`. Related instance keys are `DataKey3::SplitProposalCounter` and `DataKey3::SplitConfirmationWindow`.

`split_reason_hash` is an opaque `BytesN<32>` chosen by the admin (for example, a hash of an off-chain explanation). The contract stores it but never interprets it.

---

## Propose (`propose_group_split`)

```rust
pub fn propose_group_split(
    env: Env,
    admin: Address,
    group_id: u32,
    group_a_members: Vec<Address>,
    group_b_members: Vec<Address>,
    split_reason_hash: BytesN<32>,
) -> u32
```

Checks performed, in order:

- The contract must not be paused; the caller must authorise the call and match the stored admin, otherwise `ExtError::OnlyAdminAllowed`.
- The group must be `GroupStatus::Active`. If it is already `GroupStatus::Split`, the call fails with `ExtError::SourceGroupAlreadySplit`; any other non-active status fails with a plain `"Group is not active"` panic.
- **Partition validation** — for every address in the stored `DataKey::Members` list, the member must appear in exactly one of the two lists. The contract tests `in_a ^ in_b`, so a member present in both lists, or in neither, fails with `ExtError::SplitMembersInvalid`. Addresses in either list that are not current members also fail with `ExtError::SplitMembersInvalid`.

On success the proposal id is `SplitProposalCounter + 1` (the counter starts at `0` for a fresh contract, so the first proposal is id `1`). The proposal is stored with:

- `status = Pending`
- `created_at_ledger = env.ledger().sequence()`
- `expiry_ledger = created_at_ledger + confirmation_window`

`GroupSplitProposed { source_group_id, proposal_id }` is emitted and the id is returned.

A member list that repeats the same address within a single list is not rejected by this check, because membership is tested with `Vec::contains`.

---

## Confirm (`confirm_split_participation`)

```rust
pub fn confirm_split_participation(
    env: Env,
    member: Address,
    _group_id: u32,
    proposal_id: u32,
)
```

- The contract must not be paused and `member` must authorise the call.
- The caller must be in the current `DataKey::Members` list, otherwise `Error::NotAMember`.
- The proposal must exist, otherwise `ExtError::SplitProposalNotFound`.
- The proposal status must be `Pending`, otherwise a plain `"Proposal is not pending"` panic.
- The current ledger sequence must not be past `expiry_ledger`, otherwise `ExtError::SplitConfirmationWindowClosed`.
- The member must appear in `group_a_members` or `group_b_members` of this proposal, otherwise a plain `"Member not part of this split proposal"` panic.
- The member must not already be in `confirmations`, otherwise `ExtError::SplitAlreadyConfirmed`.

A successful confirmation appends the member to `SplitProposal::confirmations`. There is no event emitted for individual confirmations; progress is read back through `get_split_proposal`.

Note that `_group_id` is accepted for interface symmetry but is not used — proposals are looked up by `proposal_id` alone, and the counter is global to the contract rather than per group.

---

## Confirmation Window

The confirmation window is measured in **ledgers**.

- Default: **200 ledgers**, used whenever `DataKey3::SplitConfirmationWindow` has never been set.
- The admin can change it with `set_split_confirmation_window(&admin, window_ledgers)`. The change affects proposals created after the call; existing proposals keep the `expiry_ledger` they were created with.
- A proposal's deadline is fixed at creation: `expiry_ledger = created_at_ledger + window`.

Boundary behaviour (comparisons use `>` and `<=`):

| Ledger vs. `expiry_ledger` | `confirm_split_participation` | `execute_group_split` | `expire_split_proposal` |
|---|---|---|---|
| `<= expiry_ledger` | allowed (if other checks pass) | allowed (if other checks pass) | `ExtError::SplitMembersInvalid` |
| `> expiry_ledger` | `ExtError::SplitConfirmationWindowClosed` | `ExtError::SplitConfirmationWindowClosed` | allowed |

Confirming exactly on the `expiry_ledger` is still inside the window; expiring requires the sequence to be strictly greater than `expiry_ledger`.

---

## Execute (`execute_group_split`)

```rust
pub fn execute_group_split(env: Env, admin: Address, group_id: u32, proposal_id: u32)
```

- The contract must not be paused; the caller must authorise and match the stored admin, otherwise `ExtError::OnlyAdminAllowed`.
- The proposal must exist and have `status == Pending`, otherwise `ExtError::SplitProposalNotFound` (this also covers a proposal that was already `Executed` or was `Expired`).
- The current ledger sequence must not be past `expiry_ledger`, otherwise `ExtError::SplitConfirmationWindowClosed`.

Execution does **not** require every member to have confirmed. Members are partitioned into three sets: confirmed in A, confirmed in B, and unconfirmed.

**Refunds.** If the contract's token balance is greater than zero and the group has members, then for each unconfirmed member the contract transfers `contract_token_balance / total_member_count` (integer division) from the contract address to that member. The balance used is the contract's whole current token balance — there is no separate per-group reserve ledger. If the per-member share rounds down to zero, no transfer is made.

**Sub-group identifiers.** Two identifiers are derived for the event payload:

```rust
let group_a_id = group_id * 1000 + proposal_id * 2 - 1;
let group_b_id = group_id * 1000 + proposal_id * 2;
```

They are emitted in the event only. Executing a split does **not** deploy or initialise two new group contracts, copy members, or create two new on-chain groups.

**State changes.** `DataKey2::GroupStatus` is set to `GroupStatus::Split`, the proposal's `status` becomes `SplitProposalStatus::Executed`, and `GroupSplitExecuted { source_group_id, group_a_id, group_b_id }` is emitted. Confirmed members receive no payout at split time.

---

## Expire (`expire_split_proposal`)

```rust
pub fn expire_split_proposal(env: Env, proposal_id: u32)
```

Callable by anyone (no signature and no admin check — only the paused guard applies) once the window has closed.

- Proposal must exist, otherwise `ExtError::SplitProposalNotFound`.
- Proposal must still be `Pending`, otherwise `ExtError::SplitProposalNotFound`.
- The current ledger sequence must be strictly greater than `expiry_ledger`; otherwise `ExtError::SplitMembersInvalid`.

On success the proposal's `status` becomes `SplitProposalStatus::Expired`. A subsequent `execute_group_split` on it returns `ExtError::SplitProposalNotFound`, because execution only accepts `Pending` proposals.

---

## In-Flight Rounds After a Split

Executing a split does not unwind or reset the group's round state. The split entry points never read or write `DataKey::CurrentRound`, `DataKey::PaidMembers`, `DataKey::RoundDeadline`, `DataKey::PayoutOrder`, `DataKey::Members`, or `DataKey::ContributionAmt`; only the token refunds, the proposal status, and `GroupStatus` change.

Consequences worth knowing:

- **Rounds keep running.** `contribute` and the round/`finalize_round` paths reject a group only when its status is `GroupStatus::Dissolved`, so an in-flight round on a split group continues to accept contributions and complete normally under the original group's schedule and member list.
- **`is_active()` returns `false`.** The view returns true only when the timestamp has reached the start time **and** `GroupStatus::Active`.
- **No further splits.** `propose_group_split` fails with `ExtError::SourceGroupAlreadySplit` once the status is `Split`.
- **No merges.** `propose_merge` requires `GroupStatus::Active`, so it fails with a plain `"Group is not active"` panic on a split group. See [ROSCA Group Merge](rosca-group-merge.md) for the inverse flow.
- **No new groups are created.** The A/B identifiers exist only in the `GroupSplitExecuted` event; off-chain systems are expected to provision the two sub-groups from the event and the stored member assignment.

---

## Error Handling and Edge Cases

| Situation | Behaviour |
|---|---|
| Member appears in both A and B, or in neither | `ExtError::SplitMembersInvalid` (88) |
| List contains a non-member | `ExtError::SplitMembersInvalid` (88) |
| Group already split | `ExtError::SourceGroupAlreadySplit` (90) |
| Non-admin calls propose/execute | `ExtError::OnlyAdminAllowed` |
| Proposal id unknown | `ExtError::SplitProposalNotFound` (87) |
| Confirm after the window | `ExtError::SplitConfirmationWindowClosed` (89) |
| Execute after the window | `ExtError::SplitConfirmationWindowClosed` (89) |
| Execute a non-`Pending` proposal | `ExtError::SplitProposalNotFound` (87) |
| Member confirms twice | `ExtError::SplitAlreadyConfirmed` (91) |
| Non-member confirms | `Error::NotAMember` |
| Not every member confirmed before execute | Execution proceeds; unconfirmed members are refunded and do not join either sub-group |
| `expire_split_proposal` before the window closes | `ExtError::SplitMembersInvalid` (88) |
| Contract token balance is zero | No refunds are attempted; the split still executes |

`ExtError::SplitNotFullyConfirmed` (92) is declared in `errors.rs` but is not raised by any of the current split entry points: `execute_group_split` does not gate on full confirmation.

---

## API Reference

```rust
// Admin configuration
fn set_split_confirmation_window(env: Env, admin: Address, window_ledgers: u32);

// Lifecycle
fn propose_group_split(
    env: Env,
    admin: Address,
    group_id: u32,
    group_a_members: Vec<Address>,
    group_b_members: Vec<Address>,
    split_reason_hash: BytesN<32>,
) -> u32;
fn confirm_split_participation(env: Env, member: Address, group_id: u32, proposal_id: u32);
fn execute_group_split(env: Env, admin: Address, group_id: u32, proposal_id: u32);
fn expire_split_proposal(env: Env, proposal_id: u32);

// Read
fn get_split_proposal(env: Env, proposal_id: u32) -> SplitProposal;
```

Events:

```rust
GroupSplitProposed { source_group_id: u32, proposal_id: u32 }
GroupSplitExecuted { source_group_id: u32, group_a_id: u32, group_b_id: u32 }
```

---

## Testing

Coverage is in `contracts/ahjoor-rosca/src/test_group_split.rs`:

| Test | What it verifies |
|---|---|
| `test_propose_group_split_stores_proposal` | First proposal gets id `1`, starts `Pending`, and stores both member lists |
| `test_propose_split_invalid_member_assignment_panics` | A member in both lists is rejected |
| `test_confirm_split_participation` | Confirmations accumulate per member |
| `test_double_confirmation_panics` | A second confirmation from the same member is rejected |
| `test_execute_group_split_marks_source_as_split` | Execution moves the proposal to `Executed` |
| `test_operations_blocked_on_split_group` | A second split proposal on a split group is rejected |
| `test_confirmation_window_enforced` | Confirming after the window is rejected |
| `test_split_confirmation_rejects_after_expiry` | Past `expiry_ledger`, confirmation returns `SplitConfirmationWindowClosed`, `expire_split_proposal` marks the proposal `Expired`, and executing it returns `SplitProposalNotFound` |

The same file also contains `propose_merge` / `complete_merge` tests, which cover the separate [group merge](rosca-group-merge.md) feature.

---

## See Also

- [ROSCA Group Merge](rosca-group-merge.md) — the inverse operation: two groups combining into one, with an admin accept step instead of per-member confirmation.
- [Group Dissolution and Emergency Payout Voting in ROSCA](rosca-dissolution-and-emergency-payout.md) — what happens to funds when a group stops operating.
