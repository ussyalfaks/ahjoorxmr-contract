# ROSCA Ban List

Each group keeps a ban list. A banned address cannot join or rejoin the group
by any route: being added by the admin, invites, the waitlist, join requests,
being named as a successor, migration, or a group merge. A ban stays in place
until it is lifted.

Source: `contracts/ahjoor-rosca/src/bans.rs`. Tests:
`contracts/ahjoor-rosca/src/test_ban_list.rs`.

## Entry points

| Function | Caller | Description |
|---|---|---|
| `ban_member(group_id, member, reason_hash)` | group admin | Bans `member`. `reason_hash` is a 32-byte hash of the off-chain reason. |
| `unban_member(group_id, member)` | group admin | Lifts the ban. |
| `is_banned(group_id, member) -> bool` | anyone | Whether `member` is currently banned. |

`group_id` is accepted for consistency with other group-scoped calls. Each
contract instance manages a single group. All three calls are blocked while
the contract is paused, except `is_banned`.

## Who can ban

- **The group admin**, directly, with `ban_member`. Only the main admin can do
  this. Co-admin permissions do not cover it.
- **The members**, through governance: a `MemberBan` proposal created with
  `create_proposal` whose `target_member` is the address to ban. If it passes
  under the normal quorum and majority rules (see
  [ROSCA Governance Quorum Requirements](rosca-governance-quorum.md)),
  `execute_proposal` applies the ban. In this case `banned_by` is the
  contract's own address and `reason_hash` is all zeros.

### Rules

| Rule | Error |
|---|---|
| The address is not an active member. **Remove an active member first**; a member who has exited can be banned. | `MemberStillActive` |
| The address is not already banned | `AlreadyBanned` |

Banning is intended for people outside the group or former members. It is not
a way to remove someone. To get rid of a current member, use the removal or
exit flows first, then ban the address.

## What a ban blocks

Every way into the group checks the ban list and fails with `MemberBanned`:

| Path | Function(s) |
|---|---|
| Admin adds a member | `add_member` (and any internal path that adds a member) |
| Invites | `generate_invite` (inviting a banned address), `join_with_invite` |
| Waitlist | `join_waitlist` |
| Join requests | `request_to_join` |
| Succession | `designate_successor` (a banned address cannot be named as successor) |
| Migration | `approve_migration_entry`, `execute_migration` |
| Group merge | `complete_merge` (rejects the merge if any incoming member is banned) |

### Clean-up when the ban is applied

Banning also removes any existing ways back in:

- **Waitlist:** if the address is on the waitlist, it is removed and a
  `WaitlistUpdated` event is emitted.
- **Invites:** if the address has an approved invite that has not been used,
  the invite is removed.

A ban does **not** affect anything already settled. Past contributions,
payout history and penalty records stay as they were.

## Unbanning

`unban_member` removes the address from the ban list. If the address is not
banned, it fails with `NotBanned`.

Unbanning does not bring back anything the ban removed. The address is not
put back on the waitlist and its old invite is not restored. It has to join
again through one of the normal routes, which are then subject to their usual
rules.

There is no governance proposal type for unbanning. Only the admin can lift a
ban, including a ban the members voted for.

## Events

| Event | When |
|---|---|
| `MemberBanned(member, banned_by, reason_hash)` | A ban is applied. `banned_by` is the admin, or the contract address for a governance ban. |
| `MemberUnbanned(member, unbanned_by)` | The admin lifts a ban. |
| `WaitlistUpdated(member, false, new_len)` | The banned address was removed from the waitlist. |

## Errors

| Error | Code | Raised by |
|---|---|---|
| `MemberBanned` | 175 | Any join path when the address is banned |
| `MemberStillActive` | 176 | `ban_member` / `MemberBan` execution when the address is an active member |
| `AlreadyBanned` | 177 | `ban_member` / `MemberBan` execution when the address is already banned |
| `NotBanned` | 178 | `unban_member` when the address is not banned |
