# Escrow Amendments and Deadline Extensions

> **Status:** Implemented in `contracts/ahjoor-escrow/src/lib.rs`
> (`propose_amendment`, `sign_amendment`, `apply_amendment`,
> `cancel_amendment_proposal`, `propose_deadline_extension`,
> `accept_deadline_extension`, `extend_deadline`,
> `set_max_deadline_extension`, `get_max_deadline_extension`).

---

## Overview

An escrow's terms can change after creation in three ways. They overlap
(all three can move the deadline), so pick the one that matches what you
want to change and who has to agree:

| Flow | Changes | Who must agree | Deadline rule | Proposal lifetime |
| --- | --- | --- | --- | --- |
| **Amendment** (`propose_amendment` → `sign_amendment`) | Amount, deadline and/or metadata hash | Buyer **and** seller | Must be in the future (may be earlier or later than the current one) | 7 days |
| **Deadline extension proposal** (`propose_deadline_extension` → `accept_deadline_extension`) | Deadline only | Proposer + counterparty | Must be later than the current deadline | 24 hours |
| **Buyer extension** (`extend_deadline`) | Deadline only | Buyer alone | Later than the current deadline, capped by `set_max_deadline_extension` | None (applies immediately) |

### When to use each

- **The buyer just wants to give the seller more time.** Use `extend_deadline`.
  A later deadline only helps the seller, so the seller's consent is not
  needed. It applies at once, up to the configured cap.
- **The seller needs more time, or the buyer wants to extend past the cap.**
  Use `propose_deadline_extension`, and have the other party call
  `accept_deadline_extension` within 24 hours. There is no cap on this path
  because both parties agree.
- **Anything other than a later deadline**: a new amount, new metadata, or an
  *earlier* deadline. Use an amendment. Both parties sign, and an amount change
  moves funds.

---

## Amendments

```rust
pub struct AmendmentProposal {
    pub nonce: u32,
    pub proposer: Address,
    pub new_amount: Option<i128>,
    pub new_deadline: Option<u64>,
    pub new_metadata_hash: Option<BytesN<32>>,
    pub proposed_at: u64,
    pub expires_at: u64,
    pub buyer_signed: bool,
    pub seller_signed: bool,
}
```

Each field set to `None` keeps the current value. Only one amendment proposal
is stored per escrow (`DataKey2::AmendmentProposal(escrow_id)`). A new
proposal replaces the old one.

### `propose_amendment`

```rust
pub fn propose_amendment(env: Env, caller: Address, escrow_id: u32,
    new_amount: Option<i128>, new_deadline: Option<u64>,
    new_metadata_hash: Option<BytesN<32>>) -> u32
```

- The caller must be the buyer or the seller
  (`OnlyBuyerOrSellerCanProposeAmendment`).
- The escrow must not be terminal (`Released`, `Resolved`, `Refunded`,
  `Abandoned`), which gives `CannotAmendTerminalEscrow`. Disputed escrows can
  be amended.
- `new_deadline`, if set, must be in the future (`InvalidDeadline`).
- `new_amount`, if set, must be positive (`NewAmountMustBePositive`).
- The proposal counts as signed by the proposer, and it expires 7 days after
  creation (`DEFAULT_AMENDMENT_EXPIRY_SECONDS`).
- Returns the proposal `nonce`, which must be passed to the later calls, and
  emits `AmendmentProposed`.

### `sign_amendment`

```rust
pub fn sign_amendment(env: Env, caller: Address, escrow_id: u32, nonce: u32)
```

The buyer or seller signs the pending proposal. The `nonce` must match
(`AmendmentNonceMismatch`), and the call fails with
`AmendmentProposalHasExpired` once the proposal has expired. Emits
`AmendmentSigned`. When both parties have signed, `apply_amendment` runs
**automatically** in the same call.

### `apply_amendment`

```rust
pub fn apply_amendment(env: Env, escrow_id: u32, nonce: u32)
```

Applies a fully signed, unexpired proposal. You normally don't need to call it,
because `sign_amendment` already does. It is public so that anyone can apply
a proposal that both parties have signed. It fails with
`AmendmentRequiresBuyerAndSellerSignatures` until both have signed.

**Effect:**
- **Amount up:** the difference is transferred from the buyer into the
  escrow.
- **Amount down:** the difference is refunded to the buyer(s).
- The deadline and metadata hash are replaced if they were set.
- The proposal is removed and `AmendmentApplied` is emitted with the old and
  new values.

### `cancel_amendment_proposal`

```rust
pub fn cancel_amendment_proposal(env: Env, caller: Address, escrow_id: u32, nonce: u32)
```

The buyer, the seller or the contract admin can withdraw a pending proposal
before it is applied, as long as the escrow is not terminal. The `nonce` must
match. Emits `AmendmentCancelled`.

---

## Deadline Extension Proposals

### `propose_deadline_extension`

```rust
pub fn propose_deadline_extension(env: Env, caller: Address, escrow_id: u32, new_deadline: u64)
```

- The caller must be the buyer or the seller.
- The escrow must not be `Disputed` or terminal
  (`CannotExtendDeadlineWhileEscrowIsDisputed`).
- `new_deadline` must be in the future and later than the current deadline.
- Stores the proposal in `DataKey::DeadlineProposal(escrow_id)`, replacing any
  earlier one, and emits `DeadlineExtensionProposed`.

### `accept_deadline_extension`

```rust
pub fn accept_deadline_extension(env: Env, caller: Address, escrow_id: u32)
```

- The caller must be the buyer or the seller, and **not** the proposer
  (`ProposerCannotAcceptTheirOwnDeadlineExtension`).
- The proposal must be accepted within 24 hours of being made
  (`DEADLINE_EXTENSION_PROPOSAL_WINDOW`). After that it fails with
  `DeadlineExtensionProposalHasExpired` and the proposal is deleted.
- The proposed deadline is checked again. It must still be in the future and
  later than the current deadline.
- Updates the deadline, removes the proposal and emits `DeadlineExtended`.

The 24-hour window is a constant. It cannot be configured.

---

## Buyer-Only Extension

### `extend_deadline`

```rust
pub fn extend_deadline(env: Env, buyer: Address, escrow_id: u32, new_deadline: u64)
```

- The caller must be the buyer (`OnlyBuyerCanExtendDeadline`).
- The escrow must not be terminal (`EscrowAlreadyTerminal`), `Disputed` or
  `PartiallyDisputed` (`CannotExtendDeadlineDuringDispute`).
- `new_deadline` must be in the future and later than the current deadline
  (`NewDeadlineMustBeInFuture`).
- `new_deadline - current_deadline` must not exceed the configured cap
  (`DeadlineExtensionExceedsCap`).
- Applies immediately and emits `DeadlineExtendedByBuyer`.

The cap applies to **each call**, measured from the current deadline. A buyer
can extend again later, and each call is checked against the cap separately.

### `set_max_deadline_extension`

```rust
pub fn set_max_deadline_extension(env: Env, admin: Address, max_seconds: u64)
pub fn get_max_deadline_extension(env: Env) -> u64
```

- Admin only, and blocked while the contract is paused.
- Sets the contract-wide maximum, in seconds, that one `extend_deadline` call
  may add. Stored in `DataKey3::MaxDeadlineExtensionSeconds`.
- `max_seconds` must be greater than zero (`TimeoutMustBePositive`).
- The default is 30 days (`DEFAULT_MAX_DEADLINE_EXTENSION_SECONDS`).
- The cap only limits `extend_deadline`. Amendments and accepted deadline
  extension proposals are not capped.
