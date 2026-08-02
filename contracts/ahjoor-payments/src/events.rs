use crate::{PaymentStatus, SplitTransfer};
use soroban_sdk::{contractevent, Address, BytesN, Env, String, Symbol, Vec};

/// Event: Consent record created for zero-amount agreement signing (#307)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ConsentRecordCreated {
    pub consent_id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub terms_hash: BytesN<32>,
    pub terms_version: String,
}

/// Event: Consent record signed by customer (#307)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ConsentSigned {
    pub consent_id: u32,
    pub customer: Address,
    pub signed_at: u64,
}

/// Event: Consent record revoked (#307)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ConsentRevoked {
    pub consent_id: u32,
    pub revoked_by: Address,
    pub revoked_at: u64,
}

/// Event: Payment receipt issued on completion (#65)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentReceiptIssued {
    pub payment_id: u32,
    pub receipt_hash: BytesN<32>,
}

/// Event: Protocol fee collected on payment completion
#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCollected {
    pub payment_id: u32,
    pub fee_amount: i128,
    pub fee_recipient: Address,
    pub token: Address,
}

/// Event: Payment split distribution completed
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentSplitCompleted {
    pub payment_id: u32,
    pub splits: Vec<SplitTransfer>,
}

/// Event: Scheduled payment created
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentScheduled {
    pub payment_id: u32,
    pub execute_after: u64,
}

/// Event: Scheduled payment executed
#[contractevent]
#[derive(Clone, Debug)]
pub struct ScheduledPaymentExecuted {
    pub payment_id: u32,
}

/// Event: Merchant fee tier updated based on rolling 30-day volume
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantTierUpdated {
    pub merchant: Address,
    pub new_tier_bps: u32,
    pub volume: i128,
}

/// Event: Multi-token payment created (customer paid in non-USDC token)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MultiTokenPaymentCreated {
    pub payment_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub amount_usdc: i128,
    pub payment_token: Address,
    pub token_amount: i128,
    /// Oracle price used (scaled by 10^7)
    pub oracle_price: i128,
}

/// Event: Individual payment created
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentCreated {
    pub payment_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub notification_key: soroban_sdk::Bytes,
}

/// Event: Batch payment operation completed
#[contractevent]
#[derive(Clone, Debug)]
pub struct BatchPaymentCreated {
    pub customer: Address,
    pub payment_count: u32,
    pub total_amount: i128,
}

/// Event: Payment status changed
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentStatusChanged {
    pub payment_id: u32,
    pub old_status: PaymentStatus,
    pub new_status: PaymentStatus,
}

/// Event: Payment completed (released from escrow to merchant)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentCompleted {
    pub payment_id: u32,
    pub merchant: Address,
    pub amount: i128,
    pub completed_at: u64,
    pub notification_key: soroban_sdk::Bytes,
}

/// Event: Payment expired — funds returned to customer
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentExpired {
    pub payment_id: u32,
    pub customer: Address,
    pub amount: i128,
    pub expired_at: u64,
    pub notification_key: soroban_sdk::Bytes,
}

/// Event: Payment authorized by merchant — funds held in escrow (#127)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentAuthorized {
    pub payment_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub capture_deadline_ledger: u64,
}

/// Event: Payment created with a merchant-defined expiry override (#130)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentExpiryOverride {
    pub payment_id: u32,
    pub expiry_seconds: u64,
}

/// Event: Authorized payment captured by merchant — funds released (#127)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentCaptured {
    pub payment_id: u32,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CustomerBlocked {
    pub merchant: Address,
    pub customer: Address,
    pub reason_code: Symbol,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct UnblockRequested {
    pub request_id: u32,
    pub merchant: Address,
    pub customer: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CustomerUnblocked {
    pub merchant: Address,
    pub customer: Address,
    pub unblocked_by: Address,
}

pub fn emit_customer_blocked(e: &Env, merchant: Address, customer: Address, reason: Symbol) {
    CustomerBlocked { merchant, customer, reason_code: reason }.publish(e);
}

pub fn emit_unblock_requested(e: &Env, request_id: u32, merchant: Address, customer: Address) {
    UnblockRequested { request_id, merchant, customer }.publish(e);
}

pub fn emit_customer_unblocked(e: &Env, merchant: Address, customer: Address, by: Address) {
    CustomerUnblocked { merchant, customer, unblocked_by: by }.publish(e);
}

/// Event: Partial refund issued on a pending/disputed payment
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentPartialRefund {
    pub payment_id: u32,
    pub customer: Address,
    pub refund_amount: i128,
    pub remaining: i128,
}

/// Event: Subscription charged
#[contractevent]
#[derive(Clone, Debug)]
pub struct SubscriptionCharged {
    pub subscription_id: u32,
    pub subscriber: Address,
    pub merchant: Address,
    pub amount: i128,
    pub charged_at: u64,
}

/// Event: Subscription cancelled
#[contractevent]
#[derive(Clone, Debug)]
pub struct SubscriptionCancelled {
    pub subscription_id: u32,
    pub cancelled_by: Address,
}

/// Event: Subscription created with a trial period (#133)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SubscriptionTrialStarted {
    pub subscription_id: u32,
    pub trial_ends_at: u64,
}

/// Event: Subscription trial ended on first successful charge (#133)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SubscriptionTrialEnded {
    pub subscription_id: u32,
}

/// Event: Subscription plan changed with prorated credit applied to next charge
#[contractevent]
#[derive(Clone, Debug)]
pub struct SubscriptionPlanChanged {
    pub subscription_id: u32,
    pub old_plan: u32,
    pub new_plan: u32,
    pub prorated_credit: i128,
}

/// Event: Merchant settlement batch processed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct BatchSettlementProcessed {
    pub merchant: Address,
    pub total_amount: i128,
    pub fee_collected: i128,
    pub payment_count: u32,
}

/// Event: Merchant KYB verification hash set (#310)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantKYBSet {
    pub merchant: Address,
    pub kyb_hash: BytesN<32>,
    pub expiry_ledger: u64,
    pub jurisdiction: String,
}

/// Event: Merchant KYB verification revoked (#310)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantKYBRevoked {
    pub merchant: Address,
}

/// Event: Payment disputed by customer
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentDisputed {
    pub payment_id: u32,
    pub customer: Address,
    pub reason: String,
    pub notification_key: soroban_sdk::Bytes,
}

/// Event: Dispute resolved by admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeResolved {
    pub payment_id: u32,
    pub release_to_merchant: bool,
    pub resolved_by: Address,
}

/// Event: Dispute auto-escalated after timeout
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeEscalated {
    pub payment_id: u32,
    pub elapsed_seconds: u64,
}

/// Event: Admin transfer proposed
#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferProposed {
    pub current_admin: Address,
    pub proposed_admin: Address,
}

/// Event: Admin transfer accepted
#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
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

/// Event: Payment queued for merchant withdrawal (#126)
#[contractevent]
#[derive(Clone, Debug)]
pub struct WithdrawalQueued {
    pub merchant: Address,
    pub payment_id: u32,
    pub amount: i128,
}

/// Event: Merchant withdrawal queue processed (#126)
#[contractevent]
#[derive(Clone, Debug)]
pub struct WithdrawalProcessed {
    pub merchant: Address,
    pub total: u32,
}

/// Event: Invoice attached to payment (#128)
#[contractevent]
#[derive(Clone, Debug)]
pub struct InvoiceAttached {
    pub payment_id: u32,
    pub invoice_hash: BytesN<32>,
}

/// Event: Payment expiry extended by merchant
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentExpiryExtended {
    pub payment_id: u32,
    pub new_expires_at: u64,
    pub extension_count: u32,
}

/// Event: Installment plan created.
#[contractevent]
#[derive(Clone, Debug)]
pub struct InstallmentPlanCreated {
    pub plan_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub total_amount: i128,
    pub num_installments: u32,
}

/// Event: Installment settled.
#[contractevent]
#[derive(Clone, Debug)]
pub struct InstallmentSettled {
    pub plan_id: u32,
    pub installment_index: u32,
    pub amount_debited: i128,
}

/// Event: Installment plan completed.
#[contractevent]
#[derive(Clone, Debug)]
pub struct InstallmentPlanCompleted {
    pub plan_id: u32,
}

/// Event: Installment plan expired.
#[contractevent]
#[derive(Clone, Debug)]
pub struct InstallmentPlanExpired {
    pub plan_id: u32,
}

// --- Helper Emission Functions ---

pub fn emit_payment_created(
    e: &Env,
    payment_id: u32,
    customer: Address,
    merchant: Address,
    amount: i128,
    token: Address,
) {
    // Get notification key for the merchant
    let notification_key = e
        .storage()
        .persistent()
        .get(&crate::DataKey::MerchantNotificationKey(merchant.clone()))
        .unwrap_or(soroban_sdk::Bytes::new(e));

    PaymentCreated {
        payment_id,
        customer,
        merchant,
        amount,
        token,
        notification_key,
    }
    .publish(e);
}

pub fn emit_batch_payment_created(
    e: &Env,
    customer: Address,
    payment_count: u32,
    total_amount: i128,
) {
    BatchPaymentCreated {
        customer,
        payment_count,
        total_amount,
    }
    .publish(e);
}

pub fn emit_payment_status_changed(
    e: &Env,
    payment_id: u32,
    old_status: PaymentStatus,
    new_status: PaymentStatus,
) {
    PaymentStatusChanged {
        payment_id,
        old_status,
        new_status,
    }
    .publish(e);
}

pub fn emit_payment_completed(
    e: &Env,
    payment_id: u32,
    merchant: Address,
    amount: i128,
    completed_at: u64,
) {
    // Get notification key for the merchant
    let notification_key = e
        .storage()
        .persistent()
        .get(&crate::DataKey::MerchantNotificationKey(merchant.clone()))
        .unwrap_or(soroban_sdk::Bytes::new(e));

    PaymentCompleted {
        payment_id,
        merchant,
        amount,
        completed_at,
        notification_key,
    }
    .publish(e);
}

pub fn emit_payment_expired(
    e: &Env,
    payment_id: u32,
    customer: Address,
    amount: i128,
    expired_at: u64,
) {
    // Get the payment to find the merchant for notification key lookup
    let payment: crate::Payment = e
        .storage()
        .persistent()
        .get(&crate::DataKey::Payment(payment_id))
        .expect("Payment not found");

    // Get notification key for the merchant
    let notification_key = e
        .storage()
        .persistent()
        .get(&crate::DataKey::MerchantNotificationKey(payment.merchant))
        .unwrap_or(soroban_sdk::Bytes::new(e));

    PaymentExpired {
        payment_id,
        customer,
        amount,
        expired_at,
        notification_key,
    }
    .publish(e);
}

pub fn emit_payment_authorized(
    e: &Env,
    payment_id: u32,
    customer: Address,
    merchant: Address,
    amount: i128,
    capture_deadline_ledger: u64,
) {
    PaymentAuthorized {
        payment_id,
        customer,
        merchant,
        amount,
        capture_deadline_ledger,
    }
    .publish(e);
}

pub fn emit_payment_expiry_override(e: &Env, payment_id: u32, expiry_seconds: u64) {
    PaymentExpiryOverride {
        payment_id,
        expiry_seconds,
    }
    .publish(e);
}

pub fn emit_payment_captured(e: &Env, payment_id: u32, amount: i128) {
    PaymentCaptured { payment_id, amount }.publish(e);
}

pub fn emit_payment_partial_refund(
    e: &Env,
    payment_id: u32,
    customer: Address,
    refund_amount: i128,
    remaining: i128,
) {
    PaymentPartialRefund {
        payment_id,
        customer,
        refund_amount,
        remaining,
    }
    .publish(e);
}

pub fn emit_subscription_charged(
    e: &Env,
    subscription_id: u32,
    subscriber: Address,
    merchant: Address,
    amount: i128,
    charged_at: u64,
) {
    SubscriptionCharged {
        subscription_id,
        subscriber,
        merchant,
        amount,
        charged_at,
    }
    .publish(e);
}

pub fn emit_subscription_cancelled(e: &Env, subscription_id: u32, cancelled_by: Address) {
    SubscriptionCancelled {
        subscription_id,
        cancelled_by,
    }
    .publish(e);
}

pub fn emit_subscription_trial_started(e: &Env, subscription_id: u32, trial_ends_at: u64) {
    SubscriptionTrialStarted {
        subscription_id,
        trial_ends_at,
    }
    .publish(e);
}

pub fn emit_subscription_trial_ended(e: &Env, subscription_id: u32) {
    SubscriptionTrialEnded { subscription_id }.publish(e);
}

pub fn emit_subscription_plan_changed(
    e: &Env,
    subscription_id: u32,
    old_plan: u32,
    new_plan: u32,
    prorated_credit: i128,
) {
    SubscriptionPlanChanged {
        subscription_id,
        old_plan,
        new_plan,
        prorated_credit,
    }
    .publish(e);
}

pub fn emit_batch_settlement_processed(
    e: &Env,
    merchant: Address,
    total_amount: i128,
    fee_collected: i128,
    payment_count: u32,
) {
    BatchSettlementProcessed {
        merchant,
        total_amount,
        fee_collected,
        payment_count,
    }
    .publish(e);
}

pub fn emit_payment_disputed(e: &Env, payment_id: u32, customer: Address, reason: String) {
    // Get the payment to find the merchant for notification key lookup
    let payment: crate::Payment = e
        .storage()
        .persistent()
        .get(&crate::DataKey::Payment(payment_id))
        .expect("Payment not found");

    // Get notification key for the merchant
    let notification_key = e
        .storage()
        .persistent()
        .get(&crate::DataKey::MerchantNotificationKey(payment.merchant))
        .unwrap_or(soroban_sdk::Bytes::new(e));

    PaymentDisputed {
        payment_id,
        customer,
        reason,
        notification_key,
    }
    .publish(e);
}

pub fn emit_dispute_resolved(
    e: &Env,
    payment_id: u32,
    release_to_merchant: bool,
    resolved_by: Address,
) {
    DisputeResolved {
        payment_id,
        release_to_merchant,
        resolved_by,
    }
    .publish(e);
}

pub fn emit_dispute_escalated(e: &Env, payment_id: u32, elapsed_seconds: u64) {
    DisputeEscalated {
        payment_id,
        elapsed_seconds,
    }
    .publish(e);
}

pub fn emit_admin_transfer_proposed(e: &Env, current_admin: Address, proposed_admin: Address) {
    AdminTransferProposed {
        current_admin,
        proposed_admin,
    }
    .publish(e);
}

pub fn emit_admin_transferred(e: &Env, old_admin: Address, new_admin: Address) {
    AdminTransferred {
        old_admin,
        new_admin,
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

pub fn emit_payment_receipt_issued(e: &Env, payment_id: u32, receipt_hash: BytesN<32>) {
    PaymentReceiptIssued {
        payment_id,
        receipt_hash,
    }
    .publish(e);
}

pub fn emit_fee_collected(
    e: &Env,
    payment_id: u32,
    fee_amount: i128,
    fee_recipient: Address,
    token: Address,
) {
    FeeCollected {
        payment_id,
        fee_amount,
        fee_recipient,
        token,
    }
    .publish(e);
}

pub fn emit_payment_split_completed(e: &Env, payment_id: u32, splits: Vec<SplitTransfer>) {
    PaymentSplitCompleted { payment_id, splits }.publish(e);
}

pub fn emit_payment_scheduled(e: &Env, payment_id: u32, execute_after: u64) {
    PaymentScheduled {
        payment_id,
        execute_after,
    }
    .publish(e);
}

pub fn emit_scheduled_payment_executed(e: &Env, payment_id: u32) {
    ScheduledPaymentExecuted { payment_id }.publish(e);
}

pub fn emit_merchant_tier_updated(e: &Env, merchant: Address, new_tier_bps: u32, volume: i128) {
    MerchantTierUpdated {
        merchant,
        new_tier_bps,
        volume,
    }
    .publish(e);
}

#[allow(clippy::too_many_arguments)]
/// Event: Payment tagged with a category (#122)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentCategorized {
    pub payment_id: u32,
    pub merchant: Address,
    pub category: Symbol,
    pub tags: Vec<Symbol>,
}

/// Event: Bulk expire batch completed (#123)
#[contractevent]
#[derive(Clone, Debug)]
pub struct BulkExpireCompleted {
    pub expired_count: u32,
    pub refund_total: i128,
}

/// Event: Subscription paused by subscriber (#124)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SubscriptionPaused {
    pub sub_id: u32,
    pub paused_at: u64,
}

/// Event: Subscription resumed by subscriber (#124)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SubscriptionResumed {
    pub sub_id: u32,
    pub resumed_at: u64,
}

/// Event: Conditional payment completion attempt (#125)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ConditionalPaymentAttempt {
    pub payment_id: u32,
    pub oracle_price: i128,
    pub threshold: i128,
    pub met: bool,
}

pub fn emit_payment_categorized(
    e: &Env,
    payment_id: u32,
    merchant: Address,
    category: Symbol,
    tags: Vec<Symbol>,
) {
    PaymentCategorized {
        payment_id,
        merchant,
        category,
        tags,
    }
    .publish(e);
}

pub fn emit_bulk_expire_completed(e: &Env, expired_count: u32, refund_total: i128) {
    BulkExpireCompleted {
        expired_count,
        refund_total,
    }
    .publish(e);
}

pub fn emit_subscription_paused(e: &Env, sub_id: u32, paused_at: u64) {
    SubscriptionPaused { sub_id, paused_at }.publish(e);
}

pub fn emit_subscription_resumed(e: &Env, sub_id: u32, resumed_at: u64) {
    SubscriptionResumed { sub_id, resumed_at }.publish(e);
}

pub fn emit_conditional_payment_attempt(
    e: &Env,
    payment_id: u32,
    oracle_price: i128,
    threshold: i128,
    met: bool,
) {
    ConditionalPaymentAttempt {
        payment_id,
        oracle_price,
        threshold,
        met,
    }
    .publish(e);
}

pub fn emit_multi_token_payment_created(
    e: &Env,
    payment_id: u32,
    customer: Address,
    merchant: Address,
    amount_usdc: i128,
    payment_token: Address,
    token_amount: i128,
    oracle_price: i128,
) {
    MultiTokenPaymentCreated {
        payment_id,
        customer,
        merchant,
        amount_usdc,
        payment_token,
        token_amount,
        oracle_price,
    }
    .publish(e);
}

pub fn emit_withdrawal_queued(e: &Env, merchant: Address, payment_id: u32, amount: i128) {
    WithdrawalQueued {
        merchant,
        payment_id,
        amount,
    }
    .publish(e);
}

pub fn emit_withdrawal_processed(e: &Env, merchant: Address, total: u32) {
    WithdrawalProcessed { merchant, total }.publish(e);
}

pub fn emit_invoice_attached(e: &Env, payment_id: u32, invoice_hash: BytesN<32>) {
    InvoiceAttached {
        payment_id,
        invoice_hash,
    }
    .publish(e);
}

pub fn emit_payment_expiry_extended(
    e: &Env,
    payment_id: u32,
    new_expires_at: u64,
    extension_count: u32,
) {
    PaymentExpiryExtended {
        payment_id,
        new_expires_at,
        extension_count,
    }
    .publish(e);
}

// --- Collateral Events (#129) ---

/// Event: Merchant deposited collateral
#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralDeposited {
    pub merchant: Address,
    pub amount: i128,
}

/// Event: Merchant withdrew collateral
#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralWithdrawn {
    pub merchant: Address,
    pub amount: i128,
}

/// Event: Merchant collateral slashed due to dispute resolved for customer
#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralSlashed {
    pub merchant: Address,
    pub amount: i128,
    pub payment_id: u32,
}

pub fn emit_collateral_deposited(e: &Env, merchant: Address, amount: i128) {
    CollateralDeposited { merchant, amount }.publish(e);
}

pub fn emit_collateral_withdrawn(e: &Env, merchant: Address, amount: i128) {
    CollateralWithdrawn { merchant, amount }.publish(e);
}

pub fn emit_collateral_slashed(e: &Env, merchant: Address, amount: i128, payment_id: u32) {
    CollateralSlashed {
        merchant,
        amount,
        payment_id,
    }
    .publish(e);
}

// --- Slippage Tolerance Event (#135) ---

/// Event: Slippage tolerance applied to a multi-token payment
#[contractevent]
#[derive(Clone, Debug)]
pub struct SlippageToleranceApplied {
    pub payment_id: u32,
    pub tolerance_bps: u32,
    pub oracle_price: i128,
    pub settled_amount: i128,
}

pub fn emit_slippage_tolerance_applied(
    e: &Env,
    payment_id: u32,
    tolerance_bps: u32,
    oracle_price: i128,
    settled_amount: i128,
) {
    SlippageToleranceApplied {
        payment_id,
        tolerance_bps,
        oracle_price,
        settled_amount,
    }
    .publish(e);
}

// --- Volume Cap Event (#131) ---

/// Event: Merchant payment rejected due to volume cap
#[contractevent]
#[derive(Clone, Debug)]
pub struct VolumeCapped {
    pub merchant: Address,
    pub payment_id: u32,
    pub current_volume: i128,
    pub cap: i128,
}

pub fn emit_volume_capped(
    e: &Env,
    merchant: Address,
    payment_id: u32,
    current_volume: i128,
    cap: i128,
) {
    VolumeCapped {
        merchant,
        payment_id,
        current_volume,
        cap,
    }
    .publish(e);
}

// --- Notification Key Events ---

/// Event: Merchant registered a notification key
#[contractevent]
#[derive(Clone, Debug)]
pub struct NotificationKeyRegistered {
    pub merchant: Address,
    pub key: soroban_sdk::Bytes,
}

/// Event: Merchant removed their notification key
#[contractevent]
#[derive(Clone, Debug)]
pub struct NotificationKeyRemoved {
    pub merchant: Address,
}

/// Event: Notification key rotated (#377)
#[contractevent]
#[derive(Clone, Debug)]
pub struct NotificationKeyRotated {
    pub merchant: Address,
    pub old_key_hash: BytesN<32>,
    pub new_key_hash: BytesN<32>,
    pub overlap_until: u64,
}

pub fn emit_notification_key_registered(e: &Env, merchant: Address, key: soroban_sdk::Bytes) {
    NotificationKeyRegistered { merchant, key }.publish(e);
}

pub fn emit_notification_key_removed(e: &Env, merchant: Address) {
    NotificationKeyRemoved { merchant }.publish(e);
}

pub fn emit_notification_key_rotated(
    e: &Env,
    merchant: Address,
    old_key_hash: BytesN<32>,
    new_key_hash: BytesN<32>,
    overlap_until: u64,
) {
    NotificationKeyRotated {
        merchant,
        old_key_hash,
        new_key_hash,
        overlap_until,
    }
    .publish(e);
}

// --- Token Swap Events ---

/// Event: Payment swapped and settled
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentSwappedAndSettled {
    pub payment_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub input_token: Address,
    pub output_token: Address,
    pub input_amount: i128,
    pub output_amount: i128,
}

/// Event: Payment swap failed
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentSwapFailed {
    pub payment_id: u32,
    pub customer: Address,
    pub token: Address,
    pub amount: i128,
    pub reason: Symbol,
}

/// Event: Merchant preferred token set
#[contractevent]
#[derive(Clone, Debug)]
pub struct PreferredTokenSet {
    pub merchant: Address,
    pub token: Address,
}

/// Event: Swap router set
#[contractevent]
#[derive(Clone, Debug)]
pub struct SwapRouterSet {
    pub router: Address,
}

// --- Helper Emission Functions ---

pub fn emit_payment_swapped_and_settled(
    e: &Env,
    payment_id: u32,
    customer: Address,
    merchant: Address,
    input_token: Address,
    output_token: Address,
    input_amount: i128,
    output_amount: i128,
) {
    PaymentSwappedAndSettled {
        payment_id,
        customer,
        merchant,
        input_token,
        output_token,
        input_amount,
        output_amount,
    }
    .publish(e);
}

pub fn emit_payment_swap_failed(
    e: &Env,
    payment_id: u32,
    customer: Address,
    token: Address,
    amount: i128,
    reason: Symbol,
) {
    PaymentSwapFailed {
        payment_id,
        customer,
        token,
        amount,
        reason,
    }
    .publish(e);
}

pub fn emit_preferred_token_set(e: &Env, merchant: Address, token: Address) {
    PreferredTokenSet { merchant, token }.publish(e);
}

pub fn emit_swap_router_set(e: &Env, router: Address) {
    SwapRouterSet { router }.publish(e);
}

// --- Merchant Ban/Appeal Events ---

/// Event: Merchant suspended by admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantSuspended {
    pub merchant: Address,
    pub reason_hash: BytesN<32>,
    pub suspension_expires_at: u64,
}

/// Event: Merchant permanently banned by admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantBanned {
    pub merchant: Address,
    pub reason_hash: BytesN<32>,
}

/// Event: Banned merchant submitted an appeal
#[contractevent]
#[derive(Clone, Debug)]
pub struct AppealSubmitted {
    pub merchant: Address,
    pub evidence_hash: BytesN<32>,
}

/// Event: Admin approved a merchant appeal — cooling-off period begins
#[contractevent]
#[derive(Clone, Debug)]
pub struct AppealApproved {
    pub merchant: Address,
    pub cooling_off_until: u64,
}

/// Event: Admin rejected a merchant appeal
#[contractevent]
#[derive(Clone, Debug)]
pub struct AppealRejected {
    pub merchant: Address,
}

/// Event: Merchant fully reinstated after cooling-off period
#[contractevent]
#[derive(Clone, Debug)]
pub struct MerchantReinstated {
    pub merchant: Address,
    pub reinstated_at: u64,
}

pub fn emit_merchant_suspended(e: &Env, merchant: Address, reason_hash: BytesN<32>, suspension_expires_at: u64) {
    MerchantSuspended { merchant, reason_hash, suspension_expires_at }.publish(e);
}

pub fn emit_merchant_banned(e: &Env, merchant: Address, reason_hash: BytesN<32>) {
    MerchantBanned {
        merchant,
        reason_hash,
    }
    .publish(e);
}

pub fn emit_appeal_submitted(e: &Env, merchant: Address, evidence_hash: BytesN<32>) {
    AppealSubmitted {
        merchant,
        evidence_hash,
    }
    .publish(e);
}

pub fn emit_appeal_approved(e: &Env, merchant: Address, cooling_off_until: u64) {
    AppealApproved { merchant, cooling_off_until }.publish(e);
}

pub fn emit_merchant_reinstated(e: &Env, merchant: Address, reinstated_at: u64) {
    MerchantReinstated { merchant, reinstated_at }.publish(e);
}

pub fn emit_appeal_rejected(e: &Env, merchant: Address) {
    AppealRejected { merchant }.publish(e);
}

// --- #216: Recurring Invoice Events ---

/// Event: Recurring invoice schedule created
#[contractevent]
#[derive(Clone, Debug)]
pub struct RecurringInvoiceCreated {
    pub invoice_id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub amount: i128,
}

/// Event: Invoice cycle triggered, creating a new payment
#[contractevent]
#[derive(Clone, Debug)]
pub struct InvoiceCycleTriggered {
    pub invoice_id: u32,
    pub payment_id: u32,
    pub cycle_number: u32,
}

/// Event: Recurring invoice cancelled
#[contractevent]
#[derive(Clone, Debug)]
pub struct RecurringInvoiceCancelled {
    pub invoice_id: u32,
    pub cancelled_by: Address,
}

/// Event: Recurring invoice completed (max cycles reached)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RecurringInvoiceCompleted {
    pub invoice_id: u32,
}

pub fn emit_recurring_invoice_created(
    e: &Env,
    invoice_id: u32,
    merchant: Address,
    customer: Address,
    amount: i128,
) {
    RecurringInvoiceCreated {
        invoice_id,
        merchant,
        customer,
        amount,
    }
    .publish(e);
}

pub fn emit_invoice_cycle_triggered(e: &Env, invoice_id: u32, payment_id: u32, cycle_number: u32) {
    InvoiceCycleTriggered {
        invoice_id,
        payment_id,
        cycle_number,
    }
    .publish(e);
}

pub fn emit_recurring_invoice_cancelled(e: &Env, invoice_id: u32, cancelled_by: Address) {
    RecurringInvoiceCancelled {
        invoice_id,
        cancelled_by,
    }
    .publish(e);
}

pub fn emit_recurring_invoice_completed(e: &Env, invoice_id: u32) {
    RecurringInvoiceCompleted { invoice_id }.publish(e);
}

// #231: Withdrawal Rate Limiting Events
pub fn emit_withdrawal_rate_limit_set(e: &Env, merchant: Address, window_seconds: u64, cap: i128) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "WdrlLimitSet"),),
        (merchant, window_seconds, cap),
    );
}
pub fn emit_withdrawal_rate_limit_exceeded(e: &Env, merchant: Address, attempted: i128, cap: i128) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "WdrlLimitExceeded"),),
        (merchant, attempted, cap),
    );
}

// #239: Loyalty Points Events

/// Event: Points accrued to customer after payment completion
#[contractevent]
#[derive(Clone, Debug)]
pub struct PointsAccrued {
    pub customer: Address,
    pub payment_id: u32,
    pub points_earned: i128,
    pub balance: i128,
}

/// Event: Points redeemed as discount on a payment
#[contractevent]
#[derive(Clone, Debug)]
pub struct PointsRedeemed {
    pub customer: Address,
    pub payment_id: u32,
    pub points_used: i128,
    pub discount_applied: i128,
}

/// Event: Expired points burned on next interaction
#[contractevent]
#[derive(Clone, Debug)]
pub struct PointsExpired {
    pub customer: Address,
    pub points_expired: i128,
}

pub fn emit_points_accrued(
    e: &Env,
    customer: Address,
    payment_id: u32,
    points_earned: i128,
    balance: i128,
) {
    PointsAccrued {
        customer,
        payment_id,
        points_earned,
        balance,
    }
    .publish(e);
}

pub fn emit_points_redeemed(
    e: &Env,
    customer: Address,
    payment_id: u32,
    points_used: i128,
    discount_applied: i128,
) {
    PointsRedeemed {
        customer,
        payment_id,
        points_used,
        discount_applied,
    }
    .publish(e);
}

pub fn emit_points_expired(e: &Env, customer: Address, points_expired: i128) {
    PointsExpired {
        customer,
        points_expired,
    }
    .publish(e);
}

// #242: Merchant Referral Commission Events
/// Event: Referral registered
#[contractevent]
#[derive(Clone, Debug)]
pub struct ReferralRegistered {
    pub referrer: Address,
    pub referred_merchant: Address,
}

/// Event: Commission accrued on referred merchant payment
#[contractevent]
#[derive(Clone, Debug)]
pub struct CommissionAccrued {
    pub referrer: Address,
    pub referred_merchant: Address,
    pub payment_id: u32,
    pub amount: i128,
}

/// Event: Referrer claimed accumulated commission
#[contractevent]
#[derive(Clone, Debug)]
pub struct CommissionClaimed {
    pub referrer: Address,
    pub amount: i128,
}

pub fn emit_referral_registered(e: &Env, referrer: Address, referred_merchant: Address) {
    ReferralRegistered {
        referrer,
        referred_merchant,
    }
    .publish(e);
}

pub fn emit_commission_accrued(
    e: &Env,
    referrer: Address,
    referred_merchant: Address,
    payment_id: u32,
    amount: i128,
) {
    CommissionAccrued {
        referrer,
        referred_merchant,
        payment_id,
        amount,
    }
    .publish(e);
}

pub fn emit_commission_claimed(e: &Env, referrer: Address, amount: i128) {
    CommissionClaimed { referrer, amount }.publish(e);
}

// #246: Dynamic Settlement Events
/// Event: Dynamic payment created with oracle price feed
#[contractevent]
#[derive(Clone, Debug)]
pub struct DynamicPaymentCreated {
    pub payment_id: u32,
    pub fiat_amount: i128,
    pub fiat_currency: soroban_sdk::Symbol,
    pub oracle_address: Address,
}

/// Event: Dynamic payment settled with oracle rate
#[contractevent]
#[derive(Clone, Debug)]
pub struct DynamicPaymentSettled {
    pub payment_id: u32,
    pub fiat_amount: i128,
    pub token_amount: i128,
    pub rate_used: i128,
}

pub fn emit_dynamic_payment_created(
    e: &Env,
    payment_id: u32,
    fiat_amount: i128,
    fiat_currency: soroban_sdk::Symbol,
    oracle_address: Address,
) {
    DynamicPaymentCreated {
        payment_id,
        fiat_amount,
        fiat_currency,
        oracle_address,
    }
    .publish(e);
}

pub fn emit_dynamic_payment_settled(
    e: &Env,
    payment_id: u32,
    fiat_amount: i128,
    token_amount: i128,
    rate_used: i128,
) {
    DynamicPaymentSettled {
        payment_id,
        fiat_amount,
        token_amount,
        rate_used,
    }
    .publish(e);
}

// #235: Customer Spend Limit Events
/// Event: Merchant set a per-customer spend limit
#[contractevent]
#[derive(Clone, Debug)]
pub struct CustomerSpendLimitSet {
    pub merchant: Address,
    pub customer: Address,
    pub amount: i128,
    pub window_seconds: u64,
}

/// Event: Customer spend limit removed
#[contractevent]
#[derive(Clone, Debug)]
pub struct CustomerSpendLimitRemoved {
    pub merchant: Address,
    pub customer: Address,
}

/// Event: Customer spend limit exceeded (payment rejected)
#[contractevent]
#[derive(Clone, Debug)]
pub struct CustomerSpendLimitExceeded {
    pub merchant: Address,
    pub customer: Address,
    pub attempted: i128,
    pub cap: i128,
}

/// Event: Merchant default spend limit set
#[contractevent]
#[derive(Clone, Debug)]
pub struct DefaultSpendLimitSet {
    pub merchant: Address,
    pub amount: i128,
    pub window_seconds: u64,
}

pub fn emit_customer_spend_limit_set(
    e: &Env,
    merchant: Address,
    customer: Address,
    amount: i128,
    window_seconds: u64,
) {
    CustomerSpendLimitSet {
        merchant,
        customer,
        amount,
        window_seconds,
    }
    .publish(e);
}

pub fn emit_customer_spend_limit_removed(e: &Env, merchant: Address, customer: Address) {
    CustomerSpendLimitRemoved { merchant, customer }.publish(e);
}

pub fn emit_customer_spend_limit_exceeded(
    e: &Env,
    merchant: Address,
    customer: Address,
    attempted: i128,
    cap: i128,
) {
    CustomerSpendLimitExceeded {
        merchant,
        customer,
        attempted,
        cap,
    }
    .publish(e);
}

pub fn emit_default_spend_limit_set(e: &Env, merchant: Address, amount: i128, window_seconds: u64) {
    DefaultSpendLimitSet {
        merchant,
        amount,
        window_seconds,
    }
    .publish(e);
}

// #265: Tip / Gratuity Events

/// Event: Customer tip forwarded to merchant at payment settlement (#265)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TipReceived {
    pub payment_id: u32,
    pub merchant: Address,
    pub tip_amount: i128,
    pub token: Address,
}

/// Event: Tip split to beneficiary (#370)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TipSplit {
    pub payment_id: u32,
    pub beneficiary: Address,
    pub amount: i128,
    pub token: Address,
}

pub fn emit_tip_received(
    e: &Env,
    payment_id: u32,
    merchant: Address,
    tip_amount: i128,
    token: Address,
) {
    TipReceived {
        payment_id,
        merchant,
        tip_amount,
        token,
    }
    .publish(e);
}

pub fn emit_tip_split(
    e: &Env,
    payment_id: u32,
    beneficiary: Address,
    amount: i128,
    token: Address,
) {
    TipSplit {
        payment_id,
        beneficiary,
        amount,
        token,
    }
    .publish(e);
}

// Multi-sig approval events
pub fn emit_payment_approval_expired(e: &Env, payment_id: u32) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "PmtApprovalExp"),),
        (payment_id,),
    );
}

pub fn emit_payment_approved(e: &Env, payment_id: u32, signer: Address) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "PmtApproved"),),
        (payment_id, signer),
    );
}

// Voucher events
pub fn emit_voucher_issued(
    e: &Env,
    merchant: Address,
    code_hash: soroban_sdk::BytesN<32>,
    _discount_type: crate::DiscountType,
    discount_value: u32,
    max_uses: u32,
    expiry: u64,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "VoucherIssued"),),
        (merchant, code_hash, discount_value, max_uses, expiry),
    );
}

pub fn emit_voucher_revoked(e: &Env, merchant: Address, code_hash: soroban_sdk::BytesN<32>) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "VoucherRevoked"),),
        (merchant, code_hash),
    );
}

pub fn emit_voucher_redeemed(
    e: &Env,
    merchant: Address,
    code_hash: soroban_sdk::BytesN<32>,
    customer: Address,
    discount: i128,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "VoucherRedeemed"),),
        (merchant, code_hash, customer, discount),
    );
}

pub fn emit_voucher_exhausted(e: &Env, merchant: Address, code_hash: soroban_sdk::BytesN<32>) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "VoucherExhausted"),),
        (merchant, code_hash),
    );
}

// External ID indexing event
pub fn emit_payment_indexed_by_external_id(
    e: &Env,
    payment_id: u32,
    ext_id: soroban_sdk::BytesN<32>,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "PmtIndexedExtId"),),
        (payment_id, ext_id),
    );
}

// Consent record events (#307)
pub fn emit_consent_record_created(
    e: &Env,
    consent_id: u32,
    merchant: Address,
    customer: Address,
    terms_hash: BytesN<32>,
    terms_version: String,
) {
    ConsentRecordCreated {
        consent_id,
        merchant,
        customer,
        terms_hash,
        terms_version,
    }
    .publish(e);
}

pub fn emit_consent_signed(e: &Env, consent_id: u32, customer: Address, signed_at: u64) {
    ConsentSigned {
        consent_id,
        customer,
        signed_at,
    }
    .publish(e);
}

pub fn emit_consent_revoked(e: &Env, consent_id: u32, revoked_by: Address, revoked_at: u64) {
    ConsentRevoked {
        consent_id,
        revoked_by,
        revoked_at,
    }
    .publish(e);
}
// --- Dispute Evidence Events (#308) ---

/// Event: Evidence submitted for a dispute
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeEvidenceSubmitted {
    pub payment_id: u32,
    pub submitter: Address,
    pub evidence_hash: BytesN<32>,
    pub evidence_type: Symbol,
}

/// Event: Evidence submission window closed
#[contractevent]
#[derive(Clone, Debug)]
pub struct EvidenceWindowClosed {
    pub payment_id: u32,
}

pub fn emit_dispute_evidence_submitted(
    e: &Env,
    payment_id: u32,
    submitter: Address,
    evidence_hash: BytesN<32>,
    evidence_type: Symbol,
) {
    DisputeEvidenceSubmitted {
        payment_id,
        submitter,
        evidence_hash,
        evidence_type,
    }
    .publish(e);
}

pub fn emit_evidence_window_closed(e: &Env, payment_id: u32) {
    EvidenceWindowClosed { payment_id }.publish(e);
}

// --- Cooling-Off Events (#309) ---

/// Event: Payment entered cooling-off period
#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentInCoolingOff {
    pub payment_id: u32,
    pub customer: Address,
    pub expiry_ledger: u32,
}

/// Event: Payment cancelled during cooling-off period
#[contractevent]
#[derive(Clone, Debug)]
pub struct CoolingOffCancellation {
    pub payment_id: u32,
    pub customer: Address,
    pub refund_amount: i128,
}

pub fn emit_payment_in_cooling_off(
    e: &Env,
    payment_id: u32,
    customer: Address,
    expiry_ledger: u32,
) {
    PaymentInCoolingOff {
        payment_id,
        customer,
        expiry_ledger,
    }
    .publish(e);
}

pub fn emit_cooling_off_cancellation(
    e: &Env,
    payment_id: u32,
    customer: Address,
    refund_amount: i128,
) {
    CoolingOffCancellation {
        payment_id,
        customer,
        refund_amount,
    }
    .publish(e);
}

pub fn emit_merchant_kyb_set(
    env: &Env,
    merchant: Address,
    kyb_hash: BytesN<32>,
    expiry_ledger: u64,
    jurisdiction: String,
) {
    MerchantKYBSet {
        merchant,
        kyb_hash,
        expiry_ledger,
        jurisdiction,
    }
    .publish(env);
}

pub fn emit_merchant_kyb_revoked(env: &Env, merchant: Address) {
    MerchantKYBRevoked { merchant }.publish(env);
}
// ── #329: Failed Auto-Debit Retry Queue Events ────────────────────────────────

pub fn emit_debit_failed(
    e: &Env,
    record_id: u32,
    plan_id: u32,
    attempt_number: u32,
    next_retry_ledger: u64,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "DebitFailed"),),
        (record_id, plan_id, attempt_number, next_retry_ledger),
    );
}

pub fn emit_debit_retry_succeeded(e: &Env, record_id: u32, plan_id: u32, amount: i128) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "DebitRetrySucceeded"),),
        (record_id, plan_id, amount),
    );
}

pub fn emit_debit_abandoned(e: &Env, record_id: u32, plan_id: u32) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "DebitAbandoned"),),
        (record_id, plan_id),
    );
}

// --- #327: Subscription Pause/Resume with Prorated Billing ---

pub fn emit_subscription_paused_v2(
    e: &Env,
    sub_id: u32,
    paused_by: Address,
    paused_at_ledger: u32,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "SubscriptionPausedV2"),),
        (sub_id, paused_by, paused_at_ledger),
    );
}

pub fn emit_subscription_resumed_v2(
    e: &Env,
    sub_id: u32,
    resumed_by: Address,
    next_due_ledger: u32,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "SubscriptionResumedV2"),),
        (sub_id, resumed_by, next_due_ledger),
    );
}

// ── #351: Recurring Payment Scheduler ────────────────────────────────────────

pub fn emit_recurring_payment_executed(
    e: &soroban_sdk::Env,
    schedule_id: u32,
    cycle: u32,
    amount: i128,
    next_due: u64,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "RecurringExec"),),
        (schedule_id, cycle, amount, next_due),
    );
}

pub fn emit_recurring_payment_cancelled(e: &soroban_sdk::Env, schedule_id: u32, payer: soroban_sdk::Address) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "RecurringCancel"),),
        (schedule_id, payer),
    );
}

// ── #358: Buyer Trust Tier Events ────────────────────────────────────────────

/// Event: Merchant updated a buyer's trust tier (#358)
#[contractevent]
#[derive(Clone, Debug)]
pub struct BuyerTierUpdated {
    pub merchant: Address,
    pub buyer: Address,
    pub old_tier: u32,
    pub new_tier: u32,
}

pub fn emit_buyer_tier_updated(
    e: &Env,
    merchant: Address,
    buyer: Address,
    old_tier: u32,
    new_tier: u32,
) {
    BuyerTierUpdated { merchant, buyer, old_tier, new_tier }.publish(e);
}

// ── #367: Dynamic Settlement Fee Tiers ───────────────────────────────────────

/// Event: Fee tier applied during merchant settlement (#367)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TierFeeApplied {
    pub merchant: Address,
    pub tier_fee_bps: u32,
    pub fee_collected: i128,
    pub volume_30d: i128,
}

pub fn emit_tier_fee_applied(
    e: &Env,
    merchant: Address,
    tier_fee_bps: u32,
    fee_collected: i128,
    volume_30d: i128,
) {
    TierFeeApplied { merchant, tier_fee_bps, fee_collected, volume_30d }.publish(e);
}

// ── Dispute Mediation DAO Events ──────────────────────────────────────────────

/// Event: DAO mediation configuration updated by admin.
#[contractevent]
#[derive(Clone, Debug)]
pub struct DaoConfigured {
    pub member_count: u32,
    pub vote_window_seconds: u64,
    pub min_votes: u32,
}

/// Event: Disputed payment escalated to the DAO for mediation.
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeEscalatedToDao {
    pub payment_id: u32,
    pub case_id: u32,
    pub initiated_by: Address,
    pub vote_window_closes_at: u64,
}

/// Event: DAO member cast a vote on a mediation case.
#[contractevent]
#[derive(Clone, Debug)]
pub struct DaoVoteCast {
    pub case_id: u32,
    pub voter: Address,
    pub for_merchant: bool,
    pub votes_for_merchant: u32,
    pub votes_for_customer: u32,
}

/// Event: DAO verdict executed — dispute resolved by quorum.
#[contractevent]
#[derive(Clone, Debug)]
pub struct DaoVerdictExecuted {
    pub case_id: u32,
    pub payment_id: u32,
    pub merchant_wins: bool,
    pub votes_for_merchant: u32,
    pub votes_for_customer: u32,
}

/// Event: DAO escalation was cancelled before voting began.
#[contractevent]
#[derive(Clone, Debug)]
pub struct DaoEscalationCancelled {
    pub payment_id: u32,
    pub case_id: u32,
}

pub fn emit_dao_configured(e: &Env, member_count: u32, vote_window_seconds: u64, min_votes: u32) {
    DaoConfigured { member_count, vote_window_seconds, min_votes }.publish(e);
}

pub fn emit_dispute_escalated_to_dao(
    e: &Env,
    payment_id: u32,
    case_id: u32,
    initiated_by: Address,
    vote_window_closes_at: u64,
) {
    DisputeEscalatedToDao { payment_id, case_id, initiated_by, vote_window_closes_at }.publish(e);
}

pub fn emit_dao_vote_cast(
    e: &Env,
    case_id: u32,
    voter: Address,
    for_merchant: bool,
    votes_for_merchant: u32,
    votes_for_customer: u32,
) {
    DaoVoteCast { case_id, voter, for_merchant, votes_for_merchant, votes_for_customer }.publish(e);
}

pub fn emit_dao_verdict_executed(
    e: &Env,
    case_id: u32,
    payment_id: u32,
    merchant_wins: bool,
    votes_for_merchant: u32,
    votes_for_customer: u32,
) {
    DaoVerdictExecuted { case_id, payment_id, merchant_wins, votes_for_merchant, votes_for_customer }.publish(e);
}

pub fn emit_dao_escalation_cancelled(e: &Env, payment_id: u32, case_id: u32) {
    DaoEscalationCancelled { payment_id, case_id }.publish(e);
}
