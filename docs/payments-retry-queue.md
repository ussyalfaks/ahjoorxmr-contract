# Failed Auto-Debit Retry Queue in `ahjoor-payments`

> **Status:** Implemented in `contracts/ahjoor-payments/src/lib.rs`
> (`set_retry_config`, `initiate_allowed_payment`, `retry_failed_debit`, `trigger_early_retry`, `get_failed_debit`, `get_retry_config`)
> and tested in `contracts/ahjoor-payments/src/test_retry_queue.rs`.

---

## Overview

In `ahjoor-payments`, merchants can initiate recurring pull-debits against pre-authorized customer allowances via `initiate_allowed_payment`. If a customer has an insufficient token balance at the time of debit, the contract does **not** revert the transaction. Instead, it records a `FailedDebitRecord` with `FailedDebitStatus::Pending`, emits a `DebitFailed` event, and queues the failed debit for exponential back-off retries.

Key characteristics of the retry queue:
- **Non-reverting failures**: Insufficient funds store a pending record instead of reverting the contract call, returning a unique `record_id`.
- **Configurable back-off**: Retry intervals double with each failed attempt up to a maximum cap.
- **Dual retry triggers**: Retries can be executed automatically/by off-chain keepers after the back-off delay via `retry_failed_debit`, or triggered immediately by the customer via `trigger_early_retry`.
- **Recurring Invoice integration**: Successfully retried debits linked to a `RecurringInvoice` automatically increment the invoice's cycle counter and advance its due dates.

| Fact | Value |
| --- | --- |
| Admin configuration function | `set_retry_config` |
| Primary entry points | `initiate_allowed_payment`, `retry_failed_debit`, `trigger_early_retry` |
| Read entry points | `get_failed_debit`, `get_retry_config` |
| Default Base Retry Interval | `100` ledgers |
| Default Max Retry Interval | `3,200` ledgers |
| Default Max Attempts | `5` attempts |
| Key Storage Keys | `DataKey2::FailedDebitCounter`, `DataKey2::FailedDebit(record_id)`, `DataKey2::RetryConfig` |

---

## Retry Backoff & Configuration Model

The retry queue operates on an exponential back-off schedule controlled by global administration settings.

### Admin Configuration (`set_retry_config`)

```rust
pub fn set_retry_config(
    env: Env,
    admin: Address,
    base_retry_interval: u64,
    max_retry_interval: u64,
    max_retry_attempts: u32,
)
```

- **Authentication**: Requires `admin.require_auth()` and matches stored contract admin (`DataKey::Admin`).
- **Validation**:
  - `base_retry_interval` must be strictly positive (`> 0`).
  - `max_retry_interval` must be greater than or equal to `base_retry_interval`.
  - `max_retry_attempts` must be at least `1`.
- **Defaults** (used if `DataKey2::RetryConfig` is unset):
  - `base_retry_interval`: `100` ledgers (~500 seconds at ~5s/ledger)
  - `max_retry_interval`: `3,200` ledgers (~4.4 hours)
  - `max_retry_attempts`: `5` attempts

### Backoff Calculation

When an initial debit fails (attempt 1), `next_retry_ledger` is scheduled as:
$$\text{next\_retry\_ledger} = \text{current\_ledger} + \text{base\_retry\_interval}$$

For subsequent failed attempts ($k = \text{attempt\_number} \ge 2$), the delay interval doubles:
$$\text{interval} = \min\left(\text{base\_retry\_interval} \times 2^{k - 1},\, \text{max\_retry\_interval}\right)$$
$$\text{next\_retry\_ledger} = \text{current\_ledger} + \text{interval}$$

---

## Execution Mechanics & Caller Permissions

### Initiating Allowed Payments (`initiate_allowed_payment`)

```rust
pub fn initiate_allowed_payment(
    env: Env,
    merchant: Address,
    customer: Address,
    token: Address,
    amount: i128,
    plan_id: u32,
    invoice_id: Option<u32>,
) -> u32
```
- **Caller**: Merchant (`merchant.require_auth()`).
- **Behavior**:
  - Increments `DataKey2::FailedDebitCounter` to generate a new `record_id`.
  - Attempts token transfer using `try_transfer_from`.
  - **On Success**: Creates `FailedDebitRecord` with status `FailedDebitStatus::Succeeded`.
  - **On Failure**: Creates `FailedDebitRecord` with status `FailedDebitStatus::Pending`, sets `attempt_number = 1`, calculates `next_retry_ledger`, and emits `DebitFailed(record_id, plan_id, 1, next_retry_ledger)`.

---

### Scheduled Retry (`retry_failed_debit`)

```rust
pub fn retry_failed_debit(env: Env, record_id: u32)
```

- **Caller**: Anyone (publicly callable by off-chain bots, keepers, or merchants).
- **Execution Rules & Preconditions**:
  1. Checks `current_ledger >= rec.next_retry_ledger`. If the ledger sequence has not reached `next_retry_ledger`, panics with `Error::RetryNotDue`.
  2. Ensures status is not `FailedDebitStatus::Abandoned` (panics with `Error::DebitAlreadyAbandoned`) or `FailedDebitStatus::Succeeded` (panics with `Error::DebitAlreadySucceeded`).
- **Outcome**:
  - **If token transfer succeeds**:
    - Status is updated to `FailedDebitStatus::Succeeded`.
    - Emits `DebitRetrySucceeded(record_id, plan_id, amount)`.
    - If `rec.invoice_id` is present, invokes `advance_recurring_invoice_on_retry_success` to increment `cycles_triggered` and advance `next_due_ledger` / `next_due_at` on the `RecurringInvoice`.
  - **If token transfer fails**:
    - Increments `rec.attempt_number`.
    - If `rec.attempt_number > max_retry_attempts`, status transitions to `FailedDebitStatus::Abandoned` and emits `DebitAbandoned(record_id, plan_id)`.
    - Otherwise, recalculates `next_retry_ledger` with doubled back-off and emits `DebitFailed(record_id, plan_id, attempt_number, next_retry_ledger)`.

---

### Customer Early Retry (`trigger_early_retry`)

```rust
pub fn trigger_early_retry(env: Env, customer: Address, record_id: u32)
```

- **Caller**: Customer only (`customer.require_auth()`).
- **Purpose**: Enables the customer to immediately re-trigger a failed debit after topping up their token balance, bypassing the ledger delay check (`current_ledger >= next_retry_ledger`).
- **Preconditions**:
  - Caller must match `rec.customer` (`panic!("Only the debited customer may trigger an early retry")`).
  - Record must not be `Abandoned` or `Succeeded`.
- **Outcome**:
  - **If transfer succeeds**: Record marked `Succeeded`, emits `DebitRetrySucceeded`, and advances linked `RecurringInvoice` (if applicable).
  - **If transfer fails**: Increments `attempt_number`, updates `next_retry_ledger` from the *current* ledger sequence, and transitions to `Abandoned` if max attempts are exceeded.

---

## Retry Exhaustion (Abandonment)

When retries fail repeatedly and `attempt_number` exceeds `max_retry_attempts`:

1. The `FailedDebitRecord` status is set to `FailedDebitStatus::Abandoned`.
2. The contract emits the `DebitAbandoned` event:
   ```rust
   DebitAbandoned { record_id, plan_id }
   ```
3. Subsequent calls to `retry_failed_debit` or `trigger_early_retry` for this `record_id` will panic with `Error::DebitAlreadyAbandoned`.
4. No further token transfers will be attempted for this record ID.

---

## Data Structures & Enums

### `FailedDebitStatus`

```rust
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedDebitStatus {
    Pending = 0,
    Succeeded = 1,
    Abandoned = 2,
}
```

### `FailedDebitRecord`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailedDebitRecord {
    pub id: u32,
    pub plan_id: u32,
    pub invoice_id: Option<u32>,
    pub merchant: Address,
    pub customer: Address,
    pub token: Address,
    pub amount: i128,
    pub attempt_number: u32,
    pub next_retry_ledger: u64,
    pub status: FailedDebitStatus,
    pub created_at: u64,
}
```

### `RetryConfig`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryConfig {
    pub base_retry_interval: u64,
    pub max_retry_interval: u64,
    pub max_retry_attempts: u32,
}
```

---

## Storage Reference

| Key | Storage Type | Data Stored |
| --- | --- | --- |
| `DataKey2::FailedDebitCounter` | Persistent | `u32` counter tracking total failed debit records created. |
| `DataKey2::FailedDebit(record_id)` | Persistent | `FailedDebitRecord` containing debit attempt details, status, and retry schedule. |
| `DataKey2::RetryConfig` | Persistent | `RetryConfig` containing admin-configured back-off intervals and max attempt caps. |

---

## Events

| Event Topic | Payload / Fields | Emitted When |
| --- | --- | --- |
| `"DebitFailed"` | `(record_id: u32, plan_id: u32, attempt_number: u32, next_retry_ledger: u64)` | Initial payment attempt or a retry attempt fails due to insufficient balance. |
| `"DebitRetrySucceeded"` | `(record_id: u32, plan_id: u32, amount: i128)` | Retry attempt succeeds and funds are transferred. |
| `"DebitAbandoned"` | `(record_id: u32, plan_id: u32)` | Retry attempt number exceeds `max_retry_attempts`. |
| `"InvoiceCycleTriggered"` | `(invoice_id: u32, payment_id: u32, cycle_number: u32)` | Linked recurring invoice cycle is advanced upon successful retry. |

---

## Error Codes

| Error Code | Name | Description |
| --- | --- | --- |
| 34 | `RetryNotDue` | `retry_failed_debit` called before `current_ledger >= next_retry_ledger`. |
| 35 | `DebitRecordNotFound` | Specified `record_id` does not exist in persistent storage. |
| 36 | `DebitAlreadyAbandoned` | Attempted retry on a record with `FailedDebitStatus::Abandoned`. |
| 37 | `DebitAlreadySucceeded` | Attempted retry on a record with `FailedDebitStatus::Succeeded`. |

---

## Test Coverage

Unit tests are located in `contracts/ahjoor-payments/src/test_retry_queue.rs`:

- `test_successful_debit_stores_succeeded_record`: Verifies immediate success stores a `Succeeded` record.
- `test_insufficient_balance_stores_pending_record`: Verifies insufficient funds stores a `Pending` record with scheduled `next_retry_ledger`.
- `test_retry_not_due_before_backoff`: Ensures `retry_failed_debit` panics with `RetryNotDue` if invoked early.
- `test_retry_after_backoff_succeeds`: Confirms retry succeeds after customer balance top-up and ledger advancement past `next_retry_ledger`.
- `test_max_attempts_leads_to_abandonment`: Validates transition to `Abandoned` once max attempts are reached.
- `test_early_retry_bypasses_backoff`: Confirms `trigger_early_retry` succeeds immediately without advancing ledger sequences.
- `test_backoff_doubles_per_attempt`: Verifies back-off delay doubles on consecutive failed attempts.
- `test_retry_after_customer_top_up`: End-to-end test of customer wallet top-up and subsequent retry settlement.
- `test_retry_success_advances_cycle_counter`: Verifies that a successful retry linked to a `RecurringInvoice` increments the invoice cycle counter and advances due dates.
