# Pre-Approved Spending

This document describes the pre-approved spending functionality in the `contracts\ahjoor-payments`; detailing how and who grants an allowance, how it is consumed/ decremented on payment, and how it is revoked or expires.

## Overview

The Pre-Approved Spending smart contract lets a **customer** authorize a **merchant** to draw funds up to a predefined limit, without needing the customer to approve each individual payment. The authorization is represented on-chain as a `SpendingAllowance`, backed by a `ConsentRecord` that provides verifiable proof of the customer granted permission.

A pre-approved allowance specifies:

- The **customer** granting authorization.
- The **merchant** permitted to spend.
- The **token** that can be spent.
- The maximum **total amount** available.
- **Per-transaction** and **daily** spending limits.
- An **expiration timestamp**.
- The **consent record** associated with the authorization.

Once created, the merchant can draw against the allowance until it is either paused, revoked, expired, or fully exhausted.

---

## Allowance Lifecycle

### 1. Granting an Allowance — who can grant it

**Only the customer can create an allowance.** During creation, the contract requires the customer's on-chain authorization:

```rust
customer.require_auth();
```

This guarantees that no third party — including the merchant who benefits from the allowance — can create or backdate an allowance on the customer's behalf.

```rust
pub fn create_allowance(
    customer,
    merchant,
    token,
    total_amount,
    per_transaction_limit,
    daily_limit,
    expires_at,
    consent_hash,
    consent_metadata
)
```

**Validation performed before creation succeeds:**

- `total_amount` must be greater than zero.
- `per_transaction_limit` must be greater than zero.
- `daily_limit` must be greater than zero.
- Neither `per_transaction_limit` nor `daily_limit` may exceed `total_amount`.
- `expires_at` must be in the future.

If any check fails, the call aborts with `InvalidAllowanceAmount` or `AllowanceExpired`.

**On successful creation, the allowance:**

- is assigned a unique `allowance_id`;
- is initialized with `status = Active`, `amount_spent = 0`, `daily_spent = 0`;
- records the creation timestamp and associated consent info;
- is indexed under both the customer's and the merchant's allowance lists (queryable via `get_customer_allowances` / `get_merchant_allowances`); and
- generates a `Created` audit log entry.

| Field | Purpose |
|---|---|
| `customer` | Account granting permission |
| `merchant` | Authorized spender |
| `token` | Asset that may be spent |
| `total_amount` | Maximum spendable amount |
| `amount_spent` | Amount already consumed |
| `per_transaction_limit` | Max spend allowed in a single payment |
| `daily_limit` | Max cumulative daily spend |
| `daily_spent` | Amount spent in the current daily window |
| `expires_at` | Automatic expiration timestamp |
| `status` | Current allowance state |
| `consent_hash` | Reference to the recorded consent |

### 2. Recording Consent

Customer consent is recorded separately, via:

```rust
pub fn record_consent(
    customer,
    merchant,
    consent_type,
    consent_hash,
    ip_hash,
    device_hash,
    location_hash,
    expires_at,
    metadata
)
```

Like allowance creation, this **requires customer authorization**:

```rust
customer.require_auth();
```

The contract verifies the consent's `expires_at` has not yet passed before persisting it. A `ConsentRecord` stores:

- customer and merchant addresses;
- consent type (`SpendingAllowance`, `RecurringPayment`, `DataSharing`, `Marketing`);
- consent hash and consent timestamp;
- expiration timestamp;
- hashed IP address, device fingerprint, and location; and
- additional metadata.

The record begins in the `Active` state and can later be checked with `verify_consent()` or withdrawn with `revoke_consent()`, independently of any allowance built on top of it.

---

## Spending From an Allowance (Consumption / Decrement)

The merchant draws from the allowance via:

```rust
pub fn spend_from_allowance(
    allowance_id,
    amount,
    reference
)
```

**Before a payment is approved, the contract checks that:**

- the allowance exists (`AllowanceNotFound` otherwise);
- the allowance is not `Paused` (`AllowancePaused`);
- the allowance has not been `Revoked` (`AllowanceRevoked`);
- the allowance has not already been marked `Expired` (`AllowanceExpired`);
- the allowance has not already been marked `Exhausted` (`AllowanceExhausted`);
- the allowance's `expires_at` has not passed as of the current ledger time (if it has, the status is transitioned to `Expired` and the call fails with `AllowanceExpired`);
- the requested `amount` does not exceed `per_transaction_limit` (`PerTransactionLimitExceeded`);
- the daily limit has not been exceeded (`DailyLimitExceeded`); and
- sufficient balance remains in the allowance (`TransactionExceedsLimit`).

If any check fails, the corresponding `SpendingAllowanceError` is raised and the transaction is declined.

### Daily Spending Reset

The contract automatically resets tracked daily spending on a rolling 24-hour basis. If more than 24 hours have passed since `daily_reset_timestamp`, the contract:

- resets `daily_spent` to `0`; and
- updates `daily_reset_timestamp` to the current ledger time.

This opens a fresh daily spending window without touching the overall `total_amount` / `amount_spent` balance.

### Decrement Logic

The allowance itself is never given a separate "balance" field to draw down — instead, the contract keeps a running total of what has been spent, and remaining capacity is always derived by subtracting that total from the original limits. This means `total_amount` and `daily_limit` stay fixed throughout, while `amount_spent` and `daily_spent` grow with each approved payment.

Once all the checks above pass, the contract updates both counters by the payment amount:
```
amount_spent += payment_amount
daily_spent  += payment_amount
```

A `Completed` `AllowanceTransaction` is recorded, and a `TransactionApproved` audit log entry is generated.

The remaining spendable balance is:

```
remaining_balance = total_amount - amount_spent
```

and the remaining daily capacity is:

```
daily_remaining = daily_limit - daily_spent
```

These can be queried directly via `get_remaining_balance(allowance_id)` and `get_daily_remaining(allowance_id)`.

**Auto-exhaustion:** if `amount_spent` reaches `total_amount`, the contract automatically transitions the allowance's status to `Exhausted`, blocking any further spending even before `expires_at` is reached.

---

## Revocation

### Revoking an Allowance

**Only the customer who owns the allowance may revoke it.** The contract enforces this with:

```rust
allowance.customer.require_auth();
```

```rust
pub fn revoke_allowance(allowance_id)
```

On revocation:

- `status` changes to `Revoked`;
- the updated allowance is persisted on-chain; and
- a `Revoked` audit log entry is recorded.

Any subsequent spend attempt fails with `AllowanceRevoked`. The allowance record itself is retained (not deleted) for historical and audit purposes.

### Revoking Consent

Consent can be withdrawn independently of any allowance:

```rust
pub fn revoke_consent(consent_id)
```

Only the customer who originally granted the consent may revoke it. This sets the consent's status to `Revoked`, so `verify_consent()` will no longer treat it as valid authorization.

### Pausing / Resuming (temporary suspension)

Spending can be temporarily disabled without permanently revoking the allowance:

```rust
pub fn pause_allowance(allowance_id)   // status -> Paused
pub fn resume_allowance(allowance_id)  // status -> Active
```

Both actions generate corresponding `Paused` / `Resumed` audit log entries.

---

## Expiration

Every allowance carries `expires_at`. On each `spend_from_allowance` call, the contract compares the current ledger timestamp against this value. If the allowance has expired:

- `status` is updated to `Expired`;
- the updated state is persisted on-chain; and
- the call fails with `AllowanceExpired`.

This guarantees an expired allowance cannot authorize further spending even if it was previously `Active`. Consent records carry their own independent `expires_at` and can be checked via `verify_consent(consent_id)`.

---

## Allowance States

| Status | Description |
|---|---|
| `Active` | Allowance is valid and may be used for spending. |
| `Paused` | Spending is temporarily disabled until resumed. |
| `Revoked` | Authorization has been permanently withdrawn by the customer. |
| `Expired` | The expiration timestamp has passed. |
| `Exhausted` | The total approved amount has been fully spent. |

---

## Public Functions

### Allowance Management

| Function | Auth required | Description |
|---|---|---|
| `create_allowance(...)` | customer | Creates a new spending allowance backed by customer consent. |
| `pause_allowance(allowance_id)` | customer | Temporarily suspends spending. |
| `resume_allowance(allowance_id)` | customer | Reactivates a paused allowance. |
| `revoke_allowance(allowance_id)` | customer | Permanently revokes an allowance. |
| `update_allowance_limits(allowance_id, per_transaction_limit, daily_limit)` | customer | Updates the per-transaction and daily spending limits. |

---

### Consent Management

| Function | Auth required | Description |
|---|---|---|
| `record_consent(...)` | customer | Records customer authorization for future spending. |
| `revoke_consent(consent_id)` | customer | Revokes a previously recorded consent. |
| `verify_consent(consent_id)` | — (read-only) | Checks whether a consent record is still valid. |
| `get_consent(consent_id)` | — (read-only) | Retrieves a stored consent record. |

---

### Spending

| Function | Auth required | Description |
|---|---|---|
| `spend_from_allowance(allowance_id, amount, reference)` | merchant | Executes a payment against the remaining allowance balance, subject to all validation and limit checks. |
---

### Queries (read-only)

| Function | Description |
|---|---|
| `get_allowance(allowance_id)` | Retrieves allowance details. |
| `get_remaining_balance(allowance_id)` | Returns `total_amount - amount_spent`. |
| `get_daily_remaining(allowance_id)` | Returns `daily_limit - daily_spent`. |
| `get_allowance_transactions(allowance_id)` | Returns all transactions executed against an allowance. |
| `get_audit_log(allowance_id)` | Returns the audit history for an allowance. |
| `get_customer_allowances(customer)` | Lists all allowances created by a customer. |
| `get_merchant_allowances(merchant)` | Lists all allowances granted to a merchant. |

---

## Error Reference

| Error | Raised when |
|---|---|
| `AllowanceNotFound` | Referenced `allowance_id` does not exist. |
| `AllowanceExpired` | `expires_at` has passed (allowance or consent). |
| `AllowanceExhausted` | `amount_spent` has reached `total_amount`. |
| `AllowanceRevoked` | Allowance status is `Revoked`. |
| `TransactionExceedsLimit` | Payment would exceed the remaining allowance balance. |
| `DailyLimitExceeded` | Payment would exceed the remaining daily limit. |
| `PerTransactionLimitExceeded` | Payment amount exceeds `per_transaction_limit`. |
| `UnauthorizedAccess` | Caller is not authorized to perform the action. |
| `ConsentNotFound` | Referenced `consent_id` does not exist. |
| `ConsentExpired` | Consent's `expires_at` has passed. |
| `ConsentRevoked` | Consent status is `Revoked`. |
| `InvalidConsentHash` | Consent hash does not match expected value. |
| `AllowancePaused` | Allowance status is `Paused`. |
| `InvalidAllowanceAmount` | `total_amount`, `per_transaction_limit`, or `daily_limit` fails validation at creation. |

---

## References

`contracts\ahjoor-payments\src\pre_approved_spending.rs` 

`contracts\ahjoor-payments\src\pre_approved_spending_impl.rs`



 