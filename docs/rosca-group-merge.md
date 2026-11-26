# ROSCA Group Merge

## Overview

The group merge feature lets two ROSCA groups combine their member lists. One group is the **source group (A)** — the contract that stores the merge proposal and executes the merge — and the other is identified only by an external **group B id**.

Unlike [group split](rosca-group-split.md), which divides one group and requires every member to confirm, a merge is a two-admin handshake executed entirely from one contract. Group B's own contract is never called and is not modified by the flow.

The feature lives in `contracts/ahjoor-rosca/src/lib.rs` (section `#230: ROSCA Group Merge`). The proposal type is `MergeProposal` in `contracts/ahjoor-rosca/src/types.rs`, events are in `contracts/ahjoor-rosca/src/events.rs`, and the tests are in `contracts/ahjoor-rosca/src/test_group_split.rs`.

The lifecycle is three steps:

1. **Propose** — Group A's admin records a merge intent against `group_b_id`. The proposal starts with `accepted = false`.
2. **Accept** — the admin signals consent, flipping `accepted` to `true`. This is consent only: no members are copied and `GroupStatus` is unchanged.
3. **Complete** — the admin submits the list of members to absorb. The contract appends them to Group A's member and payout lists and marks Group A `Merged`.

Relevant `GroupStatus` values:

```rust
pub enum GroupStatus {
    Active = 0,
    Dissolved = 1,
    Merged = 2,   // merged into another group; all further interactions are rejected
    Split = 3,
    Completed = 4,
}
```

---

## Data Model

```rust
pub struct MergeProposal {
    pub id: u32,
    pub group_a_admin: Address,
    pub group_b_id: u32,
    pub proposed_at: u64,   // env.ledger().timestamp()
    pub accepted: bool,
}
```

Proposals are stored under `DataKey4::MergeProposals` as a `Map<u32, MergeProposal>`. Related keys:

| Key | Purpose |
|---|---|
| `DataKey4::MergeProposalCounter` | Last proposal id issued (starts at `0`, first proposal is `1`) |
| `DataKey4::MergeProposals` | Proposal map |
| `DataKey4::GroupMergedInto` | `group_b_id` the source group was merged into |

`complete_merge` removes the executed proposal from the map, so it is not readable afterwards. There is no expiry, cancellation or rejection entry point: an accepted-but-not-completed proposal stays pending indefinitely, and there is no getter for `GroupMergedInto` in the current API.

---

## Propose (`propose_merge`)

```rust
pub fn propose_merge(env: Env, admin: Address, group_b_id: u32) -> u32
```

Checks performed, in order:

- The contract must not be paused; `admin` must authorise the call and match the stored admin, otherwise `ExtError::OnlyAdminAllowed`.
- The group must be `GroupStatus::Active`; any other status fails with a plain `"Group is not active"` panic.
- **Between rounds only** — `DataKey::PaidMembers` must be empty, otherwise `"Merge only permitted between rounds"` panic.

On success the proposal id is `MergeProposalCounter + 1` and the proposal is stored with `proposed_at = env.ledger().timestamp()` and `accepted = false`. `MergeProposed { proposal_id, group_a_admin, group_b_id }` is emitted and the id is returned.

`group_b_id` is not validated: the contract neither resolves it to a contract address nor checks that the other group exists, is active, uses the same token, or has compatible contribution amounts. It is an opaque label stored in the proposal and later in `GroupMergedInto`.

There is also no check for a duplicate proposal against the same `group_b_id`; the admin can create several open proposals and accept/complete any of them.

---

## Accept (`accept_merge`)

```rust
pub fn accept_merge(env: Env, admin: Address, merge_proposal_id: u32)
```

- The contract must not be paused; `admin` must authorise and match the stored admin, otherwise `ExtError::OnlyAdminAllowed`.
- The proposal must exist, otherwise `"Merge proposal not found"` panic.
- The proposal must not already be accepted, otherwise `"Merge proposal already accepted"` panic.

A successful call sets `accepted = true` and emits `MergeAccepted { proposal_id }`. It does not touch members, payout order or `GroupStatus`.

Despite the name and the source comment about "Group B admin consent", the check is against **Group A's** stored admin, because the call runs on Group A's contract and only that admin can authorise it. Group B's contract has no merge state at all. If genuine two-party consent is required, it has to be enforced operationally off-chain (or by a future entry point on Group B's contract); nothing on-chain proves Group B's admin approved.

---

## Complete (`complete_merge`)

```rust
pub fn complete_merge(
    env: Env,
    admin: Address,
    merge_proposal_id: u32,
    new_members: Vec<Address>,
)
```

Checks performed, in order:

- The contract must not be paused; `admin` must authorise and match the stored admin, otherwise `ExtError::OnlyAdminAllowed`.
- The group must not already be `GroupStatus::Merged`, otherwise `"Group already merged"` panic. (Note this is an equality check, so a `Split`, `Completed` or `Dissolved` group is not rejected here.)
- The proposal must exist, otherwise `"Merge proposal not found"` panic.
- `proposal.accepted` must be `true`, otherwise `ExtError2::MigrationNotApproved` (108). This is the guard added for #400: without a prior `accept_merge`, completion fails.
- **Between rounds only** — `DataKey::PaidMembers` must be empty, otherwise `"Merge only permitted between rounds"` panic.
- Every address in `new_members` must not be banned, otherwise `ExtError3::MemberBanned`.
- `members.len() + new_members.len()` must be `<= MaxMembers`, otherwise `"Combined member count exceeds max_members"` panic. `MaxMembers` defaults to `50` when unset (it is written during `init` from `config.max_members`, validated to `1..=100`).
- `DataKey::Members` and `DataKey::PayoutOrder` must both exist, otherwise `"Not initialized"` panic.

On success:

- `DataKey2::GroupStatus = GroupStatus::Merged`
- `DataKey4::GroupMergedInto = proposal.group_b_id`
- The proposal is removed from `DataKey4::MergeProposals`
- `MergeCompleted { proposal_id, members_added }` and `GroupMerged { group_b_id }` are emitted

`members_added` in the event is `new_members.len()`, not the number actually appended, so it over-counts addresses already in the group and duplicates within `new_members`.

A second `complete_merge` cannot succeed: the first call sets `Merged`, so the status guard panics before the (now deleted) proposal lookup.

---

## Member Handling

Only the addresses passed in `complete_merge` are absorbed — the contract never reads Group B's member list, because it has no reference to Group B's contract. The admin is responsible for supplying the correct list.

For each address in `new_members`, in order:

- If the address is **not** already in `DataKey::Members`:
  - `internals::increment_membership_count` runs, enforcing `DataKey5::MaxConcurrentMemberships` (a `0` cap means unlimited) and panicking with `ExtError2::MembershipCapReached` if the member is at the cap.
  - The address is appended to the end of `DataKey::Members`.
  - The address is appended to the end of `DataKey::PayoutOrder`.
- If the address **is** already a member, it is skipped entirely (no duplicate entry, no second membership count).

Consequences worth knowing:

- **No approval, no confirmation, no opt-in.** Members are added by the admin; absorbed members never sign anything. This is the biggest difference from [group split](rosca-group-split.md), where every member must confirm their own assignment.
- **Existing members keep their positions.** Group A's original members are untouched and keep their payout slots.
- **No per-group member metadata is copied.** Contribution history, receipts, reputation, co-payers, co-signers, delegations, savings goals, skip counts and slot auctions are per contract and do not travel with the address. An absorbed member starts from whatever state Group A's contract already holds for them (normally nothing).
- **The new members are subject to Group A's rules**, including `DataKey::ContributionAmt` and Group A's token. The contract does not validate that absorbed members can pay it or that Group B used the same token.

---

## Payout Order Handling

`complete_merge` appends members to the existing `DataKey::PayoutOrder`; it never re-sorts, re-shuffles or rewrites the order:

- Group A's current payout order is preserved exactly.
- Newly absorbed members are appended in the order they appear in `new_members`, which places them **after** every existing member.
- Absorbed members therefore receive payouts only after all of Group A's original members have had their turns, in the combined cycle.
- If `new_members` contains duplicates, the first occurrence is appended and later ones are skipped (they are already members by then).
- Previously executed slot swaps, auctions, skips and suspensions in Group A are untouched, so the merged order still honours them.

`DataKey::CurrentRound` is not reset and `DataKey::PaidMembers` is not cleared — it is merely required to be empty before the merge. After completion the group is `Merged`, so `is_active()` returns `false` and `get_upcoming_payouts` returns an empty list; the appended payout order is visible only through the raw `PayoutOrder` storage or whatever view Group A's remaining active paths expose.

---

## Balance Transfer

**`complete_merge` moves no funds.** There is no token transfer, no reserve reconciliation and no balance check anywhere in `propose_merge`, `accept_merge` or `complete_merge`.

- Each ROSCA contract holds its own token balance. A merge is recorded on Group A's contract only; Group B's contract and its balance are never called, read or modified.
- Any tokens already contributed to Group B stay in Group B's contract under Group B's rules. Absorbed members do not carry their Group B contributions, credit or reserve share into Group A.
- Group A's existing balance, insurance pool, reserve, and pending payouts are unaffected by the merge.
- Absorbed members begin contributing to Group A at Group A's `ContributionAmt`; the contract does not credit them for anything paid in Group B, and no refund is issued to Group B members who are left behind.
- `GroupStatus::Merged` does not itself sweep or refund the source group's balance. Funds held by the merged source contract remain there; only Group A (the group that ran `complete_merge`) is marked `Merged`.

If funds need to move, that is a separate, explicit step — for example a dissolution/payout path or an off-chain settlement — and is not part of this feature.

---

## Post-Merge State

A completed merge blocks further merges and splits from the same contract:

| Entry point | Behaviour on a `Merged` group |
|---|---|
| `propose_merge` | Panics `"Group is not active"` |
| `complete_merge` | Panics `"Group already merged"` |
| `propose_group_split` | Panics `"Group is not active"` |
| `is_active()` | `false` |
| `get_upcoming_payouts()` | Empty |
| `contribute`, round and other mutating paths | Not gated on `Merged`; most only reject `GroupStatus::Dissolved` |

The last row is a real asymmetry: status checks across the contract are inconsistent, and several operations that only test for `Dissolved` will still run against a merged group. Treat `Merged` as "no new merges/splits and not active" rather than as a universal freeze.

---

## Merge vs. Split

| Aspect | Merge (`propose_merge` …) | Split ([`rosca-group-split.md`](rosca-group-split.md)) |
|---|---|---|
| Direction | Two groups combine into one | One group divides into two |
| Consent | Admin accept (`accepted` flag) | Every member confirms their own assignment |
| Deadline | None | Ledger-based confirmation window, `expire_split_proposal` |
| Member list source | Supplied by admin at completion | Stored on the proposal at creation |
| Payout order | New members appended to the end | Not touched |
| Funds | No transfers | Unconfirmed members refunded pro-rata |
| Resulting status | `Merged` (source) | `Split` (source) |
| Idempotency | Proposal deleted; status guard blocks replay | Proposal marked `Executed` |

The two features cannot be combined on one group: once a group is `Split`, `propose_merge` fails; once it is `Merged`, `propose_group_split` fails.

---

## Error Handling and Edge Cases

| Situation | Behaviour |
|---|---|
| Non-admin calls any merge entry point | `ExtError::OnlyAdminAllowed` |
| Proposal id unknown (propose/accept/complete) | `"Merge proposal not found"` panic |
| `accept_merge` on an already accepted proposal | `"Merge proposal already accepted"` panic |
| `complete_merge` before `accept_merge` | `ExtError2::MigrationNotApproved` (108) |
| `complete_merge` on an already merged group | `"Group already merged"` panic |
| Merge proposed/completed while `PaidMembers` is non-empty | `"Merge only permitted between rounds"` panic |
| Combined members exceed `MaxMembers` | `"Combined member count exceeds max_members"` panic |
| Absorbed member is banned | `ExtError3::MemberBanned` |
| Absorbed member is at `MaxConcurrentMemberships` | `ExtError2::MembershipCapReached` |
| Duplicate address in `new_members` | Appended once; later occurrences skipped |
| Absorbed address already in Group A | Skipped; no new membership count |
| `group_b_id` refers to a non-existent or incompatible group | Accepted without validation |
| Contract paused | All three entry points fail the pause guard |
| Group B admin approval | Not verified on-chain — only Group A's admin is checked |

---

## API Reference

```rust
// Lifecycle
fn propose_merge(env: Env, admin: Address, group_b_id: u32) -> u32;
fn accept_merge(env: Env, admin: Address, merge_proposal_id: u32);
fn complete_merge(env: Env, admin: Address, merge_proposal_id: u32, new_members: Vec<Address>);

// Read
fn get_merge_proposal(env: Env, proposal_id: u32) -> MergeProposal;
fn get_group_status(env: Env) -> GroupStatus;
```

Events:

```rust
MergeProposed  { proposal_id: u32, group_a_admin: Address, group_b_id: u32 }
MergeAccepted  { proposal_id: u32 }
MergeCompleted { proposal_id: u32, members_added: u32 }
GroupMerged    { group_b_id: u32 }
```

---

## Testing

Coverage lives in `contracts/ahjoor-rosca/src/test_group_split.rs` alongside the split tests:

| Test | What it verifies |
|---|---|
| `test_merge_requires_acceptance` | `complete_merge` without `accept_merge` fails with `MigrationNotApproved` |
| `test_complete_merge_double_execution_blocked` | A second `complete_merge` after a successful one panics |
| `test_complete_merge_sets_status_merged` | A completed merge leaves the group `GroupStatus::Merged` |

Not covered by tests: member and payout-order appending, the `MaxMembers` guard, banned-member rejection, membership-cap enforcement, the pause guard, and the absence of any token transfer.

---

## See Also

- [ROSCA Group Split Flow](rosca-group-split.md) — the inverse operation, with per-member confirmation and refunds.
- [Group Dissolution and Emergency Payout Voting in ROSCA](rosca-dissolution-and-emergency-payout.md) — how funds leave a group, if a merge needs to be followed by a settlement.
- [ROSCA Group Charter Acknowledgement](rosca-group-charter.md) — governance and member-facing rules that a merged group inherits from Group A.
