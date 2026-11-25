# Vouchers, External IDs and Multisig Payment Approval in `ahjoor-payments`

> **Status:** Implemented in `contracts/ahjoor-payments/src/lib.rs`
> (`issue_voucher`, `revoke_voucher`, `get_voucher`, `create_payment_with_voucher`, `get_payment_by_external_id`,
> `set_multisig_policy`, `get_multisig_policy`, `approve_payment`, `get_approval_state`, `expire_pending_approval`)
> and tested in `contracts/ahjoor-payments/src/test_external_id_multisig_voucher.rs`.

---

## Overview

All three features meet in one entry point, `create_payment_with_voucher`:

- **Vouchers** — merchants issue discount codes on-chain. A customer passes the code's hash when paying and the
  discount is taken off the amount they transfer.
- **External IDs** — a payment can carry a merchant-chosen 32-byte ID (for example an off-chain order ID). The
  merchant can later look the payment up by that ID.
- **Multisig approval** — a merchant can require `m` of `n` co-signers to approve any payment at or above a
  threshold before it becomes a normal `Pending` payment.

| Fact | Value |
| --- | --- |
| Payment entry point | `create_payment_with_voucher` |
| Voucher entry points | `issue_voucher`, `revoke_voucher`, `get_voucher` |
| External ID lookup | `get_payment_by_external_id` |
| Multisig entry points | `set_multisig_policy`, `approve_payment`, `expire_pending_approval` |
| Multisig read entry points | `get_multisig_policy`, `get_approval_state` |
| Storage keys | `DataKey2::Voucher(merchant, code_hash)`, `DataKey::ExternalIdIndex(merchant, external_id)`, `DataKey::MultisigPolicy(merchant)`, `DataKey2::ApprovalState(payment_id)` |

### Errors

| Error | Code | Raised when |
| --- | --- | --- |
| `Error::DuplicateExternalId` | `#6` | The merchant already has a payment with this `external_id` |
| `Error::AlreadyApproved` | `#8` | A signer approves the same payment twice |
| `Error::NotASigner` | `#9` | The approver is not in the merchant's policy |
| `Error::VoucherExpired` | `#10` | The voucher's `expiry` has passed |
| `Error::VoucherExhausted` | `#11` | A limited voucher has no uses left |
| `Error::VoucherRevoked` | `#12` | The merchant revoked the voucher |

---

## Creating a Payment

```rust
pub fn create_payment_with_voucher(
    env: Env,
    customer: Address,
    merchant: Address,
    amount: i128,
    token: Address,
    reference: Option<String>,
    metadata: Option<Map<String, String>>,
    idempotency_key: Option<BytesN<32>>,
    voucher_code_hash: Option<BytesN<32>>,
    external_id: Option<BytesN<32>>,
) -> u32
```

The customer must authorize. The steps run in this order:

1. If `idempotency_key` was already used, the existing payment ID is returned and nothing else happens.
2. Customer rate limit, positive `amount`, reference/metadata, accepted-token and merchant-approval checks.
3. **External ID uniqueness** check (see below).
4. **Voucher** validation and discount (see below). The voucher use is consumed here.
5. The customer transfers the discounted amount (`effective_amount`) into escrow.
6. **Multisig gate:** if the merchant has a policy and `effective_amount >= policy.threshold`, the payment is
   created as `PendingApproval`; otherwise it is `Pending`.
7. The external ID index and, for `PendingApproval`, an empty `ApprovalState` are stored.

Because the whole call reverts on any panic, a failed payment does not consume a voucher use or reserve an
external ID.

---

## Vouchers

### Issuing

```rust
pub fn issue_voucher(
    env: Env,
    merchant: Address,
    code_hash: BytesN<32>,
    discount_type: DiscountType,
    discount_value: u32,
    max_uses: u32,
    expiry: u64,
)
```

- **Auth:** `merchant.require_auth()`. Fails if the contract is paused.
- `code_hash` is the sha256 of the promo code. Only the hash is stored, so the plain code is not visible on-chain
  before it is used.
- `discount_type`:
  - `DiscountType::Fixed` — `discount_value` token units off the amount.
  - `DiscountType::Percentage` — `discount_value` percent off (`0`–`100`, values above `100` panic).
- `discount_value` must be `> 0`.
- A merchant cannot issue two vouchers with the same `code_hash` (`"Voucher with this code_hash already exists"`).
  Different merchants can use the same hash, since vouchers are keyed by `(merchant, code_hash)`.
- Emits `VoucherIssued (merchant, code_hash, discount_value, max_uses, expiry)`.

### `max_uses`

- `max_uses = 0` means **unlimited** redemptions.
- `max_uses > 0` sets `uses_remaining = max_uses`. Every successful `create_payment_with_voucher` decrements it by
  one. When it reaches `0`, `VoucherExhausted (merchant, code_hash)` is emitted, and any later redemption fails with
  `Error::VoucherExhausted` (`#11`).
- Uses are counted across all customers, not per customer.

### `expiry`

- `expiry` is a ledger timestamp in seconds. `expiry = 0` means the voucher **never expires**.
- A redemption fails with `Error::VoucherExpired` (`#10`) when `now > expiry`. Redeeming exactly at `expiry` is
  still allowed.
- `issue_voucher` does not check that `expiry` is in the future.

### Redemption

The checks run in this order: not revoked (`#12`), not expired (`#10`), not exhausted (`#11`). The discount is:

```text
Fixed:       discount = discount_value
Percentage:  discount = amount * discount_value / 100   (integer division)
effective_amount = max(amount - discount, 0)
```

If `effective_amount` is `0` (for example a 100% voucher or a fixed discount larger than the amount), the call
panics with `"Effective payment amount after discount must be positive"`. The stored payment's `amount` is the
discounted `effective_amount`. Emits `VoucherRedeemed (merchant, code_hash, customer, discount)`.

### Revoking and reading

- `revoke_voucher(merchant, code_hash)` — merchant only. Sets `revoked = true` immediately and emits
  `VoucherRevoked (merchant, code_hash)`. Revoking twice panics with `"Voucher already revoked"`. There is no
  un-revoke.
- `get_voucher(merchant, code_hash) -> Voucher` returns `merchant`, `discount_type`, `discount_value`, `max_uses`,
  `uses_remaining`, `expiry` and `revoked`. Panics with `"Voucher not found"` if missing.

---

## External IDs

- `external_id` is an optional `BytesN<32>` passed to `create_payment_with_voucher`. It is stored on the
  `Payment` and indexed under `DataKey::ExternalIdIndex(merchant, external_id)`.
- **Uniqueness is per merchant.** If the same merchant already has a payment with this ID, the call fails with
  `Error::DuplicateExternalId` (`#6`). Two different merchants can use the same ID.
- The index is never removed, so an ID stays taken even after the payment is refunded, expired or cancelled.
- Payments created without an `external_id` are not indexed. Other payment entry points do not set an external ID.
- When indexed, `PmtIndexedExtId (payment_id, external_id)` is emitted.

```rust
pub fn get_payment_by_external_id(env: Env, merchant: Address, external_id: BytesN<32>) -> Payment
```

Returns the full `Payment`. Panics with `"No payment found for this external_id"` if the merchant has no payment
with that ID.

---

## Multisig Payment Approval

### Policy

```rust
pub fn set_multisig_policy(
    env: Env,
    merchant: Address,
    threshold: i128,
    signers: Vec<Address>,
    m: u32,
    approval_window_seconds: u64,
)
```

- **Auth:** `merchant.require_auth()`. Fails if the contract is paused.
- `threshold > 0`: payments whose discounted amount is `>= threshold` need approval.
- `signers` must not be empty, and `1 <= m <= signers.len()`.
- `approval_window_seconds > 0`: how long signers have to approve, counted from the payment's creation.
- Calling it again replaces the policy. `get_multisig_policy(merchant)` returns `None` when no policy is set.

### Approving

```rust
pub fn approve_payment(env: Env, signer: Address, payment_id: u32)
```

- **Auth:** `signer.require_auth()`. Fails if the contract is paused.
- The payment must be `PendingApproval` (`"Payment is not pending approval"`).
- `signer` must be in the merchant's current policy (`Error::NotASigner`, `#9`) and must not have approved already
  (`Error::AlreadyApproved`, `#8`).
- Each approval is appended to `ApprovalState.approvals` and emits `PmtApproved (payment_id, signer)`.
- When `approvals.len() >= m`, the payment moves to `Pending` and a status-changed event is emitted. From there it
  follows the normal payment lifecycle.
- If the window has passed (`now > created_at + approval_window_seconds`), the call does **not** approve: it
  refunds the full amount to the customer, sets the payment to `Refunded` and emits `PmtApprovalExp (payment_id)`.

`get_approval_state(payment_id)` returns `{ payment_id, approvals, created_at }`, or `None` for payments that never
needed approval.

### Expiring

```rust
pub fn expire_pending_approval(env: Env, payment_id: u32)
```

Anyone can call this once the window has passed. It refunds the customer, sets the payment to `Refunded` and emits
`PmtApprovalExp`. It panics with `"Approval window has not expired yet"` if called at or before
`created_at + approval_window_seconds`.

### Notes

- The policy is read live, so changing `signers`, `m` or `approval_window_seconds` affects payments that are
  already `PendingApproval`.
- Only `create_payment_with_voucher` applies the multisig gate.

---

## Events

| Event | Emitted by | Payload |
| --- | --- | --- |
| `VoucherIssued` | `issue_voucher` | `(merchant, code_hash, discount_value, max_uses, expiry)` |
| `VoucherRedeemed` | `create_payment_with_voucher` | `(merchant, code_hash, customer, discount)` |
| `VoucherExhausted` | `create_payment_with_voucher` (last use) | `(merchant, code_hash)` |
| `VoucherRevoked` | `revoke_voucher` | `(merchant, code_hash)` |
| `PmtIndexedExtId` | `create_payment_with_voucher` | `(payment_id, external_id)` |
| `PmtApproved` | `approve_payment` | `(payment_id, signer)` |
| `PmtApprovalExp` | `approve_payment` (after window), `expire_pending_approval` | `(payment_id,)` |
