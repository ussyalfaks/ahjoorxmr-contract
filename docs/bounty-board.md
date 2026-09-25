# Open Bounty Board & Milestone-Gated Bounties in `ahjoor-escrow`

> **Status:** Implemented in `contracts/ahjoor-escrow/src/lib.rs`
> (`create_bounty`, `claim_bounty`, `submit_bounty_work`, `approve_bounty_submission`, `reject_bounty_submission`, `cancel_bounty`, `create_milestone_bounty`, `submit_bounty_milestone`, `verify_bounty_milestone`, `set_max_bounty_rejection_rounds`, `get_bounty_data`, `list_bounty_submissions`)
> and tested in `contracts/ahjoor-escrow/src/test_bounty_board.rs`.

---

## Overview

The **Bounty Board** feature in `ahjoor-escrow` enables an open, competitive work assignment model. Unlike standard escrows where a buyer locks funds for a specific pre-designated seller, a bounty allows a buyer to lock funds up front without assigning a seller.

Any whitelisted solver can claim an unclaimed bounty on a first-come, first-served basis, submit their work deliverable before the submission deadline, and receive the escrowed funds upon buyer approval (or verifier sign-off for milestone-gated bounties).

Key capabilities:
- **Open Claiming**: Solvers claim bounties without prior buyer assignment.
- **Rejection & Re-Opening**: Buyers can reject unsatisfactory submissions, re-opening the bounty for other solvers up to a configurable maximum rejection threshold (`MaxBountyRejectionRounds`, default `3`).
- **Cancellation & Refunds**: Buyers can cancel unclaimed bounties (or bounties whose claim deadline has passed) and receive a full refund of unused funds.
- **Milestone Gating**: Buyers can create multi-stage bounties with custom verifiers per milestone tranche.

| Fact | Value |
| --- | --- |
| Primary Entry Points | `create_bounty`, `claim_bounty`, `submit_bounty_work`, `approve_bounty_submission`, `reject_bounty_submission`, `cancel_bounty` |
| Milestone Entry Points | `create_milestone_bounty`, `submit_bounty_milestone`, `verify_bounty_milestone` |
| Read Entry Points | `get_bounty_data`, `list_bounty_submissions`, `get_bounty_milestones` |
| Default Max Rejection Rounds | `3` rounds |
| Initial Escrow Status | `EscrowStatus::BountyUnclaimed` |
| Claimed Escrow Status | `EscrowStatus::BountyClaimed` |
| Key Storage Keys | `DataKey2::BountyData(escrow_id)`, `DataKey2::BountySubmissions(escrow_id)`, `DataKey2::BountyMilestones(escrow_id)`, `DataKey2::MaxBountyRejectionRounds` |

---

## Bounty Lifecycle

```
                 ┌───────────────────────────┐
                 │       create_bounty       │
                 └─────────────┬─────────────┘
                               │
                               ▼
                    ┌─────────────────────┐
             ┌─────▶│   BountyUnclaimed   │◀─────┐
             │      └──────────┬──────────┘      │
             │                 │                 │
             │                 │ claim_bounty    │ cancel_bounty
             │                 ▼                 │ (if past deadline)
             │      ┌─────────────────────┐      │
             │      │    BountyClaimed    │      │
             │      └──────────┬──────────┘      │
             │                 │                 │
             │ reject_bounty_  │ submit_bounty_  │
             │ submission      │ work            │
             │ (rejection_count│                 │
             │  < max_rounds)  ▼                 │
             │      ┌─────────────────────┐      │
             └──────┤   Work Submitted    │──────┘
                    └──────────┬──────────┘
                               │
                               │ approve_bounty_submission
                               ▼
                    ┌─────────────────────┐
                    │      Released       │
                    └─────────────────────┘
```

### 1. Creation (`create_bounty` / `create_milestone_bounty`)
- **Caller**: Buyer (`buyer.require_auth()`).
- **Validation**:
  - `amount > 0` (or positive amounts per milestone for milestone bounties).
  - `claim_deadline_ledger > current_time`.
  - `submission_deadline_ledger > claim_deadline_ledger`.
  - Token must be whitelisted if a token whitelist contract is configured.
- **Action**: Transfers total funds from buyer to the contract. Initializes escrow with `seller = contract_address` and status `EscrowStatus::BountyUnclaimed`.

### 2. Claiming (`claim_bounty`)
- **Caller**: Any solver (`solver.require_auth()`).
- **Validation**:
  - Escrow status must be `BountyUnclaimed`.
  - `current_time <= claim_deadline_ledger`.
- **Action**: Assigns solver as `escrow.seller`, updates status to `EscrowStatus::BountyClaimed`, and emits `BountyClaimed(escrow_id, solver)`.

### 3. Submission (`submit_bounty_work` / `submit_bounty_milestone`)
- **Caller**: Assigned solver only (`solver.require_auth()`).
- **Validation**:
  - Escrow status must be `BountyClaimed`.
  - Caller must match assigned solver (`escrow.seller`).
  - `current_time <= submission_deadline_ledger` (or `escrow.deadline`).
  - For milestone bounties: All prior milestones ($0 \dots \text{index}-1$) must be `Paid`.
- **Action**: Stores `submission_hash` (or deliverable hash), appends submission record to `BountySubmissions`, and emits `BountyWorkSubmitted` / `BountyMilestoneSubmitted`.

### 4. Approval (`approve_bounty_submission` / `verify_bounty_milestone`)
- **Caller**: Buyer (`buyer.require_auth()`) for standard bounties; designated milestone verifier (`verifier.require_auth()`) for milestone bounties.
- **Validation**:
  - Submission must exist (`submission_hash.is_some()`).
  - Escrow status must be `BountyClaimed`.
- **Action**: Transfers escrowed funds (or milestone tranche) to the solver, transitions status to `EscrowStatus::Released` (or `Paid`), and emits `BountyAwarded` / `MilestoneVerified`.

### 5. Rejection & Re-Opening (`reject_bounty_submission`)
- **Caller**: Buyer (`buyer.require_auth()`).
- **Validation**:
  - Escrow status must be `BountyClaimed`.
  - `bounty_data.rejection_count < max_rejections` (default 3). If `rejection_count >= max_rejections`, panics with `MaximumRejectionRoundsReached`.
- **Action**:
  - Resets `escrow.seller` to contract address and status to `BountyUnclaimed`.
  - Clears `bounty_data.solver` and `bounty_data.submission_hash`.
  - Increments `rejection_count` by 1.
  - Emits `BountyRejected(escrow_id, solver, rejection_count)`.
  - Allows another solver to claim the bounty.

### 6. Cancellation & Refund (`cancel_bounty`)
- **Caller**: Buyer (`buyer.require_auth()`).
- **Validation**:
  - Allowed if escrow status is `BountyUnclaimed` (anytime), OR if status is `BountyClaimed` and `current_time > claim_deadline_ledger`.
- **Action**: Refunds remaining undisbursed funds to the buyer, sets status to `EscrowStatus::Refunded`, and emits `BountyCancelled(escrow_id, buyer, refund_amount)`.

---

## Milestone-Gated Bounty Variant

For complex deliverables, buyers can create milestone-gated bounties via `create_milestone_bounty`:

- **Upfront Funding**: Buyer deposits the sum of all milestone tranche amounts up front.
- **Ordered Execution**: Milestones must be submitted and verified strictly in sequential index order ($0, 1, 2, \dots$). Milestone $N+1$ cannot be submitted until milestone $N$ is verified and paid.
- **Custom Verifiers**: Each milestone specifies a dedicated `verifier` address. Only that verifier (`verifier.require_auth()`) can execute `verify_bounty_milestone`.
- **Partial Refund on Cancellation**: If cancelled, any already-disbursed milestone payouts remain with the solver; only remaining undisbursed funds are refunded to the buyer.

---

## Permissions Matrix

| Function | Authorized Caller | Preconditions |
| --- | --- | --- |
| `create_bounty` | Buyer (`buyer.require_auth()`) | Valid deadlines (`claim < submission`), `amount > 0`. |
| `create_milestone_bounty` | Buyer (`buyer.require_auth()`) | Non-empty milestones list, positive tranche amounts. |
| `claim_bounty` | Any Solver (`solver.require_auth()`) | `BountyUnclaimed`, before `claim_deadline_ledger`. |
| `submit_bounty_work` | Assigned Solver (`solver.require_auth()`) | `BountyClaimed`, before `submission_deadline_ledger`. |
| `submit_bounty_milestone` | Assigned Solver (`solver.require_auth()`) | `BountyClaimed`, prior milestones `Paid`. |
| `approve_bounty_submission` | Buyer (`buyer.require_auth()`) | `BountyClaimed`, `submission_hash` present. |
| `verify_bounty_milestone` | Milestone Verifier (`verifier.require_auth()`) | Milestone `Submitted`. |
| `reject_bounty_submission` | Buyer (`buyer.require_auth()`) | `BountyClaimed`, `rejection_count < max_rejections`. |
| `cancel_bounty` | Buyer (`buyer.require_auth()`) | `BountyUnclaimed` OR past `claim_deadline_ledger`. |
| `set_max_bounty_rejection_rounds` | Admin (`admin.require_auth()`) | Must match stored contract admin. |

---

## Data Structures

### `BountyData`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BountyData {
    pub description_hash: BytesN<32>,
    pub claim_deadline_ledger: u64,
    pub submission_deadline_ledger: u64,
    pub solver: Option<Address>,
    pub submission_hash: Option<BytesN<32>>,
    pub rejection_count: u32,
    pub fees_disbursed: i128,
}
```

### `BountySubmission`

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BountySubmission {
    pub solver: Address,
    pub submission_hash: BytesN<32>,
    pub submitted_at: u64,
}
```

### `BountyMilestone` & `BountyMilestoneStatus`

```rust
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BountyMilestoneStatus {
    Pending = 0,
    Submitted = 1,
    Verified = 2,
    Paid = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BountyMilestone {
    pub description_hash: BytesN<32>,
    pub verifier: Address,
    pub amount: i128,
    pub status: BountyMilestoneStatus,
    pub deliverable_hash: Option<BytesN<32>>,
}
```

---

## Storage Reference

| Key | Storage Type | Data Stored |
| --- | --- | --- |
| `DataKey::Escrow(escrow_id)` | Persistent | Core `Escrow` struct tracking status (`BountyUnclaimed`, `BountyClaimed`, `Released`, `Refunded`). |
| `DataKey2::BountyData(escrow_id)` | Persistent | `BountyData` struct holding metadata, deadlines, assigned solver, and rejection count. |
| `DataKey2::BountySubmissions(escrow_id)` | Persistent | `Vec<BountySubmission>` tracking historical work submissions. |
| `DataKey2::BountyMilestones(escrow_id)` | Persistent | `Vec<BountyMilestone>` tracking milestone schedules and statuses. |
| `DataKey2::MaxBountyRejectionRounds` | Instance | `u32` value overriding default max rejection rounds (default `3`). |

---

## Events

| Event Topic | Payload / Fields | Emitted When |
| --- | --- | --- |
| `"BountyCreated"` | `(escrow_id, buyer, amount, token, description_hash, claim_deadline, submission_deadline)` | Bounty escrow created and funded. |
| `"BountyClaimed"` | `(escrow_id, solver)` | Solver claims an unclaimed bounty. |
| `"BountyWorkSubmitted"` | `(escrow_id, solver, submission_hash)` | Solver submits work deliverable. |
| `"BountyAwarded"` | `(escrow_id, solver, amount)` | Buyer approves submission and funds are released. |
| `"BountyRejected"` | `(escrow_id, rejected_solver, rejection_count)` | Buyer rejects submission and bounty is re-opened. |
| `"BountyCancelled"` | `(escrow_id, buyer, refund_amount)` | Unclaimed bounty is cancelled and buyer is refunded. |
| `"BountyMilestoneCreated"`| `(escrow_id, buyer, milestone_count, total_amount)` | Milestone-gated bounty created. |
| `"BountyMilestoneSubmitted"`| `(escrow_id, index, solver, deliverable_hash)` | Solver submits deliverable for a specific milestone. |
| `"MilestoneVerified"` | `(escrow_id, index, amount, verifier)` | Verifier approves milestone and tranche is paid. |

---

## Error Codes (`EscrowErrorExt3`)

| Error Name | Description |
| --- | --- |
| `BountyAmountMustBePositive` | `create_bounty` invoked with amount $\le 0$. |
| `ClaimDeadlineMustBeFuture` | `claim_deadline_ledger` is in the past or current time. |
| `SubmissionDeadlineMustBeAfterClaimDeadline` | `submission_deadline_ledger <= claim_deadline_ledger`. |
| `BountyIsNotAvailableClaiming` | `claim_bounty` called on a non-`BountyUnclaimed` escrow. |
| `ClaimDeadlineHasPassed` | `claim_bounty` called after `claim_deadline_ledger`. |
| `BountyIsNotClaimedStatus` | Operation requires `BountyClaimed` status. |
| `OnlyAssignedSolverCanSubmitWork` | `submit_bounty_work` called by an address other than `escrow.seller`. |
| `SubmissionDeadlineHasPassed` | Submission attempted past `submission_deadline_ledger`. |
| `OnlyBuyerCanApproveSubmission` | Non-buyer attempted `approve_bounty_submission`. |
| `NoSubmissionHasBeenMade` | `approve_bounty_submission` called before work was submitted. |
| `OnlyBuyerCanRejectSubmission` | Non-buyer attempted `reject_bounty_submission`. |
| `MaximumRejectionRoundsReached` | `reject_bounty_submission` called when `rejection_count >= max_rejections`. |
| `OnlyBuyerCanCancelBounty` | Non-buyer attempted `cancel_bounty`. |
| `CannotCancelBountyCurrentState` | `cancel_bounty` called on claimed bounty before claim deadline. |
| `BountyMustHaveAtLeastOneMilestone` | `create_milestone_bounty` called with empty milestones vector. |

---

## Test Coverage

Unit tests in `contracts/ahjoor-escrow/src/test_bounty_board.rs`:

- `test_create_bounty_success`: Validates bounty creation, status `BountyUnclaimed`, and initial data.
- `test_create_bounty_zero_amount`: Asserts `BountyAmountMustBePositive` error.
- `test_create_bounty_past_claim_deadline`: Asserts `ClaimDeadlineMustBeFuture` error.
- `test_create_bounty_invalid_deadlines`: Asserts `SubmissionDeadlineMustBeAfterClaimDeadline` error.
- `test_claim_bounty_success`: Validates claiming by solver and status update to `BountyClaimed`.
- `test_claim_bounty_duplicate`: Confirms second claim panics with `BountyIsNotAvailableClaiming`.
- `test_claim_bounty_after_deadline`: Confirms claim after deadline panics with `ClaimDeadlineHasPassed`.
- `test_submit_bounty_work_success`: Validates submission hash storage and `BountySubmissions` list.
- `test_submit_bounty_work_wrong_solver`: Asserts `OnlyAssignedSolverCanSubmitWork` error.
- `test_submit_bounty_work_after_deadline`: Asserts `SubmissionDeadlineHasPassed` error.
- `test_approve_bounty_submission_success`: Validates fund transfer to solver and status update to `Released`.
- `test_approve_bounty_without_submission`: Asserts `NoSubmissionHasBeenMade` error.
- `test_reject_bounty_submission_and_reclaim`: Validates rejection, reset to `BountyUnclaimed`, and re-claiming by second solver.
- `test_reject_bounty_max_rejections`: Validates panic on exceeding `MaxBountyRejectionRounds`.
- `test_cancel_bounty_unclaimed`: Validates cancellation and refund of unclaimed bounty.
- `test_cancel_bounty_claimed`: Asserts `CannotCancelBountyCurrentState` error when active.
- `test_cancel_bounty_after_inspection_fee`: Validates partial refund after partial milestone payout.
- `test_full_bounty_award_flow`: Complete end-to-end integration test of the bounty lifecycle.
