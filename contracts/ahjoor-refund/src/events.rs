use soroban_sdk::{contractevent, Address, BytesN, Env, String, Symbol};

/// Event: Refund reason code recorded (#157)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundReasonRecorded {
    pub refund_id: u32,
    pub reason_code: u32,
}

/// Event: Refund auto-rejected after idle window (#158)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundAutoRejected {
    pub refund_id: u32,
    pub elapsed_seconds: u64,
}

/// Event: Customer appealed a rejected refund (#159)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundAppealed {
    pub refund_id: u32,
    pub customer: Address,
}

/// Event: Admin resolved a refund appeal (#159)
#[contractevent]
#[derive(Clone, Debug)]
pub struct AppealResolved {
    pub refund_id: u32,
    pub approved: bool,
}

/// Event: Refund requested
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundRequested {
    pub refund_id: u32,
    pub customer: Address,
    pub amount: i128,
    pub token: Address,
    pub reason: String,
}

/// Event: Refund approved
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundApproved {
    pub refund_id: u32,
    pub approved_by: Address,
    pub approved_at: u64,
}

/// Event: Refund rejected
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundRejected {
    pub refund_id: u32,
    pub rejected_by: Address,
    pub rejection_reason: String,
    pub rejected_at: u64,
}

/// Event: Refund processed (tokens transferred)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundProcessed {
    pub refund_id: u32,
    pub customer: Address,
    pub amount: i128,
    pub processed_at: u64,
}

/// Event: Contract WASM upgraded
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractUpgraded {
    pub old_version: u32,
    pub new_version: u32,
    pub by_admin: Address,
}

/// Event: Contract paused
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractPaused {
    pub admin: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Event: Contract resumed
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractResumed {
    pub admin: Address,
    pub timestamp: u64,
}

/// Event: Merchant made a counter-offer on a refund request
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundCounterOffered {
    pub refund_id: u32,
    pub merchant: Address,
    pub counter_amount: i128,
    pub expires_at: u64,
}

/// Event: Customer accepted the counter-offer
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundCounterAccepted {
    pub refund_id: u32,
    pub customer: Address,
    pub amount: i128,
}

/// Event: Customer rejected the counter-offer (escalated to admin)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundCounterRejected {
    pub refund_id: u32,
    pub customer: Address,
}

/// Event: Counter-offer expired and auto-settled
#[contractevent]
#[derive(Clone, Debug)]
pub struct CounterOfferExpired {
    pub refund_id: u32,
    /// true = original refund accepted, false = rejected
    pub resolution: bool,
    pub original_amount: i128,
}

/// Event: Refund auto-approved after dispute window elapsed without merchant response
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundAutoApproved {
    pub refund_id: u32,
    pub customer: Address,
    pub amount: i128,
}

/// Event: Refund auto-approved via whitelist
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundAutoApprovedWhitelist {
    pub refund_id: u32,
    pub merchant: Address,
    pub amount: i128,
}

/// Event: Escrow refund registered
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowRefundRegistered {
    pub refund_id: u32,
    pub escrow_id: u32,
    pub buyer: Address,
    pub amount: i128,
}

/// Event: Refund fee collected
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundFeeCollected {
    pub refund_id: u32,
    pub fee_amount: i128,
}

/// Event: Partial refund cap threshold reached
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialRefundCapApplied {
    pub refund_id: u32,
    pub remaining_refundable: i128,
}

/// Event: Tier applied to a refund request
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundTierApplied {
    pub refund_id: u32,
    pub tier_bps: u32,
    pub max_refundable: i128,
}

/// Event: Merchant initiated immediate refund
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantInitiatedRefund {
    pub refund_id: u32,
    pub payment_id: u32,
    pub merchant: Address,
    pub amount: i128,
    pub reason_code: u32,
}

/// Event: Bulk approved refunds processed
#[contractevent]
#[derive(Clone, Debug)]
pub struct BulkRefundProcessed {
    pub count: u32,
    pub total_amount: i128,
}

// --- Issue #166: Per-Customer Refund Request Cooldown Period ---

/// Event: Customer attempted a refund request within the cooldown window
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundCooldownActive {
    pub customer: Address,
    pub next_eligible_at: u64,
}

// --- Issue #167: Delegated Refund Approval for Merchant Sub-Admins ---

/// Event: A delegate was added for a merchant
#[contractevent]
#[derive(Clone, Debug)]
pub struct DelegateAdded {
    pub merchant: Address,
    pub delegate: Address,
}

/// Event: A refund was approved by a merchant delegate
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundApprovedByDelegate {
    pub refund_id: u32,
    pub delegate: Address,
}

// --- Issue #168: Refund Request Expiry with Auto-Cancellation ---

/// Event: A refund request was cancelled (by customer or auto-cancelled)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundRequestCancelled {
    pub refund_id: u32,
    pub cancelled_by: Address,
}

// --- Helper Emission Functions ---

pub fn emit_refund_requested(
    e: &Env,
    refund_id: u32,
    customer: Address,
    amount: i128,
    token: Address,
    reason: String,
) {
    RefundRequested {
        refund_id,
        customer,
        amount,
        token,
        reason,
    }
    .publish(e);
}

pub fn emit_refund_approved(e: &Env, refund_id: u32, approved_by: Address, approved_at: u64) {
    RefundApproved {
        refund_id,
        approved_by,
        approved_at,
    }
    .publish(e);
}

pub fn emit_refund_rejected(
    e: &Env,
    refund_id: u32,
    rejected_by: Address,
    rejection_reason: String,
    rejected_at: u64,
) {
    RefundRejected {
        refund_id,
        rejected_by,
        rejection_reason,
        rejected_at,
    }
    .publish(e);
}

pub fn emit_refund_processed(
    e: &Env,
    refund_id: u32,
    customer: Address,
    amount: i128,
    processed_at: u64,
) {
    RefundProcessed {
        refund_id,
        customer,
        amount,
        processed_at,
    }
    .publish(e);
}

pub fn emit_contract_upgraded(e: &Env, old_version: u32, new_version: u32, by_admin: Address) {
    ContractUpgraded {
        old_version,
        new_version,
        by_admin,
    }
    .publish(e);
}

pub fn emit_refund_auto_approved(e: &Env, refund_id: u32, customer: Address, amount: i128) {
    RefundAutoApproved {
        refund_id,
        customer,
        amount,
    }
    .publish(e);
}

pub fn emit_partial_refund_cap_applied(e: &Env, refund_id: u32, remaining_refundable: i128) {
    PartialRefundCapApplied {
        refund_id,
        remaining_refundable,
    }
    .publish(e);
}

pub fn emit_contract_paused(e: &Env, admin: Address, reason: String, timestamp: u64) {
    ContractPaused {
        admin,
        reason,
        timestamp,
    }
    .publish(e);
}

pub fn emit_contract_resumed(e: &Env, admin: Address, timestamp: u64) {
    ContractResumed { admin, timestamp }.publish(e);
}

pub fn emit_refund_auto_approved_whitelist(
    e: &Env,
    refund_id: u32,
    merchant: Address,
    amount: i128,
) {
    RefundAutoApprovedWhitelist {
        refund_id,
        merchant,
        amount,
    }
    .publish(e);
}

pub fn emit_escrow_refund_registered(
    e: &Env,
    refund_id: u32,
    escrow_id: u32,
    buyer: Address,
    amount: i128,
) {
    EscrowRefundRegistered {
        refund_id,
        escrow_id,
        buyer,
        amount,
    }
    .publish(e);
}

pub fn emit_refund_fee_collected(e: &Env, refund_id: u32, fee_amount: i128) {
    RefundFeeCollected {
        refund_id,
        fee_amount,
    }
    .publish(e);
}

pub fn emit_refund_reason_recorded(e: &Env, refund_id: u32, reason_code: u32) {
    RefundReasonRecorded {
        refund_id,
        reason_code,
    }
    .publish(e);
}

pub fn emit_refund_auto_rejected(e: &Env, refund_id: u32, elapsed_seconds: u64) {
    RefundAutoRejected {
        refund_id,
        elapsed_seconds,
    }
    .publish(e);
}

pub fn emit_refund_tier_applied(e: &Env, refund_id: u32, tier_bps: u32, max_refundable: i128) {
    RefundTierApplied {
        refund_id,
        tier_bps,
        max_refundable,
    }
    .publish(e);
}

pub fn emit_merchant_initiated_refund(
    e: &Env,
    refund_id: u32,
    payment_id: u32,
    merchant: Address,
    amount: i128,
    reason_code: u32,
) {
    MerchantInitiatedRefund {
        refund_id,
        payment_id,
        merchant,
        amount,
        reason_code,
    }
    .publish(e);
}

pub fn emit_appeal_resolved(e: &Env, refund_id: u32, approved: bool) {
    AppealResolved {
        refund_id,
        approved,
    }
    .publish(e);
}

pub fn emit_bulk_refund_processed(e: &Env, count: u32, total_amount: i128) {
    BulkRefundProcessed {
        count,
        total_amount,
    }
    .publish(e);
}

pub fn emit_refund_appealed(e: &Env, refund_id: u32, customer: Address) {
    RefundAppealed {
        refund_id,
        customer,
    }
    .publish(e);
}

// --- #166 ---

pub fn emit_refund_cooldown_active(e: &Env, customer: Address, next_eligible_at: u64) {
    RefundCooldownActive {
        customer,
        next_eligible_at,
    }
    .publish(e);
}

// --- #167 ---

pub fn emit_delegate_added(e: &Env, merchant: Address, delegate: Address) {
    DelegateAdded { merchant, delegate }.publish(e);
}

pub fn emit_refund_approved_by_delegate(e: &Env, refund_id: u32, delegate: Address) {
    RefundApprovedByDelegate {
        refund_id,
        delegate,
    }
    .publish(e);
}

// --- #168 ---

pub fn emit_refund_request_cancelled(e: &Env, refund_id: u32, cancelled_by: Address) {
    RefundRequestCancelled {
        refund_id,
        cancelled_by,
    }
    .publish(e);
}

// --- Fraud Score Events ---

/// Event: Fraud score updated for a buyer
#[contractevent]
#[derive(Clone, Debug)]
pub struct FraudScoreUpdated {
    pub buyer: Address,
    pub new_score: u32,
    pub reason: Symbol,
}

/// Event: Buyer blocked for exceeding fraud score threshold
#[contractevent]
#[derive(Clone, Debug)]
pub struct BuyerBlockedForFraud {
    pub buyer: Address,
    pub score: u32,
    pub threshold: u32,
}

/// Event: Buyer's fraud score manually reset by admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct FraudScoreReset {
    pub buyer: Address,
    pub reset_by: Address,
}

/// Event: Fraud score decay applied
#[contractevent]
#[derive(Clone, Debug)]
pub struct FraudScoreDecayApplied {
    pub buyer: Address,
    pub old_score: u32,
    pub new_score: u32,
}

/// Event: Fraud score block threshold updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct FraudBlockThresholdUpdated {
    pub old_threshold: u32,
    pub new_threshold: u32,
}

// --- Helper Emission Functions ---

pub fn emit_fraud_score_updated(e: &Env, buyer: Address, new_score: u32, reason: Symbol) {
    FraudScoreUpdated {
        buyer,
        new_score,
        reason,
    }
    .publish(e);
}

pub fn emit_buyer_blocked_for_fraud(e: &Env, buyer: Address, score: u32, threshold: u32) {
    BuyerBlockedForFraud {
        buyer,
        score,
        threshold,
    }
    .publish(e);
}

pub fn emit_fraud_score_reset(e: &Env, buyer: Address, reset_by: Address) {
    FraudScoreReset { buyer, reset_by }.publish(e);
}

pub fn emit_fraud_score_decay_applied(e: &Env, buyer: Address, old_score: u32, new_score: u32) {
    FraudScoreDecayApplied {
        buyer,
        old_score,
        new_score,
    }
    .publish(e);
}

pub fn emit_fraud_score_block_threshold_updated(e: &Env, old_threshold: u32, new_threshold: u32) {
    FraudBlockThresholdUpdated {
        old_threshold,
        new_threshold,
    }
    .publish(e);
}

pub fn emit_refund_counter_offered(
    e: &Env,
    refund_id: u32,
    merchant: Address,
    counter_amount: i128,
    expires_at: u64,
) {
    RefundCounterOffered {
        refund_id,
        merchant,
        counter_amount,
        expires_at,
    }
    .publish(e);
}

pub fn emit_refund_counter_accepted(e: &Env, refund_id: u32, customer: Address, amount: i128) {
    RefundCounterAccepted {
        refund_id,
        customer,
        amount,
    }
    .publish(e);
}

pub fn emit_refund_counter_rejected(e: &Env, refund_id: u32, customer: Address) {
    RefundCounterRejected {
        refund_id,
        customer,
    }
    .publish(e);
}

pub fn emit_counter_offer_expired(e: &Env, refund_id: u32, resolution: bool, original_amount: i128) {
    CounterOfferExpired {
        refund_id,
        resolution,
        original_amount,
    }
    .publish(e);
}

// --- Issue #228: Refund Merchant Auto-Approval Threshold ---

/// Event: Merchant set their auto-approval threshold
#[contractevent]
#[derive(Clone, Debug)]
pub struct AutoApproveThresholdSet {
    pub merchant: Address,
    pub amount: i128,
}

/// Event: Refund auto-approved because amount <= merchant threshold
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundApprovedByLimit {
    pub refund_id: u32,
    pub amount: i128,
}

pub fn emit_auto_approve_threshold_set(e: &Env, merchant: Address, amount: i128) {
    AutoApproveThresholdSet { merchant, amount }.publish(e);
}

pub fn emit_refund_auto_approved_by_threshold(e: &Env, refund_id: u32, amount: i128) {
    RefundApprovedByLimit { refund_id, amount }.publish(e);
}

// --- Issue #245: Partial Refund Approval ---

#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundPartiallyApproved {
    pub refund_id: u32,
    pub merchant: Address,
    pub approved_amount: i128,
    pub requested_amount: i128,
    pub remaining_unreturned: i128,
}

pub fn emit_refund_partially_approved(
    e: &Env,
    refund_id: u32,
    merchant: Address,
    approved_amount: i128,
    requested_amount: i128,
    remaining_unreturned: i128,
) {
    RefundPartiallyApproved {
        refund_id,
        merchant,
        approved_amount,
        requested_amount,
        remaining_unreturned,
    }
    .publish(e);
}

// --- Issue #238: Refund Priority ---

/// Event: Priority set or changed on a refund request
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundPrioritySet {
    pub refund_id: u32,
    pub new_priority: u32, // 0=High, 1=Medium, 2=Low
    pub set_by: Address,
}

pub fn emit_refund_priority_set(e: &Env, refund_id: u32, new_priority: u32, set_by: Address) {
    RefundPrioritySet {
        refund_id,
        new_priority,
        set_by,
    }
    .publish(e);
}

// --- Issue #274: Merchant Reserve Fund ---

pub fn emit_reserve_deposited(e: &Env, merchant: Address, amount: i128) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "ReserveDeposited"),),
        (merchant, amount),
    );
}

pub fn emit_reserve_withdrawn(e: &Env, merchant: Address, amount: i128) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "ReserveWithdrawn"),),
        (merchant, amount),
    );
}

pub fn emit_reserve_used_for_refund(e: &Env, merchant: Address, refund_id: u32, amount: i128) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "ReserveUsedForRefund"),),
        (merchant, refund_id, amount),
    );
}

pub fn emit_merchant_flagged_low_reserve(
    e: &Env,
    merchant: Address,
    current_reserve: i128,
    required_reserve: i128,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "MerchantFlaggedLowReserve"),),
        (merchant, current_reserve, required_reserve),
    );
}

// --- Issue #276: Merchant Counter-Dispute Evidence Window ---

pub fn emit_merchant_evidence_submitted(
    e: &Env,
    refund_id: u32,
    merchant: Address,
    num_hashes: u32,
    statement_hash: soroban_sdk::BytesN<32>,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "MerchantEvidenceSubmitted"),),
        (refund_id, merchant, num_hashes, statement_hash),
    );
}

pub fn emit_evidence_period_expired(e: &Env, refund_id: u32, merchant: Address) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "EvidencePeriodExpired"),),
        (refund_id, merchant),
    );
}

// --- Store-Credit Voucher Events ---

/// Event: Store-credit voucher issued as alternative to token refund
#[contractevent]
#[derive(Clone, Debug)]
pub struct StoreCreditIssued {
    pub refund_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub credit_amount: i128,
    pub expiry_ledger: u64,
}

/// Event: Store-credit redeemed against a payment
#[contractevent]
#[derive(Clone, Debug)]
pub struct StoreCreditRedeemed {
    pub payment_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub amount_used: i128,
    pub remaining_credit: i128,
}

pub fn emit_store_credit_issued(
    e: &Env,
    refund_id: u32,
    customer: Address,
    merchant: Address,
    credit_amount: i128,
    expiry_ledger: u64,
) {
    StoreCreditIssued {
        refund_id,
        customer,
        merchant,
        credit_amount,
        expiry_ledger,
    }
    .publish(e);
}

// --- New Events for #320, #334, #335 ---

/// Event: Merchant published refund policy (#320)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundPolicyPublished {
    pub merchant: Address,
    pub eligible_window_ledgers: u32,
    pub max_refund_bps: u32,
}

/// Event: Merchant reserve deposited (#334)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantReserveDeposited {
    pub merchant: Address,
    pub amount: i128,
    pub new_balance: i128,
}

/// Event: Merchant reserve low alert (#334)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantReserveLow {
    pub merchant: Address,
    pub balance: i128,
    pub required_minimum: i128,
}

/// Event: Review extension granted (#335)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ReviewExtensionGranted {
    pub refund_id: u32,
    pub new_deadline_ledger: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ReserveRequirementWaived {
    pub merchant: Address,
    pub expiry_ledger: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundPolicyUpdated {
    pub merchant: Address,
}

pub fn emit_refund_policy_published(
    e: &Env,
    merchant: Address,
    eligible_window_ledgers: u32,
    max_refund_bps: u32,
) {
    RefundPolicyPublished {
        merchant,
        eligible_window_ledgers,
        max_refund_bps,
    }
    .publish(e);
}

pub fn emit_merchant_reserve_deposited(
    e: &Env,
    merchant: Address,
    amount: i128,
    new_balance: i128,
) {
    MerchantReserveDeposited {
        merchant,
        amount,
        new_balance,
    }
    .publish(e);
}

pub fn emit_merchant_reserve_low(
    e: &Env,
    merchant: Address,
    balance: i128,
    required_minimum: i128,
) {
    MerchantReserveLow {
        merchant,
        balance,
        required_minimum,
    }
    .publish(e);
}

pub fn emit_review_extension_granted(e: &Env, refund_id: u32, new_deadline_ledger: u32) {
    ReviewExtensionGranted {
        refund_id,
        new_deadline_ledger,
    }
    .publish(e);
}

pub fn emit_reserve_requirement_waived(e: &Env, merchant: Address, expiry_ledger: u32) {
    ReserveRequirementWaived {
        merchant,
        expiry_ledger,
    }
    .publish(e);
}

pub fn emit_refund_policy_updated(e: &Env, merchant: Address) {
    RefundPolicyUpdated { merchant }.publish(e);
}

/// Event: legacy (#274) reserve balance migrated into canonical (#334) reserve (#585)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantReserveMigrated {
    pub merchant: Address,
    pub migrated_amount: i128,
    pub new_canonical_balance: i128,
}

pub fn emit_merchant_reserve_migrated(
    e: &Env,
    merchant: Address,
    migrated_amount: i128,
    new_canonical_balance: i128,
) {
    MerchantReserveMigrated {
        merchant,
        migrated_amount,
        new_canonical_balance,
    }
    .publish(e);
}

pub fn emit_store_credit_redeemed(
    e: &Env,
    payment_id: u32,
    customer: Address,
    merchant: Address,
    amount_used: i128,
    remaining_credit: i128,
) {
    StoreCreditRedeemed {
        payment_id,
        customer,
        merchant,
        amount_used,
        remaining_credit,
    }
    .publish(e);
}

// ─── Feature: Escalating Mediation Timeline ──────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundEscalated {
    pub refund_id: u32,
    pub escalated_by: Address,
    pub senior_arbiter: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct EscalatedRefundResolved {
    pub refund_id: u32,
    pub senior_arbiter: Address,
    pub approved: bool,
    pub resolution_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct RefundAutoApprovedSeniorMiss {
    pub refund_id: u32,
}

pub fn emit_refund_escalated(
    e: &Env,
    refund_id: u32,
    escalated_by: Address,
    senior_arbiter: Address,
) {
    RefundEscalated {
        refund_id,
        escalated_by,
        senior_arbiter,
    }
    .publish(e);
}

pub fn emit_escalated_refund_resolved(
    e: &Env,
    refund_id: u32,
    senior_arbiter: Address,
    approved: bool,
    resolution_hash: BytesN<32>,
) {
    EscalatedRefundResolved {
        refund_id,
        senior_arbiter,
        approved,
        resolution_hash,
    }
    .publish(e);
}

pub fn emit_refund_auto_approved_senior_miss(e: &Env, refund_id: u32) {
    RefundAutoApprovedSeniorMiss { refund_id }.publish(e);
}

// ─── Feature: Customer Abuse Score ───────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct CustomerAbuseScoreUpdated {
    pub customer: Address,
    pub new_score: u32,
    pub delta: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CustomerBlockedForAbuse {
    pub customer: Address,
    pub blocked_until_ledger: u64,
}

pub fn emit_customer_abuse_score_updated(e: &Env, customer: Address, new_score: u32, delta: u32) {
    CustomerAbuseScoreUpdated {
        customer,
        new_score,
        delta,
    }
    .publish(e);
}

pub fn emit_customer_blocked_for_abuse(e: &Env, customer: Address, blocked_until_ledger: u64) {
    CustomerBlockedForAbuse {
        customer,
        blocked_until_ledger,
    }
    .publish(e);
}

// ─── Issue #349: Partial Refund Processed ────────────────────────────────────

/// Event: A partial refund was processed; tracks cumulative progress per payment.
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialRefundProcessed {
    pub payment_id: u32,
    pub refund_amount: i128,
    pub remaining_amount: i128,
}

pub fn emit_partial_refund_processed(
    e: &Env,
    payment_id: u32,
    refund_amount: i128,
    remaining_amount: i128,
) {
    PartialRefundProcessed {
        payment_id,
        refund_amount,
        remaining_amount,
    }
    .publish(e);
}

// ─── Feature: Cross-Contract Refund Registration ─────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct CrossContractRefundRegistered {
    pub refund_id: u32,
    pub origin_contract: Address,
    pub origin_id: u32,
    pub customer: Address,
    pub amount: i128,
}

pub fn emit_cross_contract_refund_registered(
    e: &Env,
    refund_id: u32,
    origin_contract: Address,
    origin_id: u32,
    customer: Address,
    amount: i128,
) {
    CrossContractRefundRegistered {
        refund_id,
        origin_contract,
        origin_id,
        customer,
        amount,
    }
    .publish(e);
}

// ─── Issue #355: Configurable Abuse Score Decay ───────────────────────────────

/// Event: Abuse score decayed due to elapsed time without new violations
#[contractevent]
#[derive(Clone, Debug)]
pub struct AbuseScoreDecayed {
    pub buyer: Address,
    pub old_score: u32,
    pub new_score: u32,
    pub periods_elapsed: u32,
}

pub fn emit_abuse_score_decayed(
    e: &Env,
    buyer: Address,
    old_score: u32,
    new_score: u32,
    periods_elapsed: u32,
) {
    AbuseScoreDecayed {
        buyer,
        old_score,
        new_score,
        periods_elapsed,
    }
    .publish(e);
}

// ─── Issue #365: Evidence Hash Anchoring ─────────────────────────────────────

/// Event: Evidence content hash anchored on-chain for a refund (#365)
#[contractevent]
#[derive(Clone, Debug)]
pub struct EvidenceAnchored {
    pub refund_id: u32,
    pub submitter: Address,
    pub content_hash: BytesN<32>,
    pub timestamp: u64,
}

pub fn emit_evidence_anchored(
    e: &Env,
    refund_id: u32,
    submitter: Address,
    content_hash: BytesN<32>,
    timestamp: u64,
) {
    EvidenceAnchored {
        refund_id,
        submitter,
        content_hash,
        timestamp,
    }
    .publish(e);
}
