# Bounty Board Implementation

## Overview
Successfully implemented an open competitive work assignment system (Bounty Board) in the ahjoor-escrow contract, allowing a buyer to create a bounty escrow with no pre-assigned seller. Any solver can claim the bounty first-come-first-served, submit work, and receive payment upon buyer approval.

## Features Implemented

### 1. Basic Bounty Creation & Lifecycle
- **Open Bounties**: Buyer creates a bounty with description hash, amount, claim deadline, and submission deadline
- **First-Come-First-Served Claiming**: Any solver can claim an unclaimed bounty
- **Work Submission**: Assigned solver submits a proof-of-work hash before the submission deadline
- **Buyer Approval**: Buyer approves submission, releasing funds to the solver
- **Buyer Rejection**: Buyer rejects submission, re-opening bounty for another solver (up to configurable max rounds)
- **Buyer Cancellation**: Buyer can cancel an unclaimed bounty and receive a full refund

### 2. Configurable Rejection Rounds
- **Default Maximum**: 3 rejection rounds (configurable by admin)
- **Admin Configuration**: `set_max_bounty_rejection_rounds()` to adjust the limit
- **Enforcement**: After max rejections, further rejection panics with "Maximum rejection rounds reached"
- **Per-Bounty Tracking**: `rejection_count` field on `BountyData` tracks current round

### 3. Milestone-Gated Bounty Variant
- **Multi-Milestone Bounties**: Bounty split into sequential milestones, each with its own verifier
- **Ordered Verification**: Milestone N+1 cannot be submitted before milestone N is verified
- **Per-Milestone Verifiers**: Each milestone has a designated verifier address
- **Verifier Replacement**: Bounty creator can replace a verifier before the milestone is submitted
- **Tranche Release**: Funds released per-milestone; escrow settles only after all milestones paid

### 4. Bounty Cancellation with Partial Payout
- **Milestone Bounty Cancellation**: Cancelling a milestone bounty after partial payouts refunds only remaining funds
- **State Check**: Cancellation only allowed in `BountyUnclaimed` or `BountyClaimed` (past deadline) states

## New Storage Keys
- `DataKey2::BountyData(u32)`: Bounty metadata per escrow (description hash, deadlines, solver, submission hash, rejection count, fees disbursed)
- `DataKey2::MaxBountyRejectionRounds`: Global admin-configured max rejection rounds
- `DataKey2::BountyMilestones(u32)`: Ordered milestone schedule per escrow (Vec of BountyMilestone)

## New Data Types

### BountyData
```rust
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

### BountyMilestone / BountyMilestoneInput
```rust
pub struct BountyMilestoneInput {
    pub description_hash: BytesN<32>,
    pub verifier: Address,
    pub amount: i128,
}

pub struct BountyMilestone {
    pub description_hash: BytesN<32>,
    pub verifier: Address,
    pub amount: i128,
    pub status: BountyMilestoneStatus,
    pub deliverable_hash: Option<BytesN<32>>,
}

pub enum BountyMilestoneStatus {
    Pending = 0,
    Submitted = 1,
    Verified = 2,
    Paid = 3,
}
```

### New Escrow Statuses
- `EscrowStatus::BountyUnclaimed = 14`: Bounty created but not yet claimed
- `EscrowStatus::BountyClaimed = 15`: Bounty claimed by a solver, awaiting submission

## New Events

### Basic Bounty Events
```rust
pub struct BountyCreated {
    pub escrow_id: u32,
    pub buyer: Address,
    pub amount: i128,
    pub token: Address,
    pub description_hash: BytesN<32>,
    pub claim_deadline_ledger: u64,
    pub submission_deadline_ledger: u64,
}

pub struct BountyClaimed {
    pub escrow_id: u32,
    pub solver: Address,
}

pub struct BountyWorkSubmitted {
    pub escrow_id: u32,
    pub solver: Address,
    pub submission_hash: BytesN<32>,
}

pub struct BountyAwarded {
    pub escrow_id: u32,
    pub solver: Address,
    pub amount: i128,
}

pub struct BountyRejected {
    pub escrow_id: u32,
    pub solver: Address,
    pub rejection_count: u32,
}

pub struct BountyCancelled {
    pub escrow_id: u32,
    pub buyer: Address,
    pub refund_amount: i128,
}
```

### Milestone Bounty Events
```rust
pub struct BountyMilestoneCreated {
    pub escrow_id: u32,
    pub buyer: Address,
    pub milestone_count: u32,
    pub total_amount: i128,
}

pub struct BountyMilestoneSubmitted {
    pub escrow_id: u32,
    pub index: u32,
    pub solver: Address,
    pub deliverable_hash: BytesN<32>,
}

pub struct MilestoneVerified {
    pub escrow_id: u32,
    pub index: u32,
    pub amount: i128,
    pub verifier: Address,
}

pub struct BountyVerifierReplaced {
    pub escrow_id: u32,
    pub index: u32,
    pub old_verifier: Address,
    pub new_verifier: Address,
}
```

## API Functions

### Bounty Board Functions
```rust
pub fn create_bounty(
    env: Env,
    buyer: Address,
    token: Address,
    amount: i128,
    description_hash: BytesN<32>,
    claim_deadline_ledger: u64,
    submission_deadline_ledger: u64,
) -> u32

pub fn claim_bounty(env: Env, solver: Address, escrow_id: u32)

pub fn submit_bounty_work(
    env: Env,
    solver: Address,
    escrow_id: u32,
    submission_hash: BytesN<32>,
)

pub fn approve_bounty_submission(env: Env, buyer: Address, escrow_id: u32)

pub fn reject_bounty_submission(env: Env, buyer: Address, escrow_id: u32)

pub fn cancel_bounty(env: Env, buyer: Address, escrow_id: u32)

pub fn set_max_bounty_rejection_rounds(env: Env, admin: Address, max_rounds: u32)

pub fn get_bounty_data(env: Env, escrow_id: u32) -> Option<BountyData>
```

### Milestone Bounty Functions
```rust
pub fn create_milestone_bounty(
    env: Env,
    buyer: Address,
    token: Address,
    milestones: Vec<BountyMilestoneInput>,
    claim_deadline_ledger: u64,
    submission_deadline_ledger: u64,
) -> u32

pub fn submit_bounty_milestone(
    env: Env,
    solver: Address,
    escrow_id: u32,
    index: u32,
    deliverable_hash: BytesN<32>,
)

pub fn verify_bounty_milestone(env: Env, escrow_id: u32, index: u32)

pub fn replace_bounty_verifier(
    env: Env,
    buyer: Address,
    escrow_id: u32,
    index: u32,
    new_verifier: Address,
)

pub fn get_bounty_milestones(env: Env, escrow_id: u32) -> Vec<BountyMilestone>
```

## Implementation Details

### Bounty Lifecycle
1. **Create**: Buyer calls `create_bounty()` with description hash, deadlines, and funds. Escrow created with `BountyUnclaimed` status. Bounty metadata stored in `BountyData`.
2. **Claim**: Any solver calls `claim_bounty()` which sets `seller` to the solver and status to `BountyClaimed`.`
3. **Submit Work**: Assigned solver calls `submit_bounty_work()` with a submission hash. Must be before `submission_deadline_ledger`.
4. **Approve**: Buyer calls `approve_bounty_submission()` which releases funds to the solver and sets status to `Released`.
5. **Reject**: Buyer calls `reject_bounty_submission()` which resets solver to `None`, clears submission hash, increments `rejection_count`, and returns status to `BountyUnclaimed`.
6. **Cancel**: Buyer calls `cancel_bounty()` on unclaimed bounties (or claimed bounties past deadline). Funds refunded to buyer, status set to `Refunded`.

### Milestone Lifecycle
1. **Create**: Buyer calls `create_milestone_bounty()` with an ordered list of `BountyMilestoneInput`. Full sum escrowed up front.
2. **Claim**: Standard `claim_bounty()` — milestone gating applies after claiming.
3. **Submit Milestone**: Solver calls `submit_bounty_milestone()` with index and deliverable hash. Previous milestone must be `Paid` (or index 0).
4. **Verify**: Designated verifier (or anyone matching milestone's `verifier` address) calls `verify_bounty_milestone()`. Tranche released, milestone set to `Paid`.
5. **Settle**: When all milestones are `Paid`, escrow status set to `Released`.

### Rejection Round Tracking
- `DEFAULT_MAX_BOUNTY_REJECTION_ROUNDS = 3` set during `initialize()`
- Each rejection increments `rejection_count` on the bounty
- When `rejection_count >= max_rounds`, `reject_bounty_submission()` panics
- Admin can call `set_max_bounty_rejection_rounds()` to change the global limit

## Constant
```rust
const DEFAULT_MAX_BOUNTY_REJECTION_ROUNDS: u32 = 3;
```

## Test Coverage
Comprehensive test suite covering both basic and milestone-gated bounty flows:

### Basic Bounty Tests (test_bounty_board.rs)
- ✅ `test_create_bounty_success`: Valid creation, status verification, bounty data
- ✅ `test_create_bounty_zero_amount`: Rejects zero amount
- ✅ `test_create_bounty_past_claim_deadline`: Rejects past claim deadline
- ✅ `test_create_bounty_invalid_deadlines`: Rejects reversed deadlines
- ✅ `test_claim_bounty_success`: Valid claim flow
- ✅ `test_claim_bounty_duplicate`: Rejects duplicate claim
- ✅ `test_claim_bounty_after_deadline`: Rejects claim after deadline
- ✅ `test_submit_bounty_work_success`: Valid submission flow
- ✅ `test_submit_bounty_work_wrong_solver`: Rejects wrong solver
- ✅ `test_submit_bounty_work_after_deadline`: Rejects late submission
- ✅ `test_approve_bounty_submission_success`: Valid approval and fund release
- ✅ `test_approve_bounty_without_submission`: Rejects approval without submission
- ✅ `test_reject_bounty_submission_and_reclaim`: Rejection resets state, new solver claims
- ✅ `test_reject_bounty_max_rejections`: Enforces max rejection rounds
- ✅ `test_cancel_bounty_unclaimed`: Cancel unclaimed bounty, full refund
- ✅ `test_cancel_bounty_after_inspection_fee`: Cancel milestone bounty after partial payout
- ✅ `test_cancel_bounty_claimed`: Rejects cancel on claimed bounty (early)
- ✅ `test_full_bounty_award_flow`: End-to-end create → claim → submit → approve
- ✅ `test_set_max_bounty_rejection_rounds`: Admin configures and verifies custom max rounds

### Milestone Bounty Tests (test_bounty_milestone.rs)
- ✅ `test_create_escrows_full_total_and_stores_milestones`: Full sum escrowed, milestone metadata stored
- ✅ `test_sequential_verification_releases_tranches_then_settles`: Ordered milestone verification
- ✅ `test_out_of_order_submission_is_rejected`: Prevents out-of-order submission
- ✅ `test_verify_before_submit_is_rejected`: Prevents verification before submission
- ✅ `test_replace_verifier_before_submission_then_flow_works`: Verifier replacement flow
- ✅ `test_replace_verifier_after_submission_is_rejected`: Prevents late verifier replacement
- ✅ `test_non_buyer_cannot_replace_verifier`: Access control on verifier replacement
- ✅ `test_non_solver_cannot_submit_milestone`: Access control on milestone submission
- ✅ `test_submit_milestone_before_claim_is_rejected`: Requires claim before milestone
- ✅ `test_multiple_verifier_replacements_before_submission`: Multiple verifier changes
- ✅ `test_all_milestones_can_be_verified_independently_by_different_verifiers`: Independent verifiers
- ✅ `test_out_of_order_submission_rejected`: #419 regression — out-of-order rejection
- ✅ `test_submit_invalid_milestone_index`: Rejects out-of-bounds milestone index

## Acceptance Criteria Status
- ✅ **Buyer can create bounty with description hash, claim deadline, and submission deadline**
- ✅ **Any solver can claim a bounty first-come-first-served**
- ✅ **Only the assigned solver can submit work**
- ✅ **Buyer can approve and release funds to solver**
- ✅ **Buyer can reject and re-open bounty for another solver**
- ✅ **Configurable maximum rejection rounds (default 3)**
- ✅ **Buyer can cancel unclaimed bounties and receive refund**
- ✅ **Milestone bounties with per-milestone verifiers and ordered sign-off chain**
- ✅ **Bounty creator can replace verifier before milestone submission**
- ✅ **Cancellation after partial milestone payout refunds only remaining funds**
- ✅ **Admin-configurable max bounty rejection rounds**
- ✅ **Events emitted for all bounty lifecycle transitions**

## Integration Notes
- **Backward Compatible**: All existing escrow functionality preserved. Bounties use new `EscrowStatus` variants and `DataKey2` storage keys.
- **Event Integration**: All bounty lifecycle transitions emit events following the existing pattern.
- **Storage Efficient**: Bounty metadata stored per-escrow; milestone schedules stored separately.
- **Token Whitelist**: Bounty creation respects the token whitelist if configured.
- **Milestone Base Points**: Milestone amounts can use BPS-based progressive release (see test_milestone_bps.rs).

## Security Considerations
- **Access Control**: Only the buyer can approve/reject/cancel their own bounty. Only the assigned solver can submit work. Only admin can set max rejection rounds.
- **Deadline Enforcement**: Claim and submission deadlines are enforced with proper validation.
- **Re-entrancy Safe**: Token transfers follow Soroban's safe patterns.
- **State Consistency**: Bounty state transitions are atomic and validated at each step.
- **Rejection Loop Protection**: Configurable max rejection rounds prevent indefinite re-opening.
- **Partial Payout Safety**: Cancellation after milestone payouts correctly refunds only remaining escrow balance.