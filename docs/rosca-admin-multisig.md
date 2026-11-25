# ROSCA Admin Multi-Sig

> **Status:** Implemented in `contracts/ahjoor-rosca/src/lib.rs`
> (`init_multisig`, `propose_admin_action`, `approve_admin_action`,
> `get_multisig_config`).

---

## Overview

A ROSCA group can require more than one signer for a small set of critical
admin actions. The group admin registers a list of co-admins and a threshold.
When the threshold is `1` the admin acts alone; when it is greater than `1`
the admin's proposal only announces the action, and a co-admin (or the admin)
must approve it before it runs.

| Fact | Value |
| --- | --- |
| Configure | `init_multisig(co_admins, threshold)` (admin only) |
| Propose | `propose_admin_action(action_type, target_member, payload)` (admin only) |
| Approve / execute | `approve_admin_action(approver, action_type, target_member, payload)` |
| Read config | `get_multisig_config() -> (Vec<Address>, u32)` |
| Default threshold | `1` (no multi-sig) |
| Storage | Temporary storage keys `co_admins` and `multisig_threshold` |

---

## Covered Actions

Only three actions go through the multi-sig flow. They are selected with the
numeric `action_type` argument:

| `action_type` | Action | Required arguments | What runs |
| --- | --- | --- | --- |
| `0` | `RemoveMember` | `target_member: Some(member)` | `remove_member(member)` |
| `1` | `PenaliseDefaulter` | `target_member: Some(member)` | Applies the group's configured `PenaltyAmount` to `member` for the current round |
| `2` | `UpdateFee` | `payload: Some(fee_bps)` | `update_fee(fee_bps)` (max 500 bps) |

Any other `action_type` panics with `"Invalid action type"`. If the required
argument is `None`, the call succeeds but does nothing.

The underlying entry points keep their own checks. `remove_member` and
`update_fee` still require the group admin's authorization, and the penalty
transfer requires the penalised member's authorization. So the multi-sig adds
signers on top of the existing checks. It does not remove any of them.

Every other admin entry point (round finalization, config setters, pausing,
and so on) is **not** covered and is still called directly by the admin.

---

## Threshold Rules

`init_multisig` is admin-only and validates the threshold against the number
of signers, where the admin counts as one signer:

```text
1 <= threshold <= co_admins.len() + 1
```

Anything outside that range panics with `"Invalid multisig threshold"`.

- **`threshold == 1`** — single-admin mode. `propose_admin_action` executes the
  action immediately.
- **`threshold > 1`** — multi-sig mode. `propose_admin_action` only emits an
  event; the action runs on `approve_admin_action`.
- Calling `init_multisig` again replaces both the co-admin list and the
  threshold.
- Approvers must be in `co_admins` or be the admin, otherwise the call fails
  with `NotACoAdmin`.
- The configuration lives in **temporary** storage. If its TTL lapses,
  `get_multisig_config` returns `([], 1)` and the group falls back to
  single-admin mode until `init_multisig` is called again.

> **Current behaviour:** the threshold only controls whether a proposal runs
> immediately (`1`) or waits for approval (`> 1`). Approvals are not counted
> against the threshold: one valid `approve_admin_action` call runs the
> action, whatever the threshold is.

---

## Proposal Lifecycle

```text
                 propose_admin_action (admin)
                            │
          ┌─────────────────┴──────────────────┐
   threshold == 1                        threshold > 1
          │                                    │
   action executes                   AdminActionProposed event
   immediately                      (action_id = 0, action name, admin)
                                               │
                              approve_admin_action (co-admin or admin)
                                               │
                                       action executes
                                               │
                                  AdminActionExecuted event
                                  (action_id = 0, action name)
```

1. **Configure.** The admin calls `init_multisig(co_admins, threshold)`.
2. **Propose.** The admin calls `propose_admin_action(action_type,
   target_member, payload)`. With a threshold above `1`, the contract emits
   `AdminActionProposed` with the action name (`RemoveMember`,
   `PenaliseDefaulter` or `UpdateFee`) and the proposing admin.
3. **Approve and execute.** A co-admin calls `approve_admin_action` with the
   same `action_type`, `target_member` and `payload`. The contract checks the
   approver, runs the action and emits `AdminActionExecuted`.

Proposals are **not stored on-chain**. There is no proposal ID (`action_id` is
always `0`), no record linking an approval to an earlier proposal, and no
expiry or cancellation. The approver sends the action parameters again, so
off-chain tooling should take them from the `AdminActionProposed` event and the
proposer's request. `approve_admin_action` does not check that a matching
proposal was made.

Both `propose_admin_action` and `approve_admin_action` are blocked while the
contract is paused.

---

## Reading the Configuration

```rust
pub fn get_multisig_config(env: Env) -> (Vec<Address>, u32)
```

Returns `(co_admins, threshold)`. If no configuration is stored, it returns an
empty list and a threshold of `1`.
