# Contribution Delegation in ROSCA

This document explains the contribution and voting delegation mechanisms in the `ahjoor-rosca` contract. These features allow a ROSCA member to authorize a proxy address to perform actions—specifically contributing and voting—on their behalf.

---

## 1. How Delegation is Granted

A member can delegate authority by calling one of the following methods, depending on the scope of delegation required:

### General Contribution Delegation
```rust
delegate_contribution_rights(member: Address, group_id: u32, proxy: Address, expiry_ledger: u64)
```
- **Authentication:** The transaction must be signed and authorized by the delegating `member` (`require_auth()`).
- **Membership Check:** The `member` must be a valid, registered member of the ROSCA group.
- **Constraints:**
  - `expiry_ledger` cannot be `0` (infinite delegation is not allowed).
  - `expiry_ledger` must be in the future. Depending on the contract configuration (`UseTimestampSchedule`), this is measured either as a ledger sequence number or a Unix timestamp.

### Contribution-Weight Voting Delegation
```rust
delegate_contribution_vote(delegator: Address, delegate: Address, expiry_ledger: u64)
```
- **Authentication:** Must be signed and authorized by the `delegator`.
- **Membership Check:** Both `delegator` and `delegate` must be active, registered members of the ROSCA group.
- **Constraints:** Matches the expiration constraints of general delegation.

> [!NOTE]
> Since both contribution rights and voting weight delegation write to the same storage space (`DataKey3::ContribDelegations`), only one proxy/delegate can be active per member at any time.

---

## 2. Scope & Limits of Delegated Authority

Once delegation is active and before it expires, the designated proxy has the authority to execute actions on behalf of the member.

### Supported Proxy Actions

#### 1. Contribution on Behalf of Member
```rust
contribute_via_proxy(proxy: Address, member: Address, token: Address, amount: i128)
```
- The `proxy` authorizes the transaction.
- Tokens are transferred from the `member`'s account using `transfer_from`.
- **Pre-requisite:** The delegating `member` must pre-approve the `proxy` to spend their tokens (i.e. set a token allowance for `proxy` covering the contribution amount), or have approved the contract directly.

#### 2. Voting on Governance Proposals
```rust
vote_proposal_via_proxy(proxy: Address, member: Address, proposal_id: u32, approve: bool)
```
- The `proxy` authorizes the transaction to vote on the given proposal on behalf of the `member`.

#### 3. Automatic Voting Weight Aggregation
- When contribution-weighted voting is active, if the delegate votes directly on a proposal (via `vote_on_proposal`), the voting weight of the delegating member is automatically aggregated and cast along with the delegate's vote (provided the delegator has not already voted).

### Constraints & Safeguards
- **Direct Action Restriction:** To prevent conflicting actions, a member with an active contribution-weighted delegation is blocked from voting directly. Attempting to call `vote_on_proposal` directly will revert with the error `CannotVoteWithActiveDelegation`.
- **Exclusivity:** Registering a new proxy replaces the old one.
- **Expiration Enforcement:** If a proxy attempts to act after the delegation has expired, the transaction will fail.
- **Auto-Revocation for Storage Reclamation:** If `contribute_via_proxy` is called with an expired delegation, the contract automatically removes the expired delegation record from persistent storage to reclaim space, emits a `proxy_expired` event, and reverts with `DelegationExpired`.

---

## 3. How Delegation is Revoked

Delegation can be revoked explicitly or implicitly:

### Explicit Revocation
A member can cancel active delegation at any time by calling:
- `revoke_contribution_delegation(member: Address, group_id: u32)` for general rights.
- `revoke_contrib_vote_delegation(delegator: Address)` for voting rights.

Both methods require the authorizing signature of the delegating member and remove the record from contract storage.

### Implicit Revocation & Overwriting
- **Replacement:** Call `delegate_contribution_rights` or `delegate_contribution_vote` with a new address to overwrite and replace the old proxy.
- **Expiration Cleanup:** Expired delegations are automatically pruned from storage during transaction execution on `contribute_via_proxy` once the expiration threshold has passed.
