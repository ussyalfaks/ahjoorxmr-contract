# Emergency Loan in ROSCA (`ahjoor-rosca`)

This document describes the **emergency loan** mechanism in the `ahjoor-rosca` contract:
how a member can request an emergency loan from the group's reserve, the repayment
terms and defaults, and how the reserve balance is managed.

> Implementation references: `contracts/ahjoor-rosca/src/lib.rs`
> (`request_emergency_loan`, `repay_emergency_loan`, `get_emergency_loan`,
> `get_emergency_loan_counter`, `get_member_active_loan`, `get_emergency_reserve_balance`,
> `set_reserve_enabled`, `get_reserve_enabled`), `contracts/ahjoor-rosca/src/types.rs`
> (`EmergencyLoan`), `contracts/ahjoor-rosca/src/events.rs`, and the test suites
> `contracts/ahjoor-rosca/src/test_emergency_loan.rs` and
> `contracts/ahjoor-rosca/src/test_emergency_reserve.rs`.

---

## 1. Overview

The **emergency loan** feature allows a ROSCA member to draw an emergency loan
against the group's reserve when they cannot make their contribution on time. This
provides a safety net for members facing temporary liquidity issues while protecting
the group from unlimited exposure.

Key characteristics:
- Loans are drawn from the group's **emergency reserve balance**
- Only one outstanding loan per member at a time
- Loan amount is capped at a fraction of the reserve (default 50%)
- Loans have a configurable repayment window in ledgers
- Defaulted loans are deducted from the member's future payout

---

## 2. Storage and Types

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyLoan {
    pub loan_id: u32,
    pub borrower: Address,
    pub amount: i128,
    pub created_at_ledger: u32,
    pub repayment_deadline_ledger: u32,
    pub repaid_amount: i128,
    pub defaulted: bool,
}
```

| Key | Storage | Contents |
| :--- | :--- | :--- |
| `DataKey3::ReserveEnabled` | instance | `bool` — whether the emergency reserve is enabled |
| `DataKey3::EmergencyReserveBalance` | persistent | `i128` — total tokens available for emergency loans |
| `DataKey3::EmergencyLoanCounter` | persistent | `u32` — total loans ever issued (for ID generation) |
| `DataKey3::EmergencyLoan(loan_id)` | persistent | `EmergencyLoan` — loan details |
| `DataKey3::MemberOutstandingLoan(member)` | persistent | `u32` — active loan ID for a member (0 if none) |

---

## 3. Reserve Management

### Enabling the Reserve

```rust
pub fn set_reserve_enabled(env: Env, admin: Address, enabled: bool)
pub fn is_reserve_enabled(env: Env) -> bool
pub fn get_reserve_enabled(env: Env) -> bool
```

The emergency reserve must be explicitly enabled by the admin before any loans can
be requested. The `is_reserve_enabled` query returns the current status.

### Reserve Balance

```rust
pub fn get_emergency_reserve_balance(env: Env) -> i128
```

Returns the current emergency reserve balance. This is the pool from which loans
are drawn. The reserve is typically funded by:
- Group treasury allocations
- Forfeited payouts from defaulted members
- Admin deposits

---

## 4. Requesting an Emergency Loan

```rust
pub fn request_emergency_loan(
    env: Env,
    member: Address,
    amount: i128,
    repayment_window_ledgers: u32,
) -> u32
```

**Authentication:** `member.require_auth()`

**Preconditions:**
1. Member must be a registered group member (`NotAMember`, error 1008)
2. Emergency reserve must be enabled
3. Member must have no outstanding loan (`OutstandingLoanExists`)
4. Loan amount must be positive
5. Reserve must have sufficient balance
6. Loan amount must not exceed `MAX_LOAN_FRACTION_BPS` (default 50% of reserve)

**Behavior:**
1. Validates all preconditions
2. Creates an `EmergencyLoan` record with:
   - `loan_id` = next sequential ID
   - `repayment_deadline_ledger` = current ledger + `repayment_window_ledgers`
   - `repaid_amount` = 0
   - `defaulted` = false
3. Stores the loan record
4. Tracks the loan ID for the member (`MemberOutstandingLoan`)
5. Deducts from reserve balance
6. Transfers tokens to the member
7. Emits `EmergencyLoanGranted` event

**Error codes:**
| Code | Name | Raised when |
| :--- | :--- | :--- |
| 1008 | `NotAMember` | Caller is not a registered member |
| — | `OutstandingLoanExists` | Member already has an active loan |
| — | `Insufficient reserve balance` | Reserve cannot fund the requested amount |
| — | `Loan amount exceeds maximum` | Amount > 50% of reserve |

**Example:**
```rust
// Request a 100 token loan with 500-ledger repayment window
let loan_id = client.request_emergency_loan(&member, &100, &500);
let loan = client.get_emergency_loan(&loan_id);
assert_eq!(loan.amount, 100);
assert!(!loan.defaulted);
```

---

## 5. Repaying an Emergency Loan

```rust
pub fn repay_emergency_loan(env: Env, member: Address, loan_id: u32, amount: i128)
```

**Authentication:** `member.require_auth()`

**Preconditions:**
1. Loan must exist
2. Caller must be the borrower
3. Loan must not have defaulted
4. Repayment amount must be positive
5. Repayment must not exceed remaining balance

**Behavior:**
1. Validates all preconditions
2. Updates `repaid_amount` on the loan
3. If fully repaid, clears the member's outstanding loan tracking
4. Transfers tokens from member to contract
5. Adds tokens back to the reserve balance
6. Emits `EmergencyLoanRepaid` event

**Error codes:**
| Code | Name | Raised when |
| :--- | :--- | :--- |
| — | `Loan not found` | Invalid loan ID |
| — | `Only the borrower can repay` | Caller is not the loan borrower |
| — | `Loan has defaulted` | Cannot repay a defaulted loan |
| — | `Repayment exceeds balance` | Amount > remaining owed |

**Example:**
```rust
// Repay 50 of a 100 token loan
client.repay_emergency_loan(&member, &loan_id, &50);
let loan = client.get_emergency_loan(&loan_id);
assert_eq!(loan.repaid_amount, 50);

// Later, repay the remaining 50
client.repay_emergency_loan(&member, &loan_id, &50);
let loan = client.get_emergency_loan(&loan_id);
assert_eq!(loan.repaid_amount, 100);
```

---

## 6. Default Handling

When `finalize_round` processes a member with an outstanding emergency loan:

1. Checks if the loan's `repayment_deadline_ledger` has passed
2. If deadline passed and not fully repaid:
   - Sets `loan.defaulted = true`
   - Deducts the remaining owed amount from the member's payout
   - Emits `LoanDefaultDeducted` event

The defaulted amount is subtracted from any payout the member would receive,
ensuring the group is made whole before the member benefits.

---

## 7. Queries

```rust
pub fn get_emergency_loan(env: Env, loan_id: u32) -> EmergencyLoan
```

Returns the full loan record for a given loan ID.

```rust
pub fn get_emergency_loan_counter(env: Env) -> u32
```

Returns the total number of emergency loans ever issued (issue #645). Returns `0`
when no loan has ever been requested.

```rust
pub fn get_member_active_loan(env: Env, member: Address) -> u32
```

Returns the active loan ID for a member, or `0` if no outstanding loan exists.

---

## 8. Events and Errors

### Events

| Symbol | Payload |
| :--- | :--- |
| `EmergencyLoanGranted` | `(group_id, member, loan_id, amount, repayment_deadline)` |
| `EmergencyLoanRepaid` | `(group_id, loan_id, amount, remaining)` |
| `LoanDefaultDeducted` | `(group_id, loan_id, deducted_from_payout)` |

### Event Schemas

```rust
pub struct EmergencyLoanGranted {
    pub group_id: u32,
    pub member: Address,
    pub loan_id: u32,
    pub amount: i128,
    pub repayment_deadline: u32,
}

pub struct EmergencyLoanRepaid {
    pub group_id: u32,
    pub loan_id: u32,
    pub amount: i128,
    pub remaining: i128,
}

pub struct LoanDefaultDeducted {
    pub group_id: u32,
    pub loan_id: u32,
    pub deducted_from_payout: i128,
}
```

### Errors

| Code | Name | When |
| :--- | :--- | :--- |
| 1008 | `NotAMember` | Non-member requests a loan |
| — | `OutstandingLoanExists` | Member requests loan while one is active |
| — | `Loan not found` | Query/repay with invalid ID |
| — | `Only the borrower can repay` | Non-borrower attempts repayment |
| — | `Loan has defaulted` | Repayment attempted on defaulted loan |

---

## 9. Constants

```rust
const MAX_LOAN_FRACTION_BPS: u32 = 5_000; // 50%
```

The maximum loan amount is capped at `reserve_balance * MAX_LOAN_FRACTION_BPS / 10_000`,
preventing a single member from draining the entire reserve.

---

## 10. End-to-End Example

```text
1. Admin enables the emergency reserve:
   client.set_reserve_enabled(&admin, &true);

2. Reserve is funded with 1000 tokens (from treasury or defaults):
   (internal - reserve balance = 1000)

3. Member M requests a 200 token loan with 500-ledger repayment:
   loan_id = client.request_emergency_loan(&member, &200, &500);
   // Reserve balance = 800
   // Loan record created with repayment_deadline = current + 500

4. Member repays 100 tokens before deadline:
   client.repay_emergency_loan(&member, &loan_id, &100);
   // Loan repaid_amount = 100, remaining = 100
   // Reserve balance = 900

5. Time passes, member repays remaining 100:
   client.repay_emergency_loan(&member, &loan_id, &100);
   // Loan fully repaid, MemberOutstandingLoan cleared
   // Reserve balance = 1000

---

ALTERNATIVE: Default Scenario

1-3. Same as above (member takes 200 token loan)

4. Member fails to repay before deadline:
   // During finalize_round, loan.defaulted is set to true

5. When member's payout would be 300:
   // 100 is deducted to cover the defaulted loan
   // Member receives 200 net
   // LoanDefaultDeducted event emitted
```

## 11. Function Reference

| Function | Caller | Purpose |
| :--- | :--- | :--- |
| `set_reserve_enabled(admin, enabled)` | Admin | Enable or disable the emergency reserve |
| `get_emergency_reserve_balance()` | Anyone | Query current reserve balance |
| `request_emergency_loan(member, amount, window)` | Member | Request a loan from the reserve |
| `repay_emergency_loan(member, loan_id, amount)` | Member | Repay an outstanding loan |
| `get_emergency_loan(loan_id)` | Anyone | Get loan details |
| `get_emergency_loan_counter()` | Anyone | Get total loans ever issued |
| `get_member_active_loan(member)` | Anyone | Get member's active loan ID (0 if none) |