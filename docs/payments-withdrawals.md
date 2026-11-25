# Merchant Withdrawal Queue and Limits in `ahjoor-payments`

> **Status:** Implemented in `contracts/ahjoor-payments/src/lib.rs`
> (`process_withdrawal_queue`, `prioritize_withdrawal`, `get_merchant_withdrawal_queue`, `get_withdrawal_queue_position`,
> `set_withdrawal_window`, `set_default_withdrawal_limits`, `set_withdrawal_limit`, `override_withdrawal_limit`,
> `get_withdrawal_limit`, `get_default_withdrawal_limits`, `get_withdrawal_rate_limit`)
> and tested in `test.rs`, `test_get_withdrawal_limit.rs` and `test_get_default_withdrawal_limits.rs`.

---

## Overview

`ahjoor-payments` has two related merchant-withdrawal mechanisms:

1. **Withdrawal queue (#126)** — every completed payment is appended to a per-merchant FIFO queue of
   `(payment_id, net_amount)` entries. The merchant reconciles the queue in batches and can move a specific
   entry to the front.
2. **Withdrawal rate limit (#231)** — a rolling time window with a cap on the total net amount a merchant can
   receive through `settle_merchant_payments`. A global default applies to every merchant; a merchant can set
   its own limit and the admin can override it.

| Fact | Value |
| --- | --- |
| Queue entry points | `process_withdrawal_queue`, `prioritize_withdrawal` |
| Queue read entry points | `get_merchant_withdrawal_queue`, `get_withdrawal_queue_position` |
| Admin limit entry points | `set_withdrawal_window`, `set_default_withdrawal_limits`, `override_withdrawal_limit` |
| Merchant limit entry point | `set_withdrawal_limit` |
| Limit read entry points | `get_withdrawal_limit`, `get_default_withdrawal_limits`, `get_withdrawal_rate_limit` |
| Default window | `86_400` seconds (24 hours) |
| Default cap | `i128::MAX` (effectively unlimited) |
| Default batch size for `process_withdrawal_queue(max_count = 0)` | `20` (`DEFAULT_MAX_BATCH_SIZE`) |
| Error on cap breach | `Error::WithdrawalRateLimitExceeded` (`#14`) |
| Storage keys | `DataKey::WithdrawalQueue(merchant)`, `DataKey2::WithdrawalWindowSeconds`, `DataKey2::WithdrawalWindowCap`, `DataKey2::MerchantWithdrawalLimit(merchant)`, `DataKey2::MerchantWithdrawalWindow(merchant)` |

---

## Withdrawal Queue

### How entries are added

When a payment completes, the contract pays the merchant and then calls the internal `enqueue_withdrawal`,
which appends `(payment_id, net_amount)` to the back of `DataKey::WithdrawalQueue(merchant)` and emits
`WithdrawalQueued { merchant, payment_id, amount }`.

- `net_amount` is the amount after protocol fees.
- Multi-payee split payments (`DataKey3::SplitPayees(payment_id)` set) are **not** queued, because every payee is
  paid at settlement.
- The queue is a bookkeeping record: the funds were already transferred when the payment completed. Processing
  the queue reconciles entries; it does not move tokens.

### Queue ordering

The queue is **FIFO**: entries are processed in the order payments completed. The only way to change the order
is `prioritize_withdrawal`.

### `process_withdrawal_queue`

```rust
pub fn process_withdrawal_queue(env: Env, merchant: Address, max_count: u32) -> i128
```

- **Auth:** `merchant.require_auth()`. Fails if the contract is paused.
- Returns `0` immediately if the queue is empty.
- `max_count = 0` means "use the default batch size" (`20`). Otherwise up to `min(max_count, queue.len())`
  entries are taken from the **front** of the queue.
- Each processed entry's payment must still be `PaymentStatus::Completed`; otherwise the call panics with
  `"Payment in queue is not completed"` and nothing is removed.
- The processed entries are removed, the rest keep their order, and `WithdrawalProcessed { merchant, total }` is
  emitted where `total` is the number of entries processed.
- Returns the sum of the processed entries' amounts.

### `prioritize_withdrawal`

```rust
pub fn prioritize_withdrawal(env: Env, merchant: Address, payment_id: u32)
```

- **Auth:** `merchant.require_auth()`. Fails if the contract is paused.
- Panics with `"Withdrawal queue is empty"` if the queue is empty, and `"Payment not found in withdrawal queue"`
  if `payment_id` is not queued for this merchant.
- Moves the entry to index `0`. All other entries keep their relative order. If it is already at the front, the
  call does nothing.

### Reading the queue

- `get_merchant_withdrawal_queue(merchant) -> Vec<(u32, i128)>` returns the whole queue in processing order.
- `get_withdrawal_queue_position(merchant, payment_id) -> Option<u32>` returns the entry's 0-indexed position
  (taking `prioritize_withdrawal` into account), or `None` if it is not queued.

### Example

```text
queue: [(1, 100), (2, 250), (3, 75)]

prioritize_withdrawal(merchant, 3)   -> [(3, 75), (1, 100), (2, 250)]
process_withdrawal_queue(merchant, 2) -> returns 175, queue: [(2, 250)]
```

---

## Withdrawal Rate-Limit Windows

### Effective limit

The limit is checked in `settle_merchant_payments` against the merchant's net settlement amount (after fees,
before any rolling-reserve hold). The effective `(window_seconds, cap)` is picked in this order:

1. The merchant-specific limit in `DataKey2::MerchantWithdrawalLimit(merchant)`, if set (by the merchant or by
   an admin override).
2. Otherwise the global default from `DataKey2::WithdrawalWindowSeconds` and `DataKey2::WithdrawalWindowCap`.
3. If neither is stored, no limit is enforced.

`initialize` stores the global default as `86_400` seconds and `i128::MAX`, so out of the box the window exists
but the cap never triggers. `get_withdrawal_rate_limit(merchant)` returns the effective `(window_seconds, cap)`
for a merchant using the same order.

### How the window works

Per merchant, `DataKey2::MerchantWithdrawalWindow(merchant)` stores a `WithdrawalWindowState { window_start, withdrawn }`.

- On the first settlement the window starts at the current ledger timestamp.
- If `now >= window_start + window_seconds`, the window resets: `window_start = now`, `withdrawn = 0`.
- The new total `withdrawn + amount` is compared with `cap`. If it is higher, the contract emits
  `WdrlLimitExceeded (merchant, attempted, cap)` and panics with `Error::WithdrawalRateLimitExceeded` (`#14`).
  The whole settlement reverts.
- Otherwise `withdrawn` is updated.

The window is a fixed window that starts at the first settlement after the previous one expired, not a sliding
window.

### Default limits (admin)

```rust
pub fn set_withdrawal_window(env: Env, admin: Address, window_seconds: u64, cap: i128)
pub fn set_default_withdrawal_limits(env: Env, admin: Address, window_seconds: u64, cap: i128)
pub fn get_default_withdrawal_limits(env: Env) -> (u64, i128)
```

- Both setters write the same global keys (`WithdrawalWindowSeconds`, `WithdrawalWindowCap`). They require the
  stored admin and an unpaused contract.
- `window_seconds` must be `> 0` (`"Window must be positive"`) and `cap` must be `> 0` (`"Cap must be positive"`).
- `get_default_withdrawal_limits` returns `(86_400, i128::MAX)` when nothing is stored.
- The default only applies to merchants **without** a merchant-specific limit.

### Merchant limit

```rust
pub fn set_withdrawal_limit(env: Env, merchant: Address, window_seconds: u64, cap: i128)
pub fn get_withdrawal_limit(env: Env, merchant: Address) -> (u64, i128)
```

- **Auth:** `merchant.require_auth()`. Fails if the contract is paused.
- Same validation as the defaults: positive window and cap.
- Stores a `WithdrawalLimit { window_seconds, cap }` and emits `WdrlLimitSet (merchant, window_seconds, cap)`.
- The merchant limit **replaces** the global default for that merchant. It is meant to be stricter, but the
  contract does not check that it is lower than the default.
- `get_withdrawal_limit` returns `(0, 0)` if no merchant-specific limit is stored.

### Admin override

```rust
pub fn override_withdrawal_limit(env: Env, admin: Address, merchant: Address, cap: i128)
```

- **Auth:** the stored admin only (`"Only admin can override withdrawal limit"`). Fails if the contract is paused.
- `cap` must be `> 0`.
- Changes only the cap. The window is kept from the merchant's existing limit, or taken from the global default
  (`86_400` if none is stored) when the merchant has no limit yet.
- Writes the result as the merchant-specific limit and emits `WdrlLimitSet`. The merchant can later replace it by
  calling `set_withdrawal_limit` again.
- The current window state (`withdrawn` so far) is not reset by an override.

---

## Events

| Event | Emitted by | Payload |
| --- | --- | --- |
| `WithdrawalQueued` | payment completion | `merchant`, `payment_id`, `amount` |
| `WithdrawalProcessed` | `process_withdrawal_queue` | `merchant`, `total` (entries processed) |
| `WdrlLimitSet` | `set_withdrawal_limit`, `override_withdrawal_limit` | `(merchant, window_seconds, cap)` |
| `WdrlLimitExceeded` | `settle_merchant_payments` | `(merchant, attempted, cap)` |
