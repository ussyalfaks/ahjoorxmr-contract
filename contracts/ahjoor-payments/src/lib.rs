#![no_std]
#![allow(deprecated, unused_imports, unused_variables, dead_code, unused_mut)]

use ahjoor_token_whitelist::TokenWhitelistClient;
use soroban_sdk::xdr::ToXdr;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Bytes,
    BytesN, Env, Map, String, Symbol, Vec,
};

pub mod multi_token_invoice;
pub mod multi_token_invoice_impl;
pub mod pre_approved_spending;
pub mod pre_approved_spending_impl;

/// Maximum length (bytes) for the optional payment reference string.
const MAX_REFERENCE_LEN: u32 = 64;
/// Maximum number of entries in the optional metadata map.
const MAX_METADATA_KEYS: u32 = 5;
/// Maximum length (bytes) for each metadata key or value.
const MAX_METADATA_KEY_LEN: u32 = 32;
/// Maximum length (bytes) for merchant notification key.
const MAX_NOTIFICATION_KEY_LEN: u32 = 128;

// ---------------------------------------------------------------------------
// Reflector-compatible oracle interface.
// lastprice(base, quote) returns Option<PriceData> where price is scaled by
// 10^decimals(). We call it via a generated client.
// ---------------------------------------------------------------------------
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    /// Price scaled by 10^7 (Reflector standard precision)
    pub price: i128,
    /// Ledger timestamp of the price update
    pub timestamp: u64,
}

/// Minimal oracle client — only the method we need.
mod oracle {
    use crate::PriceData;
    use soroban_sdk::{contractclient, Address, Env};

    #[allow(dead_code)]
    #[contractclient(name = "OracleClient")]
    pub trait OracleInterface {
        fn lastprice(env: Env, base: Address, quote: Address) -> Option<PriceData>;
    }
}

mod refund_reserve {
    use soroban_sdk::{contractclient, Address, Env};

    #[allow(dead_code)]
    #[contractclient(name = "RefundReserveClient")]
    pub trait RefundReserveInterface {
        fn check_merchant_reserve(env: Env, merchant: Address) -> bool;
    }
}

// --- Storage TTL Constants ---
// Instance storage: counters and config (shared TTL with contract instance)
const INSTANCE_LIFETIME_THRESHOLD: u32 = 100_000;
const INSTANCE_BUMP_AMOUNT: u32 = 120_000;

// Persistent storage: per-record data (Payment, Dispute, CustomerPayments)
// Individual TTL — survives beyond instance TTL, extended on each access.
const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

// Temporary storage: in-progress dispute state
// Short-lived; expires automatically if not extended.
const TEMP_LIFETIME_THRESHOLD: u32 = 10_000;
const TEMP_BUMP_AMOUNT: u32 = 15_000;

const DEFAULT_MAX_BATCH_SIZE: u32 = 20;
/// Maximum number of tags per payment (#122)
const MAX_TAGS: u32 = 3;
/// Maximum number of line items in invoice (#128)
const MAX_INVOICE_LINE_ITEMS: u32 = 20;
const MAX_SETTLEMENT_BATCH_SIZE: u32 = 50;
const SETTLEMENT_FEE_BPS: i128 = 0;
const DEFAULT_DISPUTE_TIMEOUT: u64 = 7 * 24 * 60 * 60; // 7 days in seconds
/// Default rate limit: effectively disabled until admin configures stricter values.
const DEFAULT_RATE_LIMIT_MAX_PAYMENTS: u32 = u32::MAX;
const DEFAULT_RATE_LIMIT_WINDOW_SIZE_LEDGERS: u32 = 1;
/// Default slippage tolerance: 50 bps (0.5%)
const DEFAULT_SLIPPAGE_BPS: u32 = 50;
/// Default slippage bounds
const DEFAULT_MIN_SLIPPAGE_BPS: u32 = 0;
const DEFAULT_MAX_SLIPPAGE_BPS: u32 = 10_000;
/// Reflector oracle price precision: prices are scaled by 10^7
const ORACLE_PRICE_PRECISION: i128 = 10_000_000;
/// Ledger sequences per weekly bucket (~7 days at 5s/ledger = 120_960 ledgers)
const LEDGER_BUCKET_SIZE: u32 = 120_960;
/// Maximum protocol fee: 500 bps = 5%
const MAX_FEE_BPS: u32 = 500;
/// Default protocol fee: 0 bps (no fee initially)
const DEFAULT_FEE_BPS: u32 = 0;
/// Idempotency key TTL: 24 hours in ledgers (~17,280 ledgers at 5s/ledger)
const IDEMPOTENCY_KEY_LIFETIME_THRESHOLD: u32 = 10_000;
const IDEMPOTENCY_KEY_BUMP_AMOUNT: u32 = 17_280;
/// Minimum collateral a merchant must maintain at all times (#129)
const DEFAULT_MIN_COLLATERAL: i128 = 1_000_000; // 1 USDC (7 decimals)
/// Default maximum tip: 3000 bps = 30% of base payment amount (#265)
const DEFAULT_MAX_TIP_BPS: u32 = 3_000;
/// Maximum number of beneficiaries in a tip split configuration (#370)
const MAX_TIP_SPLIT_BENEFICIARIES: u32 = 10;
/// Maximum number of historical notification keys to retain (#377)
const MAX_NOTIFICATION_KEY_HISTORY: u32 = 5;
/// Default notification key overlap window in seconds: 30 days (#377)
const DEFAULT_KEY_OVERLAP_WINDOW_SECONDS: u64 = 30 * 24 * 3600;
/// Default evidence submission window: 7 days in ledgers (~120,960 ledgers at 5s/ledger) (#308)
const DEFAULT_EVIDENCE_WINDOW_LEDGERS: u32 = 120_960;
/// Maximum evidence submissions per party (#308)
const MAX_EVIDENCE_SUBMISSIONS: u32 = 5;
/// Default cooling-off period: 0 ledgers (disabled by default) (#309)
const DEFAULT_MAX_COOLING_OFF_LEDGERS: u32 = 0;
/// Default maximum additional ledgers per extension (≈30 days at 5s/ledger)
const DEFAULT_MAX_EXTENSION_LEDGERS: u32 = 30 * 24 * 60 * 60 / 5;
/// Default maximum number of extensions per payment
const DEFAULT_MAX_EXTENSIONS: u32 = 3;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    RateLimitExceeded = 1,
    SubscriptionPaused = 2,
    OracleConditionNotMet = 3,
    /// Subscription's trial period has not elapsed; charging is deferred (#133)
    SubscriptionInTrial = 4,
    MerchantVolumeCapped = 20,
    TokenNotAllowed = 5,
    DuplicateExternalId = 6,
    MultisigNotRequired = 7,
    AlreadyApproved = 8,
    NotASigner = 9,
    VoucherExpired = 10,
    VoucherExhausted = 11,
    VoucherRevoked = 12,
    VoucherNotFound = 13,
    WithdrawalRateLimitExceeded = 14,
    /// Referred merchant already has a merchant record (#242)
    ReferralAlreadyExists = 15,
    /// No pending commission to claim (#242)
    NoCommissionToClaim = 16,
    /// Slippage tolerance exceeded on dynamic payment settlement (#246)
    SlippageExceeded = 21,
    /// Oracle address is not on the admin whitelist (#246)
    OracleNotWhitelisted = 22,
    /// Dynamic payment has expired (#246)
    DynamicPaymentExpired = 17,
    /// Customer cumulative spend would exceed the merchant-configured cap (#235)
    CustomerSpendLimitExceeded = 23,
    /// Capture attempted after the authorized capture deadline ledger.
    CapturePastDeadline = 24,
    /// Tip supplied on a payment that does not have tipping_enabled (#265)
    TippingNotEnabled = 18,
    /// Tip amount exceeds the admin-configured maximum tip bps of the base amount (#265)
    TipExceedsMaxBps = 19,
    /// Evidence submission window has closed (#308)
    EvidenceWindowClosed = 25,
    /// Evidence submission limit reached for this party (#308)
    EvidenceLimitReached = 26,
    /// Cooling-off period has expired (#309)
    CoolingOffExpired = 27,
    /// Payment not in cooling-off status (#309)
    NotInCoolingOff = 28,
    /// Cooling-off period exceeds maximum allowed (#309)
    CoolingOffExceedsMax = 29,
    /// KYB verification required but merchant not verified (#310)
    KYBVerificationRequired = 33,
    /// retry_failed_debit called before back-off interval has elapsed (#329)
    RetryNotDue = 34,
    /// Failed debit record not found (#329)
    DebitRecordNotFound = 35,
    /// Debit record is already abandoned; no further retries (#329)
    DebitAlreadyAbandoned = 36,
    /// Debit record already succeeded; no retry needed (#329)
    DebitAlreadySucceeded = 37,
    /// Payment is not in a pending state and cannot be extended
    InvalidPaymentStatus = 38,
    /// Maximum number of extensions reached for this payment
    MaxExtensionsReached = 39,
    /// Additional ledgers exceed the maximum allowed per extension
    MaxExtensionLedgersExceeded = 40,
    /// Subscription pause count exceeded (#327)
    PauseCountExceeded = 30,
    /// Unauthorized to pause subscription (#327)
    UnauthorizedPause = 31,
    /// Merchant refund reserve is below the configured minimum (#334)
    InsufficientMerchantReserve = 32,
    /// Customer is blocked by merchant
    CustomerBlocked = 50,
    /// DAO mediation has not been configured by admin.
    DaoNotConfigured = 51,
    /// Caller is not a registered DAO mediator member.
    NotADaoMember = 52,
    /// Payment dispute has already been escalated to the DAO.
    DaoAlreadyEscalated = 53,
    /// DAO vote window is still open; verdict cannot be executed yet.
    DaoVoteWindowOpen = 54,
    /// DAO vote window has closed; no further votes accepted.
    DaoVoteWindowClosed = 55,
    /// This DAO member has already cast a vote on this case.
    DaoAlreadyVoted = 56,
    /// DAO verdict has already been executed for this case.
    DaoCaseAlreadyExecuted = 57,
    /// Minimum number of DAO votes has not been reached.
    DaoMinVotesNotMet = 58,
    /// Payment is not in Disputed status; cannot escalate.
    PaymentNotDisputed = 59,
}

/// Overflow Error2 — split from Error because #[contracterror] is bounded to 50 variants.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error2 {
    /// No active DAO mediation case exists for the payment.
    DaoCaseNotFoundForPayment = 60,
    /// DAO escalation cannot be cancelled because voting has started.
    DaoEscalationHasVotes = 61,
}

/// Extension errors that overflow the 50-variant contracterror limit.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtError {
    /// Batch size exceeds the allowed cap or amount is out of range.
    InvalidAmount = 71,
    /// Merchant has KYB on record, but it is now expired.
    MerchantKYBExpired = 72,
}

/// Per-merchant withdrawal rate limit config (#231).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalLimit {
    pub window_seconds: u64,
    pub cap: i128,
}

/// Tracks cumulative withdrawals within the current window (#231).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalWindowState {
    pub window_start: u64,
    pub withdrawn: i128,
}

/// Referral record: tracks referrer, registration ledger, and accrual window (#242).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReferralRecord {
    pub referrer: Address,
    pub registered_at_ledger: u32,
    pub window_ledgers: u32,
}

/// Per-customer (or default) spend cap config (#235).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpendLimit {
    pub amount: i128,
    pub window_seconds: u64,
}

/// #358: Trust tier for a buyer as assigned by the merchant.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BuyerTrustTierLevel {
    New = 0,
    Standard = 1,
    Trusted = 2,
    VIP = 3,
}

/// #370: Tip split beneficiary allocation
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TipSplitBeneficiary {
    pub recipient: Address,
    pub bps: u32,
}

/// #377: Notification key entry with validity window
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NotificationKeyEntry {
    pub key: soroban_sdk::Bytes,
    pub valid_from: u64,
    pub valid_until: u64,
}

/// Tracks cumulative spend within the current rolling window (#235).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerSpendWindow {
    pub window_start: u64,
    pub spent: i128,
}

/// Direction for oracle price condition (#125)
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OracleDirection {
    Gte = 0,
    Lte = 1,
}

/// Merchant status for fine-grained access control.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MerchantStatus {
    Active = 0,
    Suspended = 1,
    Banned = 2,
}

/// Status of a merchant appeal in the reinstatement workflow.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppealStatus {
    /// Appeal is pending admin review
    Pending = 0,
    /// Appeal approved, merchant in cooling-off period
    ApprovedCoolingOff = 1,
    /// Appeal approved and cooling-off completed, merchant reinstated
    ApprovedReinstated = 2,
    /// Appeal rejected
    Rejected = 3,
}

/// On-chain appeal record for a banned merchant with time-locked reinstatement.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantAppeal {
    pub merchant: Address,
    pub reason_hash: BytesN<32>,
    pub submitted_at: u64,
    pub status: AppealStatus,
    /// Ledger timestamp when cooling-off period ends (0 if not in cooling-off)
    pub cooling_off_until: u64,
}

/// KYB (Know Your Business) verification record (#310)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantKYB {
    pub kyb_hash: BytesN<32>,
    pub expiry_ledger: u64,
    pub jurisdiction: String,
    pub revoked: bool,
}

/// KYB status response (#310)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KYBStatus {
    pub verified: bool,
    pub expiry_ledger: u64,
    pub jurisdiction: String,
}

/// Conditional release based on oracle price (#125)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OracleCondition {
    pub asset: Address,
    pub threshold: i128,
    pub direction: OracleDirection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum PaymentStatus {
    Pending = 0,
    Completed = 1,
    Refunded = 2,
    Disputed = 3,
    Expired = 4,
    Authorized = 5,
    ScheduledPending = 6,
    /// Awaiting M-of-N multi-sig approval before proceeding.
    PendingApproval = 7,
    /// Payment completed; in cooling-off window before final settlement (#309)
    CoolingOff = 8,
    /// Payment cancelled during cooling-off period (#309)
    CancelledInCoolingOff = 9,
    /// Dispute has exceeded the escalation timeout and is awaiting priority resolution (#417)
    EscalatedDispute = 10,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitRecipient {
    pub recipient: Address,
    pub bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitTransfer {
    pub recipient: Address,
    pub bps: u32,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeeTier {
    pub min_volume: i128,
    pub fee_bps: u32,
}

/// Invoice line item for payment (#128)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineItem {
    pub description: Symbol,
    pub quantity: u32,
    pub unit_price: i128,
}

/// Invoice data attached to payment (#128)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceData {
    pub line_items: Vec<LineItem>,
    pub tax_bps: u32,
    pub currency_label: Symbol,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub status: PaymentStatus,
    pub created_at: u64,
    /// Ledger timestamp after which the payment can be expired. 0 = no expiry.
    pub expires_at: u64,
    /// Cumulative amount refunded via partial refunds.
    pub refunded_amount: i128,
    /// Optional merchant reference string (max 64 bytes) for off-chain reconciliation.
    pub reference: Option<String>,
    /// Optional key-value metadata (max 5 keys, each max 32 bytes).
    pub metadata: Option<Map<String, String>>,
    /// Optional recipient split definitions (must sum to 10,000 bps).
    pub split_recipients: Option<Vec<SplitRecipient>>,
    /// Optional execution timestamp for scheduled payments. 0 = immediate.
    pub execute_after: u64,
    /// Optional payment category for on-chain segmentation (#122)
    pub category: Option<Symbol>,
    /// Optional tags (max 3) immutable after creation (#122)
    pub tags: Option<Vec<Symbol>>,
    /// Ledger sequence after which an authorized payment can no longer be captured. 0 = not authorized.
    pub capture_deadline: u64,
    // Optional oracle price condition required for completion (#125)
    // pub release_condition: Option<OracleCondition>,
    /// Optional off-chain order correlation key (hash of merchant order ID).
    pub external_id: Option<BytesN<32>>,
    /// Whether this payment accepts a customer tip at settlement time (#265).
    pub tipping_enabled: bool,
    /// Number of times this payment's expiry has been extended.
    pub extension_count: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockEntry {
    pub customer: Address,
    pub reason_code: Symbol,
    pub evidence_hash: BytesN<32>,
    pub blocked_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnblockRequest {
    pub request_id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub dispute_hash: BytesN<32>,
    pub requested_at_ledger: u32,
    pub expires_at_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentRequest {
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub reference: Option<String>,
    pub metadata: Option<Map<String, String>>,
}

/// Consent record for zero-amount agreement signing (#307)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsentRecord {
    pub id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub terms_hash: BytesN<32>,
    pub terms_version: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub is_signed: bool,
    pub signed_at: u64,
    pub is_revoked: bool,
}

/// Global protocol-wide aggregate statistics (#70).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalStats {
    pub total_payments_created: u32,
    pub total_payments_completed: u32,
    pub total_payments_refunded: u32,
    pub total_payments_expired: u32,
    pub total_volume_completed: Map<Address, i128>,
    pub total_volume_refunded: Map<Address, i128>,
}

/// Per-merchant aggregate statistics (#70).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantStats {
    pub payments_created: u32,
    pub payments_completed: u32,
    pub payments_refunded: u32,
    pub volume_completed: Map<Address, i128>,
}

/// Per-merchant revenue dashboard summary (#226).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantSummary {
    pub total_volume: i128,
    pub completed_count: u32,
    pub failed_count: u32,
    pub pending_count: u32,
    pub volume_by_token: Map<Address, i128>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Dispute {
    pub payment_id: u32,
    pub reason: String,
    pub created_at: u64,
    pub resolved: bool,
}

/// Evidence submission record for a dispute (#308)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DisputeEvidenceRecord {
    pub payment_id: u32,
    pub customer_evidence: Vec<(BytesN<32>, u64)>, // (hash, submission_ledger)
    pub merchant_evidence: Vec<(BytesN<32>, u64)>, // (hash, submission_ledger)
    pub evidence_window_close_ledger: u32,
    pub resolution_note_hash: Option<BytesN<32>>,
}

/// Cooling-off configuration for a payment (#309)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoolingOffConfig {
    pub cooling_off_ledgers: u32,
    pub cooling_off_expiry_ledger: u32,
}

/// Default payment timeout: 7 days in seconds.
const DEFAULT_PAYMENT_TIMEOUT: u64 = 7 * 24 * 60 * 60;

/// Default lower bound for merchant-defined payment expiry overrides (#130): 1 minute.
const DEFAULT_MIN_PAYMENT_EXPIRY: u64 = 60;
/// Default upper bound for merchant-defined payment expiry overrides (#130): 30 days.
const DEFAULT_MAX_PAYMENT_EXPIRY: u64 = 30 * 24 * 60 * 60;

/// Maximum allowed `page_size` for `get_customer_payments_page` (#132).
pub const MAX_CUSTOMER_PAYMENTS_PAGE_SIZE: u32 = 50;

/// Max pauses per subscription before denial (#327)
const MAX_SUBSCRIPTION_PAUSES: u32 = 3;
/// Approximate ledger close time used for subscription proration calculations.
const LEDGER_CLOSE_TIME_SECONDS: u64 = 5;

/// Paginated view over a customer's payment IDs (#132).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerPaymentsPage {
    pub payments: Vec<u32>,
    pub total_count: u32,
    pub has_more: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Subscription {
    pub id: u32,
    pub subscriber: Address,
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub interval_seconds: u64,
    pub last_charged_at: u64,
    pub max_charges: u32,
    pub charges_count: u32,
    pub active: bool,
    /// True when subscriber has temporarily paused the subscription (#124)
    pub paused: bool,
    /// Ledger timestamp when the subscription was paused (#124)
    pub paused_at: u64,
    /// Ledger timestamp at which the trial ends and the first charge becomes
    /// available. 0 = no trial (immediate first charge). (#133)
    pub trial_ends_at: u64,
    /// Who can authorize pause (#327)
    pub pause_authority: PauseAuthority,
    /// Count of times this subscription has been paused (#327)
    pub pause_count: u32,
    /// Optional current plan reference for plan-switch lifecycle.
    pub plan_id: Option<u32>,
    /// Approximate next due ledger used for plan-change proration.
    pub next_due_ledger: u64,
    /// One-time credit to be applied to the next charge after plan change.
    pub pending_prorated_credit: i128,
}

/// Merchant-defined subscription billing plan.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubscriptionPlan {
    pub plan_id: u32,
    pub merchant: Address,
    pub token: Address,
    pub amount: i128,
    pub interval_days: u64,
    pub active: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RateLimitConfig {
    pub max_payments: u32,
    pub window_size_ledgers: u32,
}

/// Slippage tolerance configuration for multi-token payments (#135)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlippageConfig {
    pub default_bps: u32,
    pub min_bps: u32,
    pub max_bps: u32,
}

/// Oracle-backed dynamic payment record (#246).
/// The settlement amount is computed at complete_payment time using the oracle rate.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DynamicPayment {
    pub payment_id: u32,
    pub fiat_amount: i128,
    pub fiat_currency: Symbol,
    pub oracle_address: Address,
    pub token: Address,
    pub slippage_bps: u32,
    /// Oracle price at creation time (scaled by 10^7), used for slippage check at settlement
    pub creation_rate: i128,
    pub expiry: u64,
}

/// Installment payment plan record.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentPlan {
    /// Unique identifier for the plan.
    pub plan_id: u32,
    /// Customer address.
    pub customer: Address,
    /// Merchant address.
    pub merchant: Address,
    /// Token address for the payment.
    pub token: Address,
    /// Total amount to be paid in installments.
    pub total_amount: i128,
    /// Number of installments.
    pub num_installments: u32,
    /// Amount per installment (assuming equal installments).
    pub installment_amount: i128,
    /// Interval in ledgers between installments.
    pub interval_ledgers: u32,
    /// Ledger at which the plan expires if not completed.
    pub expiry_ledger: u64,
    /// Ledger at which the next installment is due.
    pub next_due_ledger: u64,
    /// Number of installments already paid.
    pub installments_paid: u32,
    /// Status of the plan.
    pub status: PlanStatus,
}

/// Status of an installment plan.
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlanStatus {
    Active = 0,
    Paused = 1,
    Completed = 2,
    Expired = 3,
}

/// #351: On-chain recurring payment schedule.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringPaymentSchedule {
    pub schedule_id: u32,
    pub payer: Address,
    pub payee: Address,
    pub token: Address,
    pub amount: i128,
    /// Minimum seconds between executions.
    pub interval_seconds: u64,
    /// Maximum number of executions (0 = unlimited).
    pub max_cycles: u32,
    /// Number of executions completed so far.
    pub cycles_executed: u32,
    /// Ledger timestamp at or after which the next execution is permitted.
    pub next_due: u64,
    pub active: bool,
}

/// Per-merchant volume cap configuration (#131)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VolumeCap {
    pub cap_amount: i128,
    pub window_seconds: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerRateLimit {
    pub count: u32,
    pub window_start_ledger: u32,
}

/// Voucher discount kind: percentage of payment or fixed token-unit amount.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscountType {
    Percentage,
    Fixed,
}

/// Who is authorized to pause a subscription (#327)
#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseAuthority {
    Subscriber = 0,
    Merchant = 1,
    Both = 2,
}

/// Per-merchant multi-sig approval policy.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultisigPolicy {
    pub threshold: i128,
    pub signers: Vec<Address>,
    pub m: u32,
    pub approval_window_seconds: u64,
}

/// Per-payment approval tracking state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApprovalState {
    pub payment_id: u32,
    pub approvals: Vec<Address>,
    pub created_at: u64,
}

/// Discount voucher issued by a merchant.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Voucher {
    pub merchant: Address,
    pub discount_type: DiscountType,
    pub discount_value: u32,
    pub max_uses: u32,
    pub uses_remaining: u32,
    pub expiry: u64,
    pub revoked: bool,
}

/// Storage key classification:
/// - Instance:    Admin, PaymentCounter, MaxBatchSize, DisputeTimeout,
///                OracleAddress, UsdcToken, FeeBps, FeeRecipient
///                (config/counters — bounded, shared TTL with contract)
/// - Persistent:  Payment(u32), CustomerPayments(Address), Settled(u32)
///                (per-record data — unbounded, individual TTL)
/// - Temporary:   Dispute(u32), IdempotencyKey(BytesN<32>)
///                (in-progress dispute state, idempotency keys — short-lived, auto-expires)
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    // --- Instance ---
    Admin,
    PaymentCounter,
    MaxBatchSize,
    DisputeTimeout,
    /// Reflector oracle contract address for token/USDC price feeds
    OracleAddress,
    /// USDC token contract address — canonical settlement currency
    UsdcToken,
    /// Maximum age (seconds) an oracle price may be before rejection
    MaxOracleAge,
    /// Proposed new admin address (pending acceptance)
    ProposedAdmin,
    /// Global emergency stop flag
    Paused,
    /// Human-readable pause reason
    PauseReason,
    /// Global payment timeout in seconds (default: 7 days)
    PaymentTimeout,
    /// Lower bound (inclusive) for merchant-defined per-payment expiry overrides (#130)
    MinPaymentExpiry,
    /// Upper bound (inclusive) for merchant-defined per-payment expiry overrides (#130)
    MaxPaymentExpiry,
    /// When true, merchant allowlist is bypassed (open mode)
    MerchantOpenMode,
    /// Subscription counter
    SubscriptionCounter,
    /// Global per-customer payment creation rate limit config
    RateLimitConfig,
    /// Current contract schema/runtime version
    ContractVersion,
    /// Migration completion flag for a specific version
    MigrationCompleted(u32),
    /// Protocol fee in basis points (1 bps = 0.01%)
    FeeBps,
    /// Address that receives protocol fees
    FeeRecipient,
    /// Volume-based fee tiers sorted by min_volume ascending.
    FeeTiers,
    /// Token whitelist contract address
    TokenWhitelistContract,
    // --- Persistent ---
    Payment(u32),
    CustomerPayments(Address),
    /// Settlement marker to prevent double merchant settlement
    Settled(u32),
    /// Per-customer rate limit usage state
    CustomerRateLimit(Address),
    /// Merchant approval status (true = approved)
    MerchantApproved(Address),
    /// Subscription record
    Subscription(u32),
    /// Index: (merchant, reference_hash) → Vec<u32> of payment IDs
    MerchantReference(Address, u32),
    /// Persistent: sha256 receipt hash for a completed payment (#65)
    /// Hash inputs (big-endian): payment_id(u32) || customer(Address) || merchant(Address)
    ///                           || amount(i128) || token(Address) || completed_at(u64)
    PaymentReceipt(u32),
    /// Persistent: global aggregate statistics (#70)
    GlobalStats,
    /// Persistent: per-merchant aggregate statistics (#70)
    MerchantStats(Address),
    /// Persistent: weekly volume bucket — (token, bucket_id) → total completed volume (#70)
    VolumeBucket(Address, u32),
    /// Persistent: per-merchant weekly volume bucket for rolling tier evaluation.
    MerchantVolumeBucket(Address, u32),
    /// Persistent: cached last tier bps emitted for merchant.
    MerchantCurrentTierBps(Address),
    /// Persistent: (merchant, category) → Vec<payment_id> for category analytics (#122)
    CategoryPayments(Address, Symbol),
    /// Persistent: merchant withdrawal queue — Vec<(payment_id, amount)> (#126)
    WithdrawalQueue(Address),
    /// Persistent: invoice hash (SHA256) for payment (#128)
    InvoiceHash(u32),
    /// Persistent: merchant collateral balance (#129)
    MerchantCollateral(Address),
    /// Instance: minimum collateral required for merchant approval (#129)
    MinCollateral,
    /// Instance: global slippage tolerance config for multi-token payments (#135)
    SlippageConfig,
    /// Persistent: per-merchant volume cap config (#131)
    VolumeCap(Address),
    /// Persistent: per-merchant volume within a time window bucket (#131)
    /// Key: (merchant, window_bucket) where window_bucket = timestamp / window_seconds
    MerchantWindowVolume(Address, u64),
    /// Persistent: merchant notification key for event routing
    MerchantNotificationKey(Address),
    /// Instance: merchant's preferred token for receiving payments
    PreferredToken(Address),
    /// Instance: DEX router contract address for token swaps
    SwapRouter,
    // --- Temporary ---
    Dispute(u32),
    /// Temporary: idempotency key → payment_id mapping (expires after 24h)
    IdempotencyKey(BytesN<32>),
    // --- Task 1: External ID Index ---
    /// Persistent: (merchant, external_id) → payment_id
    ExternalIdIndex(Address, BytesN<32>),
    // --- Task 2: Multi-Sig ---
    /// Instance: per-merchant multi-sig policy
    MultisigPolicy(Address),
}

/// Overflow storage keys — split from DataKey because #[contracttype] is bounded to 50 variants.
#[contracttype]
#[derive(Clone)]
pub enum DataKey2 {
    /// Persistent: per-payment approval state
    ApprovalState(u32),
    // --- Task 3: Vouchers ---
    /// Persistent: (merchant, code_hash) → Voucher
    Voucher(Address, BytesN<32>),
    /// Persistent: merchant status (Active/Suspended/Banned)
    MerchantStatus(Address),
    /// Persistent: merchant suspension expiry timestamp
    MerchantSuspensionExpiry(Address),
    /// Persistent: active appeal record per merchant
    MerchantAppeal(Address),
    /// Persistent: timestamp after which a rejected merchant may re-appeal
    AppealCooldownUntil(Address),
    /// Instance: appeal rejection cooldown in seconds
    AppealRejectionCooldownSeconds,
    /// Instance: global default withdrawal window in seconds (#231)
    WithdrawalWindowSeconds,
    /// Instance: global default withdrawal cap per window (#231)
    WithdrawalWindowCap,
    /// Instance: max block age in ledgers (0 = never auto-expire)
    MaxBlockAgeLedgers,
    /// Persistent: per-merchant withdrawal limit override (#231)
    MerchantWithdrawalLimit(Address),
    /// Persistent: per-merchant withdrawal tracker for current window (#231)
    MerchantWithdrawalWindow(Address),
    /// Persistent: per-(merchant,customer) block entry
    CustomerBlockEntry(Address, Address),
    /// Persistent: per-merchant revenue dashboard summary (#226)
    MerchantSummary(Address),
    // --- #239: Loyalty Points ---
    /// Instance: points earned per 1_000_000 units of payment token
    LoyaltyPointsPerUnit,
    /// Instance: discount in basis points per 1 point redeemed
    LoyaltyRedemptionRateBps,
    /// Instance: minimum payment floor after discount (in token units)
    LoyaltyMinPaymentFloor,
    /// Instance: ledgers after which unspent points expire (0 = no expiry)
    LoyaltyExpiryLedgers,
    /// Persistent: customer loyalty points balance
    LoyaltyBalance(Address),
    /// Persistent: ledger at which customer's points were last accrued (for expiry)
    LoyaltyLastAccrualLedger(Address),
    /// Instance: global referral commission in basis points (#242)
    ReferralCommissionBps,
    /// Instance: global referral window in ledgers (#242)
    ReferralWindowLedgers,
    /// Persistent: referral record for a referred merchant (#242)
    ReferralRecord(Address),
    /// Persistent: pending commission balance for a referrer (#242)
    PendingCommission(Address),
    /// Persistent: dynamic payment record (#246)
    DynamicPayment(u32),
    /// Instance: admin-maintained oracle whitelist (#246)
    OracleWhitelist,
    /// Persistent: per-(merchant,customer) spend limit override (#235)
    CustomerSpendLimit(Address, Address),
    /// Persistent: merchant-level default spend limit (#235)
    DefaultSpendLimit(Address),
    /// Persistent: per-(merchant,customer) rolling spend window state (#235)
    CustomerSpendWindowState(Address, Address),
    /// #216: recurring invoice counter
    RecurringInvoiceCounter,
    /// #216: recurring invoice record
    RecurringInvoice(u32),
    /// #265: maximum tip as basis points of the base payment amount (instance storage)
    MaxTipBps,
    /// #307: consent record counter
    ConsentRecordCounter,
    /// #307: consent record storage
    ConsentRecord(u32),
    /// #307: index (merchant, customer, terms_version) → consent_id
    ConsentIndex(Address, Address, String),
    /// Instance: evidence submission window in ledgers (#308)
    EvidenceWindowLedgers,
    /// Persistent: unblock request storage
    UnblockRequest(u32),
    /// Instance: maximum evidence submissions per party (#308)
    MaxEvidenceSubmissions,
    /// Persistent: dispute evidence record for a payment (#308)
    DisputeEvidence(u32),
    /// Instance: maximum cooling-off period in ledgers (#309)
    MaxCoolingOffLedgers,
    /// Persistent: cooling-off configuration for a payment (#309)
    CoolingOffConfig(u32),
    /// Persistent: merchant KYB verification record (#310)
    MerchantKYB(Address),
    /// Instance: KYB enforcement flag (#310)
    KYBEnforcementEnabled,
    /// #329: counter for failed debit records
    FailedDebitCounter,
    /// #329: failed debit record keyed by record ID
    FailedDebit(u32),
    /// #329: admin-configurable retry back-off config
    RetryConfig,
    /// Instance: maximum additional ledgers per extension
    MaxExtensionLedgers,
    /// Instance: maximum number of extensions per payment
    MaxExtensions,
    /// Instance: refund contract used for merchant reserve enforcement (#334)
    RefundContract,
    /// Persistent: installment payment plan
    PaymentPlan(u32),
}

/// Overflow DataKey3 — split from DataKey2 (50-variant limit).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey3 {
    /// Persistent: lifetime total commission earned by a referrer (#570)
    TotalEarnedCommission(Address),
    /// Persistent: lifetime total commission claimed by a referrer (#570)
    TotalClaimedCommission(Address),
    /// #351: counter for recurring payment schedules
    RecurringCounter,
    /// #351: recurring payment schedule record
    RecurringSchedule(u32),
    /// #367: 30-day rolling settled volume record per merchant
    MerchantSettlementVolume(Address),
    /// #358: buyer trust tier assigned by merchant (merchant, buyer) → BuyerTrustTierLevel
    BuyerTrustTier(Address, Address),
    /// #358: per-tier spending limit (merchant, tier_value as u32) → SpendLimit
    TierSpendingLimit(Address, u32),
    /// #370: tip split configuration (merchant) → Vec<TipSplitBeneficiary>
    TipSplitConfig(Address),
    /// #377: notification key history (merchant) → Vec<NotificationKeyEntry>
    NotificationKeyHistory(Address),
    /// #377: notification key rotation config
    NotificationKeyRotationConfig,
    /// Slippage tolerance for multi-token dynamic payments (merchant) -> u32
    SlippageToleranceBps(Address),
    /// Instance: counter for DAO mediation cases
    DaoMediationCounter,
    /// Persistent: DAO mediation case record keyed by case ID
    DaoMediationCase(u32),
    /// Persistent: payment_id -> DAO mediation case ID
    DaoCaseByPayment(u32),
    /// Persistent: (case_id, voter) → bool vote cast by DAO member
    DaoMediationVote(u32, Address),
    /// Instance: list of DAO mediator addresses
    DaoMembers,
    /// Instance: seconds the vote window stays open after escalation
    DaoVoteWindowSeconds,
    /// Instance: minimum votes required for a valid verdict
    DaoMinVotes,
    /// Persistent: merchant-defined subscription billing plan
    SubscriptionPlan(u32),
    /// Instance: when true, merchant KYB checks are enforced on payment creation.
    KYBRequired,
}

mod events;

// ── Dispute Mediation DAO ─────────────────────────────────────────────────────

/// Default DAO vote window: 3 days in seconds.
const DEFAULT_DAO_VOTE_WINDOW_SECONDS: u64 = 3 * 24 * 60 * 60;
/// Default minimum DAO votes required for verdict execution.
const DEFAULT_DAO_MIN_VOTES: u32 = 1;

/// A DAO-mediated dispute case for an escalated payment.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaoMediationCase {
    pub case_id: u32,
    pub payment_id: u32,
    pub initiated_by: Address,
    pub created_at: u64,
    /// Timestamp after which the vote window closes and verdict can be executed.
    pub vote_window_closes_at: u64,
    pub votes_for_merchant: u32,
    pub votes_for_customer: u32,
    pub executed: bool,
}

// ── #367: Dynamic Settlement Fee Tiers ───────────────────────────────────────

/// Rolling settled-volume record for the 30-day fee-tier window (#367).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantSettlementRecord {
    /// Cumulative settled amount within the current 30-day window.
    pub volume_30d: i128,
    /// Ledger timestamp when the current window started.
    pub window_start_timestamp: u64,
}

/// 30-day window duration in seconds (30 × 24 × 3600).
const SETTLEMENT_VOLUME_WINDOW_SECONDS: u64 = 30 * 24 * 60 * 60;

// ── #329: Failed Auto-Debit Retry Queue ───────────────────────────────────────

/// Status of a failed debit record.
#[contracttype]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailedDebitStatus {
    Pending = 0,
    Succeeded = 1,
    Abandoned = 2,
}

/// Stored record of a failed merchant-pull debit attempt.
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

/// Admin-configurable exponential back-off settings for the retry queue.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetryConfig {
    pub base_retry_interval: u64,
    pub max_retry_interval: u64,
    pub max_retry_attempts: u32,
}

const DEFAULT_BASE_RETRY_INTERVAL: u64 = 100;
const DEFAULT_MAX_RETRY_INTERVAL: u64 = 3_200;
const DEFAULT_MAX_RETRY_ATTEMPTS: u32 = 5;

/// #216: Recurring invoice schedule.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecurringInvoice {
    pub id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub amount: i128,
    pub token: Address,
    pub interval_seconds: u64,
    pub interval_ledgers: u32,
    pub max_cycles: u32,
    pub cycles_triggered: u32,
    pub next_due_at: u64,
    pub next_due_ledger: u64,
    pub reference_hash: Option<BytesN<32>>,
    pub active: bool,
}

#[contract]
pub struct AhjoorPaymentsContract;

#[contractimpl]
impl AhjoorPaymentsContract {
    fn find_open_dao_case_id_by_payment(env: &Env, payment_id: u32) -> Option<u32> {
        let case_counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey3::DaoMediationCounter)
            .unwrap_or(0);

        for case_id in 0..case_counter {
            let maybe_case: Option<DaoMediationCase> = env
                .storage()
                .persistent()
                .get(&DataKey3::DaoMediationCase(case_id));
            if let Some(case) = maybe_case {
                if case.payment_id == payment_id && !case.executed {
                    return Some(case_id);
                }
            }
        }
        None
    }

    /// One-time contract initialization.
    /// Admin, counters, and config go to instance storage.
    /// fee_recipient: Address that receives protocol fees
    /// fee_bps: Protocol fee in basis points (max 500 = 5%)
    pub fn initialize(env: Env, admin: Address, fee_recipient: Address, fee_bps: u32) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }

        if fee_bps > MAX_FEE_BPS {
            panic!("Fee cannot exceed 500 bps (5%)");
        }

        // Instance: config and counters
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PaymentCounter, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::MaxBatchSize, &DEFAULT_MAX_BATCH_SIZE);
        env.storage()
            .instance()
            .set(&DataKey::DisputeTimeout, &DEFAULT_DISPUTE_TIMEOUT);
        env.storage().instance().set(
            &DataKey::RateLimitConfig,
            &RateLimitConfig {
                max_payments: DEFAULT_RATE_LIMIT_MAX_PAYMENTS,
                window_size_ledgers: DEFAULT_RATE_LIMIT_WINDOW_SIZE_LEDGERS,
            },
        );
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &1u32);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::FeeBps, &fee_bps);
        env.storage()
            .instance()
            .set(&DataKey::FeeRecipient, &fee_recipient);
        env.storage()
            .instance()
            .set(&DataKey::FeeTiers, &Vec::<FeeTier>::new(&env));
        env.storage().instance().set(
            &DataKey2::MaxExtensionLedgers,
            &DEFAULT_MAX_EXTENSION_LEDGERS,
        );
        env.storage()
            .instance()
            .set(&DataKey2::MaxExtensions, &DEFAULT_MAX_EXTENSIONS);

        env.storage()
            .instance()
            .set(&DataKey2::WithdrawalWindowSeconds, &86400u64);
        env.storage()
            .instance()
            .set(&DataKey2::WithdrawalWindowCap, &i128::MAX);
        env.storage().instance().set(&DataKey3::KYBRequired, &false);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Create a single payment: transfer tokens from customer to contract (escrow).
    /// Payment record stored in persistent storage with individual TTL.
    /// Rejects unapproved merchants unless open mode is enabled (#58).
    /// Sets expiry based on global payment timeout (#54).
    /// Accepts optional reference (max 64 bytes) and metadata (max 5 keys) (#67).
    /// Accepts optional idempotency_key to prevent duplicate payments.
    /// Returns the new payment ID.
    pub fn create_payment(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        reference: Option<String>,
        metadata: Option<Map<String, String>>,
        idempotency_key: Option<BytesN<32>>,
    ) -> u32 {
        Self::create_payment_with_options(
            env,
            customer,
            merchant,
            amount,
            token,
            reference,
            metadata,
            None,
            None,
            idempotency_key,
        )
    }

    /// Create a payment with optional invoice data attached (#128).
    pub fn create_payment_with_invoice(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        reference: Option<String>,
        metadata: Option<Map<String, String>>,
        invoice: Option<InvoiceData>,
        idempotency_key: Option<BytesN<32>>,
    ) -> u32 {
        // Validate invoice against payment amount
        Self::validate_invoice_data(&env, &invoice, amount);

        // Create the payment using the base function
        let payment_id = Self::create_payment_with_options(
            env.clone(),
            customer,
            merchant,
            amount,
            token,
            reference,
            metadata,
            None,
            None,
            idempotency_key,
        );

        // If invoice provided, compute hash and store it
        if let Some(inv) = invoice {
            let invoice_hash = Self::compute_invoice_hash(&env, &inv);
            env.storage()
                .persistent()
                .set(&DataKey::InvoiceHash(payment_id), &invoice_hash);
            env.storage().persistent().extend_ttl(
                &DataKey::InvoiceHash(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_invoice_attached(&env, payment_id, invoice_hash);
        }

        payment_id
    }

    /// Extended payment creation with optional recipient splits and scheduling.
    #[allow(clippy::too_many_arguments)]
    pub fn create_payment_with_options(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        reference: Option<String>,
        metadata: Option<Map<String, String>>,
        split_recipients: Option<Vec<SplitRecipient>>,
        execute_after: Option<u64>,
        idempotency_key: Option<BytesN<32>>,
    ) -> u32 {
        Self::create_payment_with_expiry(
            env,
            customer,
            merchant,
            amount,
            token,
            reference,
            metadata,
            split_recipients,
            execute_after,
            idempotency_key,
            None,
        )
    }

    /// Extended payment creation with an optional merchant-defined expiry (#130).
    ///
    /// When `expiry_seconds` is `Some(value)`, `value` must lie within the
    /// admin-configured `[min_expiry, max_expiry]` bounds and replaces the
    /// global default for this payment only. When `None`, the existing global
    /// `PaymentTimeout` is used (preserving the previous behaviour).
    #[allow(clippy::too_many_arguments)]
    pub fn create_payment_with_expiry(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        reference: Option<String>,
        metadata: Option<Map<String, String>>,
        split_recipients: Option<Vec<SplitRecipient>>,
        execute_after: Option<u64>,
        idempotency_key: Option<BytesN<32>>,
        expiry_seconds: Option<u64>,
    ) -> u32 {
        Self::require_not_paused(&env);
        customer.require_auth();

        // Check idempotency key first
        if let Some(ref key) = idempotency_key {
            if let Some(existing_payment_id) = env
                .storage()
                .temporary()
                .get::<DataKey, u32>(&DataKey::IdempotencyKey(key.clone()))
            {
                // Key exists, return existing payment ID
                return existing_payment_id;
            }
        }

        Self::enforce_rate_limit(&env, &customer, 1);

        if amount <= 0 {
            panic!("Payment amount must be positive");
        }

        // Validate optional reference and metadata (#67)
        Self::validate_reference(&env, &reference);
        Self::validate_metadata(&env, &metadata);
        Self::validate_split_recipients(&split_recipients);

        // Token whitelist validation
        Self::require_token_allowed(&env, &token);

        // Merchant allowlist check (#58)
        Self::require_merchant_approved(&env, &merchant);
        Self::require_merchant_reserve(&env, &merchant);

        // Blocklist lazy-expiry check: if customer is blocked, reject unless entry expired
        if env
            .storage()
            .persistent()
            .has(&DataKey2::CustomerBlockEntry(merchant.clone(), customer.clone()))
        {
            let entry: BlockEntry = env
                .storage()
                .persistent()
                .get(&DataKey2::CustomerBlockEntry(merchant.clone(), customer.clone()))
                .expect("block entry");
            let now_ledger = env.ledger().sequence();
            let max_age: u32 = env
                .storage()
                .instance()
                .get(&DataKey2::MaxBlockAgeLedgers)
                .unwrap_or(0u32);
            if max_age > 0 && now_ledger > entry.blocked_at_ledger && now_ledger - entry.blocked_at_ledger > max_age {
                // expired — remove and continue
                env.storage()
                    .persistent()
                    .remove(&DataKey2::CustomerBlockEntry(merchant.clone(), customer.clone()));
            } else {
                panic_with_error!(&env, Error::CustomerBlocked);
            }
        }

        // KYB enforcement check (#310)
        if Self::is_kyb_enforcement_enabled(env.clone()) {
            let kyb_key = DataKey2::MerchantKYB(merchant.clone());
            let current_ledger = u64::from(env.ledger().sequence());
            let kyb = env
                .storage()
                .persistent()
                .get::<_, MerchantKYB>(&kyb_key);
            if kyb.is_none() {
                panic_with_error!(&env, Error::KYBVerificationRequired);
            }
            env.storage().persistent().extend_ttl(
                &kyb_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            let kyb = kyb.expect("kyb exists");
            if kyb.revoked {
                panic_with_error!(&env, Error::KYBVerificationRequired);
            }
            if current_ledger > kyb.expiry_ledger {
                panic_with_error!(&env, ExtError::MerchantKYBExpired);
            }
        }

        let client = token::Client::new(&env, &token);
        client.transfer(&customer, &env.current_contract_address(), &amount);

        let default_timeout: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentTimeout)
            .unwrap_or(DEFAULT_PAYMENT_TIMEOUT);
        let custom_expiry = match expiry_seconds {
            Some(value) => {
                Self::require_expiry_within_bounds(&env, value);
                Some(value)
            }
            None => None,
        };
        let timeout = custom_expiry.unwrap_or(default_timeout);
        let now = env.ledger().timestamp();
        let execute_after_ts = execute_after.unwrap_or(0);
        let status = if execute_after_ts > now {
            PaymentStatus::ScheduledPending
        } else {
            PaymentStatus::Pending
        };

        let payment_id = Self::next_payment_id(&env);
        let payment = Payment {
            id: payment_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            amount,
            token: token.clone(),
            status,
            created_at: now,
            expires_at: now + timeout,
            refunded_amount: 0,
            reference: reference.clone(),
            metadata,
            split_recipients,
            execute_after: execute_after_ts,
            category: None,
            tags: None,
            capture_deadline: 0,
            // // release_condition: None,
            external_id: None,
            tipping_enabled: false,
            extension_count: 0,
        };

        // Persistent: per-payment record with individual TTL
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::add_customer_payment(&env, &customer, payment_id);

        // Index by merchant+reference if provided (#67)
        if let Some(ref r) = reference {
            Self::index_payment_by_reference(&env, &merchant, r, payment_id);
        }

        // Store idempotency key if provided
        if let Some(key) = idempotency_key {
            env.storage()
                .temporary()
                .set(&DataKey::IdempotencyKey(key.clone()), &payment_id);
            env.storage().temporary().extend_ttl(
                &DataKey::IdempotencyKey(key),
                IDEMPOTENCY_KEY_LIFETIME_THRESHOLD,
                IDEMPOTENCY_KEY_BUMP_AMOUNT,
            );
        }

        // Update stats (#70)
        Self::inc_global_created(&env);
        Self::inc_merchant_created(&env, &merchant);

        events::emit_payment_created(&env, payment_id, customer, merchant, amount, token);
        if execute_after_ts > now {
            events::emit_payment_scheduled(&env, payment_id, execute_after_ts);
        }
        if let Some(value) = custom_expiry {
            events::emit_payment_expiry_override(&env, payment_id, value);
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        payment_id
    }

    /// Merchant blocks a customer with a reason code and evidence hash.
    pub fn block_customer(env: Env, merchant: Address, customer: Address, reason_code: Symbol, evidence_hash: BytesN<32>) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let entry = BlockEntry { customer: customer.clone(), reason_code: reason_code.clone(), evidence_hash, blocked_at_ledger: env.ledger().sequence(), };
        env.storage().persistent().set(&DataKey2::CustomerBlockEntry(merchant.clone(), customer.clone()), &entry);
        env.storage().persistent().extend_ttl(&DataKey2::CustomerBlockEntry(merchant.clone(), customer.clone()), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_customer_blocked(&env, merchant, customer, reason_code);
    }

    /// Customer requests an unblock within the dispute window; admin reviews later.
    pub fn request_unblock(env: Env, customer: Address, merchant: Address, dispute_hash: BytesN<32>) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let window: u32 = env.storage().instance().get(&DataKey2::MaxBlockAgeLedgers).unwrap_or(0u32);
        let now = env.ledger().sequence();
        let expires = if window == 0 { now } else { now + window };

        let mut counter: u32 = env.storage().instance().get(&DataKey::PaymentCounter).unwrap_or(0u32);
        counter += 1;
        env.storage().instance().set(&DataKey::PaymentCounter, &counter);

        let req = UnblockRequest { request_id: counter, merchant: merchant.clone(), customer: customer.clone(), dispute_hash, requested_at_ledger: now, expires_at_ledger: expires };
        env.storage().persistent().set(&DataKey2::UnblockRequest(counter), &req);
        env.storage().persistent().extend_ttl(&DataKey2::UnblockRequest(counter), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_unblock_requested(&env, counter, merchant, customer);
    }

    /// Admin resolves an unblock request.
    pub fn resolve_unblock_request(env: Env, admin: Address, request_id: u32, approved: bool) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        if admin != stored_admin { panic!("Only admin"); }

        let req: UnblockRequest = env.storage().persistent().get(&DataKey2::UnblockRequest(request_id)).expect("Request not found");
        if approved {
            env.storage().persistent().remove(&DataKey2::CustomerBlockEntry(req.merchant.clone(), req.customer.clone()));
            events::emit_customer_unblocked(&env, req.merchant.clone(), req.customer.clone(), admin);
        }
        env.storage().persistent().remove(&DataKey2::UnblockRequest(request_id));
    }

    /// Returns a pending unblock request by `request_id` (issue #648).
    /// Returns `None` once the request has been resolved or never existed.
    pub fn get_unblock_request(env: Env, request_id: u32) -> Option<UnblockRequest> {
        env.storage()
            .persistent()
            .get(&DataKey2::UnblockRequest(request_id))
    }

    /// Merchant voluntarily unblocks a customer immediately.
    pub fn unblock_customer(env: Env, merchant: Address, customer: Address) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        env.storage().persistent().remove(&DataKey2::CustomerBlockEntry(merchant.clone(), customer.clone()));
        events::emit_customer_unblocked(&env, merchant.clone(), customer, merchant);
    }

    /// #646: Returns the block entry for a customer, if one exists.
    /// 
    /// Allows customers and UIs to inspect why they are blocked before payment attempts.
    /// Returns the full BlockEntry containing reason_code, evidence_hash, and blocked_at_ledger,
    /// or None if the customer is not blocked.
    pub fn get_block_entry(env: Env, merchant: Address, customer: Address) -> Option<BlockEntry> {
        env.storage()
            .persistent()
            .get(&DataKey2::CustomerBlockEntry(merchant, customer))
    }

    /// #646: Convenience wrapper returning whether a customer is blocked by a merchant.
    /// 
    /// Returns true if the customer has a block entry, false otherwise.
    pub fn is_customer_blocked(env: Env, merchant: Address, customer: Address) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey2::CustomerBlockEntry(merchant, customer))
    }

    /// Create multiple payments atomically.
    /// Returns a Vec of payment IDs.
    pub fn create_payments_batch(
        env: Env,
        customer: Address,
        payments: Vec<PaymentRequest>,
    ) -> Vec<u32> {
        Self::require_not_paused(&env);
        customer.require_auth();

        let batch_len = payments.len();
        if batch_len == 0 {
            panic!("Batch cannot be empty");
        }
        Self::enforce_rate_limit(&env, &customer, batch_len);

        let max_batch_size: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxBatchSize)
            .unwrap_or(DEFAULT_MAX_BATCH_SIZE);

        if batch_len > max_batch_size {
            panic!("Batch size exceeds maximum allowed");
        }

        let mut payment_ids = Vec::new(&env);
        let mut total_amount: i128 = 0;

        let timeout: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentTimeout)
            .unwrap_or(DEFAULT_PAYMENT_TIMEOUT);
        let now = env.ledger().timestamp();

        for request in payments.iter() {
            if request.amount <= 0 {
                panic!("Payment amount must be positive");
            }

            Self::validate_reference(&env, &request.reference);
            Self::validate_metadata(&env, &request.metadata);
            Self::require_token_allowed(&env, &request.token);
            Self::require_merchant_approved(&env, &request.merchant);

            let client = token::Client::new(&env, &request.token);
            client.transfer(&customer, &env.current_contract_address(), &request.amount);

            let payment_id = Self::next_payment_id(&env);
            let payment = Payment {
                id: payment_id,
                customer: customer.clone(),
                merchant: request.merchant.clone(),
                amount: request.amount,
                token: request.token.clone(),
                status: PaymentStatus::Pending,
                created_at: now,
                expires_at: now + timeout,
                refunded_amount: 0,
                reference: request.reference.clone(),
                metadata: request.metadata.clone(),
                split_recipients: None,
                execute_after: 0,
                category: None,
                tags: None,
                capture_deadline: 0,
                // release_condition: None,
                external_id: None,
                tipping_enabled: false,
                extension_count: 0,
            };

            // Persistent: per-payment record with individual TTL
            env.storage()
                .persistent()
                .set(&DataKey::Payment(payment_id), &payment);
            env.storage().persistent().extend_ttl(
                &DataKey::Payment(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

            Self::add_customer_payment(&env, &customer, payment_id);

            // Index by merchant+reference if provided (#67)
            if let Some(ref r) = request.reference {
                Self::index_payment_by_reference(&env, &request.merchant, r, payment_id);
            }

            // Update stats (#70)
            Self::inc_global_created(&env);
            Self::inc_merchant_created(&env, &request.merchant);

            events::emit_payment_created(
                &env,
                payment_id,
                customer.clone(),
                request.merchant.clone(),
                request.amount,
                request.token.clone(),
            );

            payment_ids.push_back(payment_id);
            total_amount += request.amount;
        }

        events::emit_batch_payment_created(&env, customer, batch_len, total_amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        payment_ids
    }

    /// Admin completes an immediate payment.
    pub fn complete_payment(env: Env, payment_id: u32) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        Self::complete_payment_internal(&env, payment_id, false);
    }

    /// Execute a scheduled payment once execute_after has passed. Callable by anyone.
    pub fn execute_scheduled_payment(env: Env, payment_id: u32) {
        Self::require_not_paused(&env);
        Self::complete_payment_internal(&env, payment_id, true);
        events::emit_scheduled_payment_executed(&env, payment_id);
    }

    /// Customer can cancel a scheduled payment before execution and get refunded.
    pub fn cancel_scheduled_payment(env: Env, customer: Address, payment_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.customer != customer {
            panic!("Only payment customer can cancel scheduled payment");
        }
        if payment.status != PaymentStatus::ScheduledPending {
            panic!("Payment is not scheduled");
        }
        if env.ledger().timestamp() >= payment.execute_after {
            panic!("Scheduled payment is ready to execute");
        }

        let client = token::Client::new(&env, &payment.token);
        client.transfer(
            &env.current_contract_address(),
            &payment.customer,
            &payment.amount,
        );

        let old_status = payment.status;
        payment.status = PaymentStatus::Refunded;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::inc_global_refunded(&env, &payment.token, payment.amount);
        Self::inc_merchant_refunded(&env, &payment.merchant, &payment.token, payment.amount);
        events::emit_payment_status_changed(&env, payment_id, old_status, PaymentStatus::Refunded);
    }

    /// Settle a merchant batch in one transfer.
    /// Validates all payment IDs first, then executes settlement atomically.
    pub fn settle_merchant_payments(
        env: Env,
        admin: Address,
        merchant: Address,
        payment_ids: Vec<u32>,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can settle merchant payments");
        }

        let batch_size = payment_ids.len();
        if batch_size == 0 {
            panic!("Settlement batch cannot be empty");
        }
        if batch_size > MAX_SETTLEMENT_BATCH_SIZE {
            panic!("Settlement batch size exceeds maximum allowed");
        }

        let first_payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_ids.get(0).unwrap()))
            .expect("Payment not found");
        let settlement_token = first_payment.token.clone();

        let mut total_amount: i128 = 0;
        for payment_id in payment_ids.iter() {
            let payment: Payment = env
                .storage()
                .persistent()
                .get(&DataKey::Payment(payment_id))
                .expect("Payment not found");

            if payment.status != PaymentStatus::Completed {
                panic!("Payment is not completed");
            }
            if payment.merchant != merchant {
                panic!("Payment does not belong to merchant");
            }
            if payment.token != settlement_token {
                panic!("All payments in batch must have same token");
            }
            let settled: bool = env
                .storage()
                .persistent()
                .get(&DataKey::Settled(payment_id))
                .unwrap_or(false);
            if settled {
                panic!("Payment already settled");
            }

            total_amount = total_amount
                .checked_add(payment.amount)
                .expect("Settlement amount overflow");
        }

        // #367: Load 30-day rolling settlement volume and determine fee tier
        let now_ts = env.ledger().timestamp();
        let mut settlement_record: MerchantSettlementRecord = env
            .storage()
            .persistent()
            .get(&DataKey3::MerchantSettlementVolume(merchant.clone()))
            .unwrap_or(MerchantSettlementRecord {
                volume_30d: 0,
                window_start_timestamp: now_ts,
            });
        if now_ts.saturating_sub(settlement_record.window_start_timestamp) >= SETTLEMENT_VOLUME_WINDOW_SECONDS {
            settlement_record.volume_30d = 0;
            settlement_record.window_start_timestamp = now_ts;
        }
        let projected_volume = settlement_record.volume_30d.saturating_add(total_amount);
        let tier_fee_bps = Self::fee_bps_for_volume(&env, projected_volume);

        let fee_collected = (total_amount * tier_fee_bps as i128) / 10_000;
        let net_amount = total_amount
            .checked_sub(fee_collected)
            .expect("Settlement fee exceeds amount");

        // #231: Enforce withdrawal rate limit
        Self::check_and_update_withdrawal_rate_limit(&env, &merchant, net_amount);

        let token_client = token::Client::new(&env, &settlement_token);
        token_client.transfer(&env.current_contract_address(), &merchant, &net_amount);

        if fee_collected > 0 {
            if let Some(fee_recipient) = env
                .storage()
                .instance()
                .get::<_, Address>(&DataKey::FeeRecipient)
            {
                token_client.transfer(
                    &env.current_contract_address(),
                    &fee_recipient,
                    &fee_collected,
                );
            }
        }

        // #367: Persist updated rolling volume
        settlement_record.volume_30d = projected_volume;
        env.storage()
            .persistent()
            .set(&DataKey3::MerchantSettlementVolume(merchant.clone()), &settlement_record);
        env.storage().persistent().extend_ttl(
            &DataKey3::MerchantSettlementVolume(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        for payment_id in payment_ids.iter() {
            env.storage()
                .persistent()
                .set(&DataKey::Settled(payment_id), &true);
            env.storage().persistent().extend_ttl(
                &DataKey::Settled(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }

        events::emit_tier_fee_applied(
            &env,
            merchant.clone(),
            tier_fee_bps,
            fee_collected,
            projected_volume,
        );

        events::emit_batch_settlement_processed(
            &env,
            merchant,
            total_amount,
            fee_collected,
            batch_size,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // --- Dispute Methods ---

    /// Customer disputes a Pending payment. Dispute state stored in temporary storage
    /// (short-lived, in-progress — auto-expires once resolved or timed out).
    pub fn dispute_payment(env: Env, customer: Address, payment_id: u32, reason: String) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.customer != customer {
            panic!("Only the payment customer can dispute");
        }

        if payment.status != PaymentStatus::Pending && payment.status != PaymentStatus::Authorized {
            panic!("Only pending or authorized payments can be disputed");
        }

        let old_status = payment.status;
        payment.status = PaymentStatus::Disputed;

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Temporary: active dispute state — short-lived, expires if not resolved
        let dispute = Dispute {
            payment_id,
            reason: reason.clone(),
            created_at: env.ledger().timestamp(),
            resolved: false,
        };
        env.storage()
            .temporary()
            .set(&DataKey::Dispute(payment_id), &dispute);
        env.storage().temporary().extend_ttl(
            &DataKey::Dispute(payment_id),
            TEMP_LIFETIME_THRESHOLD,
            TEMP_BUMP_AMOUNT,
        );

        events::emit_payment_disputed(&env, payment_id, customer, reason);
        events::emit_payment_status_changed(&env, payment_id, old_status, PaymentStatus::Disputed);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin resolves a dispute. Clears temporary dispute state on resolution.
    pub fn resolve_dispute(env: Env, payment_id: u32, release_to_merchant: bool) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.status != PaymentStatus::Disputed
            && payment.status != PaymentStatus::EscalatedDispute
        {
            panic!("Payment is not disputed");
        }

        let client = token::Client::new(&env, &payment.token);
        let old_status = payment.status;

        if release_to_merchant {
            payment.status = PaymentStatus::Completed;
            env.storage()
                .persistent()
                .set(&DataKey::Settled(payment_id), &false);
            env.storage().persistent().extend_ttl(
                &DataKey::Settled(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        } else {
            // Resolved in customer's favour: refund from escrow first.
            // If the escrowed amount is insufficient (e.g. partial refunds already
            // issued), cover the shortfall by slashing merchant collateral (#129).
            let already_refunded = payment.refunded_amount;
            let owed_to_customer = payment.amount - already_refunded;

            if owed_to_customer > 0 {
                // Try to cover from escrow (the remaining escrowed balance).
                // The contract holds `owed_to_customer` for this payment in escrow.
                client.transfer(
                    &env.current_contract_address(),
                    &payment.customer,
                    &owed_to_customer,
                );
            }

            // Check whether collateral needs to be slashed.
            // Slashing applies when the merchant's pending balance is insufficient
            // to cover the refund — i.e. the payment token differs from the
            // collateral token (USDC) or the admin explicitly signals a shortfall.
            // For simplicity we slash collateral equal to the full disputed amount
            // only when the payment token is the collateral token (USDC), so the
            // slash covers the exact economic loss.
            let usdc_token: Option<Address> = env.storage().instance().get(&DataKey::UsdcToken);
            if let Some(ref usdc) = usdc_token {
                if payment.token == *usdc && owed_to_customer > 0 {
                    let collateral_key = DataKey::MerchantCollateral(payment.merchant.clone());
                    let collateral: i128 =
                        env.storage().persistent().get(&collateral_key).unwrap_or(0);

                    if collateral > 0 {
                        // Slash up to the full owed amount, capped by available collateral.
                        let slash_amount = owed_to_customer.min(collateral);
                        let new_collateral = collateral - slash_amount;
                        env.storage()
                            .persistent()
                            .set(&collateral_key, &new_collateral);
                        env.storage().persistent().extend_ttl(
                            &collateral_key,
                            PERSISTENT_LIFETIME_THRESHOLD,
                            PERSISTENT_BUMP_AMOUNT,
                        );
                        events::emit_collateral_slashed(
                            &env,
                            payment.merchant.clone(),
                            slash_amount,
                            payment_id,
                        );
                    }
                }
            }

            payment.status = PaymentStatus::Refunded;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Mark dispute resolved in temporary storage, then let it expire naturally
        if let Some(mut dispute) = env
            .storage()
            .temporary()
            .get::<DataKey, Dispute>(&DataKey::Dispute(payment_id))
        {
            dispute.resolved = true;
            env.storage()
                .temporary()
                .set(&DataKey::Dispute(payment_id), &dispute);
            // No TTL extension — resolved disputes can expire on their own
        }

        // Update stats (#70)
        if !release_to_merchant {
            Self::inc_global_refunded(&env, &payment.token, payment.amount);
            Self::inc_merchant_refunded(&env, &payment.merchant, &payment.token, payment.amount);
        }

        if !release_to_merchant {
            // Reverse loyalty points on customer-favoured resolution (#411)
            let refund_amt = payment.amount - payment.refunded_amount.min(payment.amount);
            Self::reverse_points_for_refund(&env, &payment.customer, refund_amt, payment.amount);
        }

        events::emit_dispute_resolved(&env, payment_id, release_to_merchant, admin);
        events::emit_payment_status_changed(&env, payment_id, old_status, payment.status);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Submit evidence hash for a dispute (#308)
    pub fn submit_dispute_evidence(
        env: Env,
        caller: Address,
        payment_id: u32,
        evidence_hash: BytesN<32>,
        evidence_type: Symbol,
    ) {
        Self::require_not_paused(&env);
        caller.require_auth();

        let payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.status != PaymentStatus::Disputed {
            panic_with_error!(&env, Error::EvidenceWindowClosed);
        }

        // Check if caller is customer or merchant
        let is_customer = payment.customer == caller;
        let is_merchant = payment.merchant == caller;
        if !is_customer && !is_merchant {
            panic!("Only customer or merchant can submit evidence");
        }

        // Get or create evidence record
        let mut evidence_record: DisputeEvidenceRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::DisputeEvidence(payment_id))
            .unwrap_or(DisputeEvidenceRecord {
                payment_id,
                customer_evidence: Vec::new(&env),
                merchant_evidence: Vec::new(&env),
                evidence_window_close_ledger: env.ledger().sequence()
                    + Self::get_evidence_window_ledgers(&env),
                resolution_note_hash: None,
            });

        // Check if evidence window is still open
        if env.ledger().sequence() >= evidence_record.evidence_window_close_ledger {
            panic_with_error!(&env, Error::EvidenceWindowClosed);
        }

        let max_submissions = Self::get_max_evidence_submissions(&env);
        let current_ledger = env.ledger().sequence();

        if is_customer {
            if evidence_record.customer_evidence.len() >= max_submissions {
                panic_with_error!(&env, Error::EvidenceLimitReached);
            }
            evidence_record
                .customer_evidence
                .push_back((evidence_hash.clone(), current_ledger as u64));
        } else {
            if evidence_record.merchant_evidence.len() >= max_submissions {
                panic_with_error!(&env, Error::EvidenceLimitReached);
            }
            evidence_record
                .merchant_evidence
                .push_back((evidence_hash.clone(), current_ledger as u64));
        }

        env.storage()
            .persistent()
            .set(&DataKey2::DisputeEvidence(payment_id), &evidence_record);
        env.storage().persistent().extend_ttl(
            &DataKey2::DisputeEvidence(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_dispute_evidence_submitted(
            &env,
            payment_id,
            caller,
            evidence_hash,
            evidence_type,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Close evidence submission window for a dispute (#308)
    pub fn close_evidence_window(env: Env, payment_id: u32) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let mut evidence_record: DisputeEvidenceRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::DisputeEvidence(payment_id))
            .expect("No evidence record found");

        evidence_record.evidence_window_close_ledger = env.ledger().sequence();

        env.storage()
            .persistent()
            .set(&DataKey2::DisputeEvidence(payment_id), &evidence_record);
        env.storage().persistent().extend_ttl(
            &DataKey2::DisputeEvidence(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_evidence_window_closed(&env, payment_id);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get dispute evidence record (#308)
    pub fn get_dispute_evidence(env: Env, payment_id: u32) -> DisputeEvidenceRecord {
        env.storage()
            .persistent()
            .get(&DataKey2::DisputeEvidence(payment_id))
            .expect("No evidence record found")
    }

    /// Resolve dispute with evidence hash (#308)
    pub fn resolve_dispute_with_evidence(
        env: Env,
        payment_id: u32,
        winner: bool, // true = merchant, false = customer
        resolution_note_hash: BytesN<32>,
    ) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let mut evidence_record: DisputeEvidenceRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::DisputeEvidence(payment_id))
            .expect("No evidence record found");

        evidence_record.resolution_note_hash = Some(resolution_note_hash);

        env.storage()
            .persistent()
            .set(&DataKey2::DisputeEvidence(payment_id), &evidence_record);
        env.storage().persistent().extend_ttl(
            &DataKey2::DisputeEvidence(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Resolve the underlying dispute
        Self::resolve_dispute(env, payment_id, winner);
    }

    /// Create payment with cooling-off period (#309)
    pub fn create_payment_with_cooling_off(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        cooling_off_ledgers: u32,
    ) -> u32 {
        // Validate cooling-off period
        let max_cooling_off = Self::get_max_cooling_off_ledgers(&env);
        if cooling_off_ledgers > max_cooling_off && max_cooling_off > 0 {
            panic_with_error!(&env, Error::CoolingOffExceedsMax);
        }

        // Create payment normally
        let payment_id =
            Self::create_payment(env.clone(), customer.clone(), merchant, amount, token, None, None, None);

        // Store cooling-off config if enabled
        if cooling_off_ledgers > 0 {
            let cooling_off_config = CoolingOffConfig {
                cooling_off_ledgers,
                cooling_off_expiry_ledger: 0, // Will be set on completion
            };
            env.storage()
                .persistent()
                .set(&DataKey2::CoolingOffConfig(payment_id), &cooling_off_config);
            env.storage().persistent().extend_ttl(
                &DataKey2::CoolingOffConfig(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }

        payment_id
    }

    /// Cancel payment during cooling-off period (#309)
    pub fn cancel_during_cooling_off(env: Env, customer: Address, payment_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.customer != customer {
            panic!("Only the payment customer can cancel");
        }

        if payment.status != PaymentStatus::CoolingOff {
            panic_with_error!(&env, Error::NotInCoolingOff);
        }

        let cooling_off_config: CoolingOffConfig = env
            .storage()
            .persistent()
            .get(&DataKey2::CoolingOffConfig(payment_id))
            .expect("No cooling-off config found");

        if env.ledger().sequence() >= cooling_off_config.cooling_off_expiry_ledger {
            panic_with_error!(&env, Error::CoolingOffExpired);
        }

        // Refund full amount to customer
        let client = token::Client::new(&env, &payment.token);
        client.transfer(&env.current_contract_address(), &customer, &payment.amount);

        payment.status = PaymentStatus::CancelledInCoolingOff;
        payment.refunded_amount = payment.amount;

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_cooling_off_cancellation(&env, payment_id, customer, payment.amount);
        events::emit_payment_status_changed(
            &env,
            payment_id,
            PaymentStatus::CoolingOff,
            PaymentStatus::CancelledInCoolingOff,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Set evidence window in ledgers (#308)
    pub fn set_evidence_window_ledgers(env: Env, admin: Address, ledgers: u32) {
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey2::EvidenceWindowLedgers, &ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get evidence window in ledgers (#308)
    pub fn get_evidence_window_ledgers(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey2::EvidenceWindowLedgers)
            .unwrap_or(DEFAULT_EVIDENCE_WINDOW_LEDGERS)
    }

    /// Set max evidence submissions per party (#308)
    pub fn set_max_evidence_submissions(env: Env, admin: Address, max_submissions: u32) {
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey2::MaxEvidenceSubmissions, &max_submissions);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get max evidence submissions per party (#308)
    pub fn get_max_evidence_submissions(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey2::MaxEvidenceSubmissions)
            .unwrap_or(MAX_EVIDENCE_SUBMISSIONS)
    }

    /// Set max cooling-off period in ledgers (#309)
    pub fn set_max_cooling_off_ledgers(env: Env, admin: Address, max_ledgers: u32) {
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey2::MaxCoolingOffLedgers, &max_ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get max cooling-off period in ledgers (#309)
    pub fn get_max_cooling_off_ledgers(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey2::MaxCoolingOffLedgers)
            .unwrap_or(DEFAULT_MAX_COOLING_OFF_LEDGERS)
    }

    /// Check if escalation timeout reached; if so, transition payment to EscalatedDispute (#417)
    pub fn check_escalation(env: Env, payment_id: u32) -> bool {
        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.status != PaymentStatus::Disputed {
            return false;
        }

        let dispute: Dispute = env
            .storage()
            .temporary()
            .get(&DataKey::Dispute(payment_id))
            .expect("Dispute not found");

        if dispute.resolved {
            return false;
        }

        let timeout: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeTimeout)
            .unwrap_or(DEFAULT_DISPUTE_TIMEOUT);

        let elapsed = env.ledger().timestamp() - dispute.created_at;
        if elapsed > timeout {
            let old_status = payment.status;
            payment.status = PaymentStatus::EscalatedDispute;
            env.storage()
                .persistent()
                .set(&DataKey::Payment(payment_id), &payment);
            env.storage().persistent().extend_ttl(
                &DataKey::Payment(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_dispute_escalated(&env, payment_id, elapsed);
            events::emit_payment_status_changed(
                &env,
                payment_id,
                old_status,
                PaymentStatus::EscalatedDispute,
            );
            return true;
        }

        false
    }

    // --- Oracle / Multi-Token ---

    /// Admin sets the oracle contract address, USDC token address, and max
    /// oracle price age. Must be called before create_payment_multi_token.
    pub fn set_oracle(env: Env, oracle: Address, usdc_token: Address, max_oracle_age: u64) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        if max_oracle_age == 0 {
            panic!("max_oracle_age must be positive");
        }

        env.storage()
            .instance()
            .set(&DataKey::OracleAddress, &oracle);
        env.storage()
            .instance()
            .set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::MaxOracleAge, &max_oracle_age);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Create a payment where the customer pays in any supported token.
    /// The oracle provides the token/USDC rate. The contract:
    ///   1. Queries the oracle for the current price of `payment_token` in USDC.
    ///   2. Validates price freshness against `max_oracle_age`.
    ///   3. Calculates `required_token_amount` from `amount_usdc` and the rate.
    ///   4. Applies slippage tolerance: rejects if effective rate deviates
    ///      more than `slippage_tolerance_bps` basis points from the oracle rate.
    ///      If omitted, uses the global default from SlippageConfig.
    ///   5. Transfers `required_token_amount` of `payment_token` from customer
    ///      to contract (escrow).
    ///   6. Records the payment with `amount = amount_usdc` and `token = usdc_token`
    ///      so that complete_payment always releases USDC to the merchant.
    ///
    /// Fallback: if `payment_token == usdc_token`, behaves identically to
    /// create_payment (no oracle call, no conversion).
    ///
    /// Returns the new payment ID.
    pub fn create_payment_multi_token(
        env: Env,
        customer: Address,
        merchant: Address,
        amount_usdc: i128,
        payment_token: Address,
        slippage_tolerance_bps: Option<u32>,
    ) -> u32 {
        Self::require_not_paused(&env);
        if amount_usdc <= 0 {
            panic!("Payment amount must be positive");
        }

        // Resolve slippage: use provided value or fall back to global default
        let slippage_cfg = Self::get_slippage_config_internal(&env);
        let slippage_bps = match slippage_tolerance_bps {
            Some(bps) => {
                if bps < slippage_cfg.min_bps {
                    panic!("slippage_bps below minimum allowed");
                }
                if bps > slippage_cfg.max_bps {
                    panic!("slippage_bps exceeds maximum allowed");
                }
                bps
            }
            None => slippage_cfg.default_bps,
        };

        let usdc_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::UsdcToken)
            .expect("Oracle not configured");

        // --- Fallback: direct USDC payment, no oracle needed ---
        if payment_token == usdc_token {
            customer.require_auth();
            Self::enforce_rate_limit(&env, &customer, 1);
            Self::require_token_allowed(&env, &payment_token);
            Self::require_merchant_approved(&env, &merchant);

            let client = token::Client::new(&env, &payment_token);
            client.transfer(&customer, &env.current_contract_address(), &amount_usdc);

            let timeout: u64 = env
                .storage()
                .instance()
                .get(&DataKey::PaymentTimeout)
                .unwrap_or(DEFAULT_PAYMENT_TIMEOUT);
            let now = env.ledger().timestamp();

            let payment_id = Self::next_payment_id(&env);
            let payment = Payment {
                id: payment_id,
                customer: customer.clone(),
                merchant: merchant.clone(),
                amount: amount_usdc,
                token: payment_token.clone(),
                status: PaymentStatus::Pending,
                created_at: now,
                expires_at: now + timeout,
                refunded_amount: 0,
                reference: None,
                metadata: None,
                split_recipients: None,
                execute_after: 0,
                category: None,
                tags: None,
                capture_deadline: 0,
                // release_condition: None,
                external_id: None,
                tipping_enabled: false,
                extension_count: 0,
            };

            env.storage()
                .persistent()
                .set(&DataKey::Payment(payment_id), &payment);
            env.storage().persistent().extend_ttl(
                &DataKey::Payment(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

            Self::add_customer_payment(&env, &customer, payment_id);
            events::emit_payment_created(
                &env,
                payment_id,
                customer,
                merchant,
                amount_usdc,
                payment_token,
            );
            env.storage()
                .instance()
                .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

            return payment_id;
        }

        customer.require_auth();
        Self::enforce_rate_limit(&env, &customer, 1);
        Self::require_token_allowed(&env, &payment_token);
        Self::require_merchant_approved(&env, &merchant);

        let oracle_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::OracleAddress)
            .expect("Oracle not configured");
        let max_oracle_age: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MaxOracleAge)
            .expect("Oracle not configured");

        // --- Query oracle: price of payment_token denominated in USDC ---
        // Oracle returns price scaled by ORACLE_PRICE_PRECISION (10^7).
        let oracle_client = oracle::OracleClient::new(&env, &oracle_addr);
        let price_data: PriceData = oracle_client
            .lastprice(&payment_token, &usdc_token)
            .expect("Oracle price unavailable");

        // --- Freshness check ---
        let current_ts = env.ledger().timestamp();
        let age = current_ts.saturating_sub(price_data.timestamp);
        if age > max_oracle_age {
            panic!("Oracle price is stale");
        }

        if price_data.price <= 0 {
            panic!("Invalid oracle price");
        }

        // --- Calculate required payment_token amount ---
        // price = payment_token per USDC, scaled by 10^7
        // required = amount_usdc * 10^7 / price
        let required_token_amount = (amount_usdc * ORACLE_PRICE_PRECISION) / price_data.price;
        if required_token_amount <= 0 {
            panic!("Computed token amount is zero");
        }

        // --- Slippage check ---
        // Effective USDC value of required_token_amount at oracle rate must be
        // within slippage_bps of amount_usdc.
        // effective_usdc = required_token_amount * price / 10^7
        // deviation_bps = abs(effective_usdc - amount_usdc) * 10000 / amount_usdc
        let effective_usdc = (required_token_amount * price_data.price) / ORACLE_PRICE_PRECISION;
        let deviation = if effective_usdc >= amount_usdc {
            effective_usdc - amount_usdc
        } else {
            amount_usdc - effective_usdc
        };
        let deviation_bps = (deviation * 10_000) / amount_usdc;
        if deviation_bps > slippage_bps as i128 {
            panic!("Slippage tolerance exceeded");
        }

        // --- Transfer payment_token from customer to contract (escrow) ---
        let pay_client = token::Client::new(&env, &payment_token);
        pay_client.transfer(
            &customer,
            &env.current_contract_address(),
            &required_token_amount,
        );

        // --- Record payment in USDC terms so settlement releases USDC ---
        let timeout: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentTimeout)
            .unwrap_or(DEFAULT_PAYMENT_TIMEOUT);
        let now = env.ledger().timestamp();
        let payment_id = Self::next_payment_id(&env);
        let payment = Payment {
            id: payment_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            amount: amount_usdc,
            token: usdc_token.clone(),
            status: PaymentStatus::Pending,
            created_at: now,
            expires_at: now + timeout,
            refunded_amount: 0,
            reference: None,
            metadata: None,
            split_recipients: None,
            execute_after: 0,
            category: None,
            tags: None,
            capture_deadline: 0,
            // release_condition: None,
            external_id: None,
            tipping_enabled: false,
            extension_count: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::add_customer_payment(&env, &customer, payment_id);

        // Store DynamicPayment so settle_dynamic_payment_if_needed can find it
        let dynamic = DynamicPayment {
            payment_id,
            fiat_amount: amount_usdc,
            fiat_currency: Symbol::new(&env, "USDC"),
            oracle_address: oracle_addr,
            token: payment_token.clone(),
            slippage_bps,
            creation_rate: price_data.price,
            expiry: payment.expires_at,
        };
        env.storage()
            .persistent()
            .set(&DataKey2::DynamicPayment(payment_id), &dynamic);
        env.storage().persistent().extend_ttl(
            &DataKey2::DynamicPayment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Emit SlippageToleranceApplied event (#135)
        events::emit_slippage_tolerance_applied(
            &env,
            payment_id,
            slippage_bps,
            price_data.price,
            amount_usdc,
        );

        events::emit_multi_token_payment_created(
            &env,
            payment_id,
            customer,
            merchant,
            amount_usdc,
            payment_token,
            required_token_amount,
            price_data.price,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        payment_id
    }

    pub fn get_oracle_address(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::OracleAddress)
            .expect("Oracle not configured")
    }

    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::UsdcToken)
            .expect("Oracle not configured")
    }

    pub fn get_max_oracle_age(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MaxOracleAge)
            .expect("Oracle not configured")
    }

    // --- Admin ---

    pub fn set_merchant_slippage_tolerance(env: Env, merchant: Address, bps: u32) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        env.storage().persistent().set(&DataKey3::SlippageToleranceBps(merchant), &bps);
    }

    pub fn set_max_batch_size(env: Env, new_size: u32) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        if new_size == 0 {
            panic!("Max batch size must be at least 1");
        }

        env.storage()
            .instance()
            .set(&DataKey::MaxBatchSize, &new_size);
    }

    pub fn set_dispute_timeout(env: Env, timeout: u64) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        if timeout == 0 {
            panic!("Dispute timeout must be positive");
        }

        env.storage()
            .instance()
            .set(&DataKey::DisputeTimeout, &timeout);
    }

    /// Admin updates the protocol fee. Maximum allowed is 500 bps (5%).
    pub fn update_fee(env: Env, admin: Address, new_fee_bps: u32) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can update fee");
        }
        if new_fee_bps > MAX_FEE_BPS {
            panic!("Fee cannot exceed 500 bps (5%)");
        }

        env.storage().instance().set(&DataKey::FeeBps, &new_fee_bps);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin updates the fee recipient address.
    pub fn update_fee_recipient(env: Env, admin: Address, new_fee_recipient: Address) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can update fee recipient");
        }

        env.storage()
            .instance()
            .set(&DataKey::FeeRecipient, &new_fee_recipient);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the current protocol fee in basis points.
    pub fn get_fee_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or(DEFAULT_FEE_BPS)
    }

    /// Get the current fee recipient address.
    pub fn get_fee_recipient(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::FeeRecipient)
            .expect("Fee recipient not configured")
    }

    /// Admin updates the ascending fee tier table.
    pub fn update_fee_tiers(env: Env, admin: Address, tiers: Vec<FeeTier>) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can update fee tiers");
        }

        Self::validate_fee_tiers(&tiers);
        env.storage().instance().set(&DataKey::FeeTiers, &tiers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn get_fee_tiers(env: Env) -> Vec<FeeTier> {
        env.storage()
            .instance()
            .get(&DataKey::FeeTiers)
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_merchant_fee_tier(env: Env, merchant: Address) -> u32 {
        let volume = Self::rolling_merchant_volume(&env, &merchant);
        Self::fee_bps_for_volume(&env, volume)
    }

    // ── #367: Settlement-specific tier view ───────────────────────────────────

    /// Returns the merchant's current settlement fee tier info:
    /// `(fee_bps, volume_30d, volume_headroom)` where `volume_headroom` is the
    /// remaining capacity in the current tier before the next threshold is crossed.
    /// Returns (default_fee_bps, 0, i128::MAX) if no settlement history exists.
    pub fn get_merchant_settlement_tier(env: Env, merchant: Address) -> (u32, i128, i128) {
        let now_ts = env.ledger().timestamp();
        let record: MerchantSettlementRecord = env
            .storage()
            .persistent()
            .get(&DataKey3::MerchantSettlementVolume(merchant.clone()))
            .unwrap_or(MerchantSettlementRecord {
                volume_30d: 0,
                window_start_timestamp: now_ts,
            });
        let volume = if now_ts.saturating_sub(record.window_start_timestamp) >= SETTLEMENT_VOLUME_WINDOW_SECONDS {
            0i128
        } else {
            record.volume_30d
        };
        let fee_bps = Self::fee_bps_for_volume(&env, volume);
        let tiers: Vec<FeeTier> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTiers)
            .unwrap_or(Vec::new(&env));
        // Find the next tier threshold above current volume
        let mut headroom = i128::MAX;
        for tier in tiers.iter() {
            if tier.min_volume > volume {
                let gap = tier.min_volume.saturating_sub(volume);
                if gap < headroom {
                    headroom = gap;
                }
            }
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        (fee_bps, volume, headroom)
    }

    /// Admin updates global per-customer payment rate limit settings.
    pub fn update_rate_limit_config(
        env: Env,
        admin: Address,
        max_payments: u32,
        window_size_ledgers: u32,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can update rate limit config");
        }
        if max_payments == 0 {
            panic!("max_payments must be positive");
        }
        if window_size_ledgers == 0 {
            panic!("window_size_ledgers must be positive");
        }

        env.storage().instance().set(
            &DataKey::RateLimitConfig,
            &RateLimitConfig {
                max_payments,
                window_size_ledgers,
            },
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin updates the global slippage tolerance config for multi-token payments (#135).
    /// default_bps must be within [min_bps, max_bps].
    pub fn update_slippage_config(
        env: Env,
        admin: Address,
        default_bps: u32,
        min_bps: u32,
        max_bps: u32,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can update slippage config");
        }
        if max_bps > 10_000 {
            panic!("max_bps cannot exceed 10000");
        }
        if min_bps > max_bps {
            panic!("min_bps cannot exceed max_bps");
        }
        if default_bps < min_bps || default_bps > max_bps {
            panic!("default_bps must be within [min_bps, max_bps]");
        }
        env.storage().instance().set(
            &DataKey::SlippageConfig,
            &SlippageConfig {
                default_bps,
                min_bps,
                max_bps,
            },
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin updates the maximum allowed additional ledgers per extension and max number of extensions.
    pub fn update_extension_config(
        env: Env,
        admin: Address,
        max_extension_ledgers: u32,
        max_extensions: u32,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can update extension config");
        }
        if max_extension_ledgers == 0 {
            panic!("max_extension_ledgers must be positive");
        }
        if max_extensions == 0 {
            panic!("max_extensions must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey2::MaxExtensionLedgers, &max_extension_ledgers);
        env.storage()
            .instance()
            .set(&DataKey2::MaxExtensions, &max_extensions);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the current extension config.
    pub fn get_extension_config(env: Env) -> (u32, u32) {
        let max_extension_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::MaxExtensionLedgers)
            .unwrap_or(DEFAULT_MAX_EXTENSION_LEDGERS);
        let max_extensions: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::MaxExtensions)
            .unwrap_or(DEFAULT_MAX_EXTENSIONS);
        (max_extension_ledgers, max_extensions)
    }

    /// Merchant extends a pending payment's expiry by additional_ledgers.
    /// additional_ledgers must not exceed max_extension_ledgers.
    /// A payment can be extended up to max_extensions times.
    pub fn extend_payment_expiry(
        env: Env,
        merchant: Address,
        payment_id: u32,
        additional_ledgers: u32,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.merchant != merchant {
            panic!("Only the payment merchant can extend expiry");
        }

        if payment.status != PaymentStatus::Pending {
            panic_with_error!(&env, Error::InvalidPaymentStatus);
        }

        let (max_extension_ledgers, max_extensions) = Self::get_extension_config(env.clone());

        if additional_ledgers > max_extension_ledgers {
            panic_with_error!(&env, Error::MaxExtensionLedgersExceeded);
        }

        if payment.extension_count >= max_extensions {
            panic_with_error!(&env, Error::MaxExtensionsReached);
        }

        // Calculate new expiry: use ledger timestamp + additional ledgers * 5s (or just add additional_ledgers as seconds? Wait, wait - wait, the existing code uses timestamp in seconds for expires_at! Wait a second, let's check that!
        // Wait in create_payment_with_expiry, expires_at is set to now + timeout, where timeout is in seconds! Oh, but the user's issue says "additional_ledgers"! Hmm, what's the right approach here? Let's check how other parts of the codebase handle ledgers vs time!
        // Wait let's check: the default max extension ledgers is 30 days, which at 5s/ledger is 30 * 24 * 60 * 60 /5 = 518400 ledgers. But if expires_at is stored in seconds, then 1 ledger = 5 seconds, so additional_ledgers * 5 seconds!
        // Wait let's check the rest of the codebase to confirm!
        // Wait let's check the existing code for expires_at usage! For example, how is a payment considered expired?
        // Let's look for expire_payment or similar!
        let now = env.ledger().timestamp();
        // Assuming 1 ledger is 5 seconds (common in Stellar Soroban), but wait - actually, maybe the user intended additional_ledgers to mean additional seconds? Wait, let's check the issue description again! Wait the issue says:
        // "merchant calls extend_payment_expiry(payment_id, additional_ledgers: u32) on a Pending payment before it expires, the contract advances expiry_ledger by additional_ledgers."
        // But wait in our current code, we don't have expiry_ledger - we have expires_at which is a timestamp in seconds! Oh! Wait let's check the existing codebase to see if there's an expiry_ledger field anywhere!
        // Wait let's search the codebase!
        // Wait let's look for "expiry" in lib.rs!
        // Wait let's search for "expire" in lib.rs!
        // Let's check for how payments are expired!
        // Let's keep reading lib.rs from where we left off!

        // Okay, so expires_at is a timestamp in seconds! So how do we handle additional_ledgers? Let's check if the codebase has a way to convert ledgers to time, or if maybe we should just treat additional_ledgers as seconds? Wait let's check the DEFAULT_MAX_EXTENSION_LEDGERS we set earlier: we set it to 30 days in seconds divided by 5, assuming 5s per ledger! So let's multiply additional_ledgers by 5 seconds to get the additional time!
        let additional_seconds = additional_ledgers as u64 * 5;
        payment.expires_at = payment
            .expires_at
            .checked_add(additional_seconds)
            .expect("Expiry overflow");
        payment.extension_count += 1;

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_payment_expiry_extended(
            &env,
            payment_id,
            payment.expires_at,
            payment.extension_count,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the current slippage tolerance config.
    pub fn get_slippage_config(env: Env) -> SlippageConfig {
        Self::get_slippage_config_internal(&env)
    }

    /// Admin sets a volume cap for a merchant (#131).
    /// cap_amount: maximum cumulative settlement volume within window_seconds.
    /// Set cap_amount = 0 to remove the cap.
    pub fn set_merchant_volume_cap(
        env: Env,
        admin: Address,
        merchant: Address,
        cap_amount: i128,
        window_seconds: u64,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set merchant volume cap");
        }
        if cap_amount < 0 {
            panic!("cap_amount cannot be negative");
        }
        if cap_amount > 0 && window_seconds == 0 {
            panic!("window_seconds must be positive when cap is set");
        }
        let key = DataKey::VolumeCap(merchant.clone());
        if cap_amount == 0 {
            env.storage().persistent().remove(&key);
        } else {
            env.storage().persistent().set(
                &key,
                &VolumeCap {
                    cap_amount,
                    window_seconds,
                },
            );
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the volume cap for a merchant, if set (#131).
    pub fn get_merchant_volume_cap(env: Env, merchant: Address) -> Option<VolumeCap> {
        env.storage()
            .persistent()
            .get(&DataKey::VolumeCap(merchant))
    }

    /// Propose a new admin address. Only the current admin can propose.
    pub fn propose_admin_transfer(env: Env, proposed_admin: Address) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        env.storage()
            .instance()
            .set(&DataKey::ProposedAdmin, &proposed_admin);

        events::emit_admin_transfer_proposed(&env, admin, proposed_admin);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Accept the admin role. Only the proposed admin can accept.
    pub fn accept_admin_role(env: Env) {
        Self::require_not_paused(&env);
        let proposed_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::ProposedAdmin)
            .expect("No admin transfer proposed");
        proposed_admin.require_auth();

        let old_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");

        env.storage()
            .instance()
            .set(&DataKey::Admin, &proposed_admin);
        env.storage().instance().remove(&DataKey::ProposedAdmin);

        events::emit_admin_transferred(&env, old_admin, proposed_admin);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the current admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    /// Upgrade this contract's WASM code. Admin only.
    pub fn upgrade(env: Env, admin: Address, new_wasm_hash: BytesN<32>) {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can upgrade contract");
        }

        let old_version = Self::get_or_init_version(&env);
        env.deployer().update_current_contract_wasm(new_wasm_hash);

        let new_version = old_version.checked_add(1).expect("Version overflow");
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &new_version);

        events::emit_contract_upgraded(&env, old_version, new_version, admin);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Run one-time migration logic for the current version. Admin only.
    pub fn migrate(env: Env, admin: Address) {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can migrate contract");
        }

        let version = Self::get_or_init_version(&env);
        if env
            .storage()
            .instance()
            .get(&DataKey::MigrationCompleted(version))
            .unwrap_or(false)
        {
            panic!("Migration already completed for this version");
        }

        env.storage()
            .instance()
            .set(&DataKey::MigrationCompleted(version), &true);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the current contract version.
    pub fn get_version(env: Env) -> u32 {
        Self::get_or_init_version(&env)
    }

    /// Get the proposed admin address, if any.
    pub fn get_proposed_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::ProposedAdmin)
    }

    // --- Read Interface ---

    /// Returns global aggregate statistics (#70).
    pub fn get_stats(env: Env) -> GlobalStats {
        env.storage()
            .persistent()
            .get(&DataKey::GlobalStats)
            .unwrap_or(GlobalStats {
                total_payments_created: 0,
                total_payments_completed: 0,
                total_payments_refunded: 0,
                total_payments_expired: 0,
                total_volume_completed: Map::new(&env),
                total_volume_refunded: Map::new(&env),
            })
    }

    /// Returns per-merchant aggregate statistics (#70).
    pub fn get_merchant_stats(env: Env, merchant: Address) -> MerchantStats {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantStats(merchant))
            .unwrap_or(MerchantStats {
                payments_created: 0,
                payments_completed: 0,
                payments_refunded: 0,
                volume_completed: Map::new(&env),
            })
    }

    /// Returns the completed volume for a token in the current weekly ledger bucket (#70).
    pub fn get_weekly_volume(env: Env, token: Address) -> i128 {
        let bucket = env.ledger().sequence() / LEDGER_BUCKET_SIZE;
        env.storage()
            .persistent()
            .get(&DataKey::VolumeBucket(token, bucket))
            .unwrap_or(0)
    }

    pub fn get_payment(env: Env, payment_id: u32) -> Payment {
        env.storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found")
    }

    // =========================================================================
    // Task 1: External ID — Off-Chain Order Correlation
    // =========================================================================

    /// Look up a payment by merchant + external_id.
    /// Returns the full Payment struct.
    pub fn get_payment_by_external_id(
        env: Env,
        merchant: Address,
        external_id: BytesN<32>,
    ) -> Payment {
        let payment_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::ExternalIdIndex(merchant, external_id))
            .expect("No payment found for this external_id");
        env.storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found")
    }

    // =========================================================================
    // Task 2: Multi-Signature Approval for High-Value Payments
    // =========================================================================

    /// Merchant (or admin) configures a multi-sig policy.
    /// `threshold`: minimum payment amount that requires approval.
    /// `signers`: authorized co-signer addresses.
    /// `m`: number of approvals required (must be ≤ signers.len()).
    /// `approval_window_seconds`: time window before unapproved payment auto-cancels.
    pub fn set_multisig_policy(
        env: Env,
        merchant: Address,
        threshold: i128,
        signers: Vec<Address>,
        m: u32,
        approval_window_seconds: u64,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if threshold <= 0 {
            panic!("threshold must be positive");
        }
        if signers.is_empty() {
            panic!("signers cannot be empty");
        }
        if m == 0 || m > signers.len() {
            panic!("m must be between 1 and signers.len()");
        }
        if approval_window_seconds == 0 {
            panic!("approval_window_seconds must be positive");
        }

        let policy = MultisigPolicy {
            m,
            signers,
            threshold,
            approval_window_seconds,
        };
        env.storage()
            .instance()
            .set(&DataKey::MultisigPolicy(merchant), &policy);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the multi-sig policy for a merchant.
    pub fn get_multisig_policy(env: Env, merchant: Address) -> Option<MultisigPolicy> {
        env.storage()
            .instance()
            .get(&DataKey::MultisigPolicy(merchant))
    }

    /// Get the approval state for a payment.
    /// Returns `None` for payments that were never routed through multisig approval.
    pub fn get_approval_state(env: Env, payment_id: u32) -> Option<ApprovalState> {
        env.storage()
            .persistent()
            .get(&DataKey2::ApprovalState(payment_id))
    }

    /// A signer approves a PendingApproval payment.
    /// Once `m` approvals are recorded the payment transitions to Pending.
    pub fn approve_payment(env: Env, signer: Address, payment_id: u32) {
        Self::require_not_paused(&env);
        signer.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.status != PaymentStatus::PendingApproval {
            panic!("Payment is not pending approval");
        }

        let policy: MultisigPolicy = env
            .storage()
            .instance()
            .get(&DataKey::MultisigPolicy(payment.merchant.clone()))
            .expect("No multisig policy for merchant");

        // Verify signer is in the policy set
        let mut is_valid_signer = false;
        for i in 0..policy.signers.len() {
            if policy.signers.get(i).unwrap() == signer {
                is_valid_signer = true;
                break;
            }
        }
        if !is_valid_signer {
            panic_with_error!(&env, Error::NotASigner);
        }

        // Check approval window
        let mut state: ApprovalState = env
            .storage()
            .persistent()
            .get(&DataKey2::ApprovalState(payment_id))
            .expect("Approval state not found");

        let now = env.ledger().timestamp();
        if now > state.created_at + policy.approval_window_seconds {
            // Window expired — auto-cancel and refund
            let client = token::Client::new(&env, &payment.token);
            client.transfer(
                &env.current_contract_address(),
                &payment.customer,
                &payment.amount,
            );
            let old_status = payment.status;
            payment.status = PaymentStatus::Refunded;
            env.storage()
                .persistent()
                .set(&DataKey::Payment(payment_id), &payment);
            env.storage().persistent().extend_ttl(
                &DataKey::Payment(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_payment_approval_expired(&env, payment_id);
            events::emit_payment_status_changed(
                &env,
                payment_id,
                old_status,
                PaymentStatus::Refunded,
            );
            env.storage()
                .instance()
                .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
            return;
        }

        // Check for duplicate approval
        for i in 0..state.approvals.len() {
            if state.approvals.get(i).unwrap() == signer {
                panic_with_error!(&env, Error::AlreadyApproved);
            }
        }

        state.approvals.push_back(signer.clone());
        events::emit_payment_approved(&env, payment_id, signer);

        env.storage()
            .persistent()
            .set(&DataKey2::ApprovalState(payment_id), &state);
        env.storage().persistent().extend_ttl(
            &DataKey2::ApprovalState(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        if state.approvals.len() >= policy.m {
            // Quorum reached — transition to Pending
            let old_status = payment.status;
            payment.status = PaymentStatus::Pending;
            env.storage()
                .persistent()
                .set(&DataKey::Payment(payment_id), &payment);
            env.storage().persistent().extend_ttl(
                &DataKey::Payment(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_payment_status_changed(
                &env,
                payment_id,
                old_status,
                PaymentStatus::Pending,
            );
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Anyone can call this to expire an unapproved payment after the approval window.
    pub fn expire_pending_approval(env: Env, payment_id: u32) {
        Self::require_not_paused(&env);

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.status != PaymentStatus::PendingApproval {
            panic!("Payment is not pending approval");
        }

        let policy: MultisigPolicy = env
            .storage()
            .instance()
            .get(&DataKey::MultisigPolicy(payment.merchant.clone()))
            .expect("No multisig policy for merchant");

        let state: ApprovalState = env
            .storage()
            .persistent()
            .get(&DataKey2::ApprovalState(payment_id))
            .expect("Approval state not found");

        let now = env.ledger().timestamp();
        if now <= state.created_at + policy.approval_window_seconds {
            panic!("Approval window has not expired yet");
        }

        let client = token::Client::new(&env, &payment.token);
        client.transfer(
            &env.current_contract_address(),
            &payment.customer,
            &payment.amount,
        );

        let old_status = payment.status;
        payment.status = PaymentStatus::Refunded;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_payment_approval_expired(&env, payment_id);
        events::emit_payment_status_changed(&env, payment_id, old_status, PaymentStatus::Refunded);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // =========================================================================
    // Task 3: Voucher / Coupon Code Redemption
    // =========================================================================

    /// Merchant issues a voucher on-chain.
    /// `code_hash`: sha256 hash of the promo code (prevents front-running).
    /// `discount_type`: Fixed or Percentage.
    /// `discount_value`: token units (Fixed) or 0–100 (Percentage).
    /// `max_uses`: maximum redemptions; 0 = unlimited.
    /// `expiry`: ledger timestamp after which voucher is invalid; 0 = no expiry.
    pub fn issue_voucher(
        env: Env,
        merchant: Address,
        code_hash: BytesN<32>,
        discount_type: DiscountType,
        discount_value: u32,
        max_uses: u32,
        expiry: u64,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if discount_type == DiscountType::Percentage && discount_value > 100 {
            panic!("Percentage discount cannot exceed 100");
        }
        if discount_value == 0 {
            panic!("discount_value must be positive");
        }

        let key = DataKey2::Voucher(merchant.clone(), code_hash.clone());
        if env.storage().persistent().has(&key) {
            panic!("Voucher with this code_hash already exists");
        }

        let voucher = Voucher {
            merchant: merchant.clone(),
            discount_type,
            discount_value,
            max_uses,
            uses_remaining: max_uses,
            expiry,
            revoked: false,
        };

        env.storage().persistent().set(&key, &voucher);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_voucher_issued(
            &env,
            merchant,
            code_hash,
            voucher.discount_type.clone(),
            discount_value,
            max_uses,
            expiry,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant revokes a voucher immediately.
    pub fn revoke_voucher(env: Env, merchant: Address, code_hash: BytesN<32>) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let key = DataKey2::Voucher(merchant.clone(), code_hash.clone());
        let mut voucher: Voucher = env
            .storage()
            .persistent()
            .get(&key)
            .expect("Voucher not found");

        if voucher.merchant != merchant {
            panic!("Only the issuing merchant can revoke this voucher");
        }
        if voucher.revoked {
            panic!("Voucher already revoked");
        }

        voucher.revoked = true;
        env.storage().persistent().set(&key, &voucher);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_voucher_revoked(&env, merchant, code_hash);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get voucher details.
    pub fn get_voucher(env: Env, merchant: Address, code_hash: BytesN<32>) -> Voucher {
        env.storage()
            .persistent()
            .get(&DataKey2::Voucher(merchant, code_hash))
            .expect("Voucher not found")
    }

    /// Create a payment with an optional voucher code hash for discount.
    /// The discount is applied to the required payment amount.
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
    ) -> u32 {
        Self::require_not_paused(&env);
        customer.require_auth();

        // Check idempotency key first
        if let Some(ref key) = idempotency_key {
            if let Some(existing_payment_id) = env
                .storage()
                .temporary()
                .get::<DataKey, u32>(&DataKey::IdempotencyKey(key.clone()))
            {
                return existing_payment_id;
            }
        }

        Self::enforce_rate_limit(&env, &customer, 1);

        if amount <= 0 {
            panic!("Payment amount must be positive");
        }

        Self::validate_reference(&env, &reference);
        Self::validate_metadata(&env, &metadata);
        Self::require_token_allowed(&env, &token);
        Self::require_merchant_approved(&env, &merchant);

        // --- External ID uniqueness check ---
        if let Some(ref ext_id) = external_id {
            let idx_key = DataKey::ExternalIdIndex(merchant.clone(), ext_id.clone());
            if env.storage().persistent().has(&idx_key) {
                panic_with_error!(&env, Error::DuplicateExternalId);
            }
        }

        // --- Voucher discount ---
        let effective_amount = if let Some(ref code_hash) = voucher_code_hash {
            let voucher_key = DataKey2::Voucher(merchant.clone(), code_hash.clone());
            let mut voucher: Voucher = env
                .storage()
                .persistent()
                .get(&voucher_key)
                .expect("Voucher not found");

            let now = env.ledger().timestamp();
            if voucher.revoked {
                panic_with_error!(&env, Error::VoucherRevoked);
            }
            if voucher.expiry > 0 && now > voucher.expiry {
                panic_with_error!(&env, Error::VoucherExpired);
            }
            if voucher.max_uses > 0 && voucher.uses_remaining == 0 {
                panic_with_error!(&env, Error::VoucherExhausted);
            }

            let discount: i128 = match voucher.discount_type {
                DiscountType::Fixed => voucher.discount_value as i128,
                DiscountType::Percentage => (amount * voucher.discount_value as i128) / 100,
            };
            let discounted = (amount - discount).max(0);

            // Decrement uses
            if voucher.max_uses > 0 {
                voucher.uses_remaining -= 1;
            }
            let exhausted = voucher.max_uses > 0 && voucher.uses_remaining == 0;
            env.storage().persistent().set(&voucher_key, &voucher);
            env.storage().persistent().extend_ttl(
                &voucher_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

            events::emit_voucher_redeemed(
                &env,
                merchant.clone(),
                code_hash.clone(),
                customer.clone(),
                discount,
            );
            if exhausted {
                events::emit_voucher_exhausted(&env, merchant.clone(), code_hash.clone());
            }

            discounted
        } else {
            amount
        };

        if effective_amount <= 0 {
            panic!("Effective payment amount after discount must be positive");
        }

        let client = token::Client::new(&env, &token);
        client.transfer(
            &customer,
            &env.current_contract_address(),
            &effective_amount,
        );

        let timeout: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentTimeout)
            .unwrap_or(DEFAULT_PAYMENT_TIMEOUT);
        let now = env.ledger().timestamp();

        // --- Multi-sig gate check ---
        let policy_opt: Option<MultisigPolicy> = env
            .storage()
            .instance()
            .get(&DataKey::MultisigPolicy(merchant.clone()));

        let status = if let Some(ref policy) = policy_opt {
            if effective_amount >= policy.threshold {
                PaymentStatus::PendingApproval
            } else {
                PaymentStatus::Pending
            }
        } else {
            PaymentStatus::Pending
        };

        let payment_id = Self::next_payment_id(&env);
        let payment = Payment {
            id: payment_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            amount: effective_amount,
            token: token.clone(),
            status,
            created_at: now,
            expires_at: now + timeout,
            refunded_amount: 0,
            reference: reference.clone(),
            metadata,
            split_recipients: None,
            execute_after: 0,
            category: None,
            tags: None,
            capture_deadline: 0,
            external_id: external_id.clone(),
            tipping_enabled: false,
            extension_count: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::add_customer_payment(&env, &customer, payment_id);

        if let Some(ref r) = reference {
            Self::index_payment_by_reference(&env, &merchant, r, payment_id);
        }

        // --- Store external_id index ---
        if let Some(ref ext_id) = external_id {
            let idx_key = DataKey::ExternalIdIndex(merchant.clone(), ext_id.clone());
            env.storage().persistent().set(&idx_key, &payment_id);
            env.storage().persistent().extend_ttl(
                &idx_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_payment_indexed_by_external_id(&env, payment_id, ext_id.clone());
        }

        // --- Store approval state if PendingApproval ---
        if status == PaymentStatus::PendingApproval {
            let state = ApprovalState {
                payment_id,
                approvals: Vec::new(&env),
                created_at: now,
            };
            env.storage()
                .persistent()
                .set(&DataKey2::ApprovalState(payment_id), &state);
            env.storage().persistent().extend_ttl(
                &DataKey2::ApprovalState(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }

        if let Some(ref key) = idempotency_key {
            env.storage()
                .temporary()
                .set(&DataKey::IdempotencyKey(key.clone()), &payment_id);
            env.storage().temporary().extend_ttl(
                &DataKey::IdempotencyKey(key.clone()),
                IDEMPOTENCY_KEY_LIFETIME_THRESHOLD,
                IDEMPOTENCY_KEY_BUMP_AMOUNT,
            );
        }

        Self::inc_global_created(&env);
        Self::inc_merchant_created(&env, &merchant);

        events::emit_payment_created(
            &env,
            payment_id,
            customer,
            merchant,
            effective_amount,
            token,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        payment_id
    }

    /// Returns the 32-byte sha256 receipt hash for a completed payment (#65).
    /// Hash inputs (big-endian): payment_id || customer || merchant || amount || token || completed_at
    pub fn get_payment_receipt(env: Env, payment_id: u32) -> BytesN<32> {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentReceipt(payment_id))
            .expect("Receipt not found")
    }

    /// Returns true if the stored receipt hash matches `expected_hash` (#65).
    pub fn verify_payment(env: Env, payment_id: u32, expected_hash: BytesN<32>) -> bool {
        env.storage()
            .persistent()
            .get::<DataKey, BytesN<32>>(&DataKey::PaymentReceipt(payment_id))
            .map(|stored| stored == expected_hash)
            .unwrap_or(false)
    }

    /// Returns the invoice hash for a payment if one was attached (#128).
    pub fn get_invoice_hash(env: Env, payment_id: u32) -> Option<BytesN<32>> {
        env.storage()
            .persistent()
            .get::<DataKey, BytesN<32>>(&DataKey::InvoiceHash(payment_id))
    }

    /// Look up all payment IDs for a merchant+reference pair (#67).
    pub fn get_payments_by_reference(env: Env, merchant: Address, reference: String) -> Vec<u32> {
        let hash = Self::reference_hash(&env, &reference);
        env.storage()
            .persistent()
            .get(&DataKey::MerchantReference(merchant, hash))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns all payment IDs recorded for a customer.
    ///
    /// Backward-compatible single-page read; for unbounded lists use the
    /// paginated form `get_customer_payments_page`.
    pub fn get_customer_payments(env: Env, customer: Address) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::CustomerPayments(customer))
            .unwrap_or(Vec::new(&env))
    }

    /// Paginated read of a customer's payment IDs (#132).
    ///
    /// Returns the requested slice along with `total_count` and `has_more`
    /// so callers can drive UI pagination without an extra round-trip.
    /// `page_size` must be in `1..=MAX_CUSTOMER_PAYMENTS_PAGE_SIZE`.
    pub fn get_customer_payments_page(
        env: Env,
        customer: Address,
        page: u32,
        page_size: u32,
    ) -> CustomerPaymentsPage {
        if page_size == 0 {
            panic!("page_size must be greater than 0");
        }
        if page_size > MAX_CUSTOMER_PAYMENTS_PAGE_SIZE {
            panic!("page_size exceeds maximum of 50");
        }

        let all: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::CustomerPayments(customer))
            .unwrap_or(Vec::new(&env));

        let total_count = all.len();
        let start = page.saturating_mul(page_size);
        let end = start.saturating_add(page_size).min(total_count);

        let mut slice = Vec::new(&env);
        if start < end {
            for i in start..end {
                slice.push_back(all.get(i).unwrap());
            }
        }

        CustomerPaymentsPage {
            payments: slice,
            total_count,
            has_more: end < total_count,
        }
    }

    /// Returns the total number of payment IDs recorded for a customer (#132).
    pub fn get_customer_payment_count(env: Env, customer: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<DataKey, Vec<u32>>(&DataKey::CustomerPayments(customer))
            .map(|v| v.len())
            .unwrap_or(0)
    }

    pub fn get_payment_counter(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::PaymentCounter)
            .unwrap_or(0)
    }

    pub fn get_max_batch_size(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MaxBatchSize)
            .unwrap_or(DEFAULT_MAX_BATCH_SIZE)
    }

    pub fn is_disputed(env: Env, payment_id: u32) -> bool {
        let payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");
        payment.status == PaymentStatus::Disputed
    }

    pub fn get_dispute(env: Env, payment_id: u32) -> Dispute {
        env.storage()
            .temporary()
            .get(&DataKey::Dispute(payment_id))
            .expect("No dispute found for this payment")
    }

    /// Merchant/admin view of all disputes tied to a given merchant (#565).
    /// Only disputes still present in temporary storage are returned (a
    /// resolved dispute is not TTL-extended and will naturally expire and
    /// drop out of this view). `limit` is capped at 50 per call.
    pub fn list_disputes_by_merchant(
        env: Env,
        merchant: Address,
        offset: u32,
        limit: u32,
    ) -> Vec<Dispute> {
        let effective_limit = limit.min(50);
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentCounter)
            .unwrap_or(0);

        let mut result = Vec::new(&env);
        let mut matched: u32 = 0;
        for payment_id in 0..counter {
            let payment: Payment = match env.storage().persistent().get(&DataKey::Payment(payment_id)) {
                Some(p) => p,
                None => continue,
            };
            if payment.merchant != merchant {
                continue;
            }
            let dispute: Dispute = match env.storage().temporary().get(&DataKey::Dispute(payment_id)) {
                Some(d) => d,
                None => continue,
            };
            if matched >= offset && result.len() < effective_limit {
                result.push_back(dispute);
            }
            matched += 1;
        }
        result
    }

    pub fn get_dispute_timeout(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeTimeout)
            .unwrap_or(DEFAULT_DISPUTE_TIMEOUT)
    }

    pub fn get_rate_limit_config(env: Env) -> RateLimitConfig {
        Self::get_rate_limit_config_internal(&env)
    }

    /// #649: Returns the current rate limit status for a customer.
    /// 
    /// Returns (count, window_start_ledger) where:
    /// - count: number of payments made in the current window
    /// - window_start_ledger: ledger sequence when the current window started
    /// 
    /// Returns (0, 0) for a customer with no recorded activity in the current window.
    pub fn get_customer_rate_limit_status(env: Env, customer: Address) -> (u32, u32) {
        let key = DataKey::CustomerRateLimit(customer);
        let cfg = Self::get_rate_limit_config_internal(&env);
        let current_ledger = env.ledger().sequence();

        let state: CustomerRateLimit = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(CustomerRateLimit {
                count: 0,
                window_start_ledger: current_ledger,
            });

        // Check if the window has expired
        if current_ledger.saturating_sub(state.window_start_ledger) >= cfg.window_size_ledgers {
            // Window expired; return reset state
            (0, current_ledger)
        } else {
            // Window still active; return current state
            (state.count, state.window_start_ledger)
        }
    }

    pub fn is_settled(env: Env, payment_id: u32) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Settled(payment_id))
            .unwrap_or(false)
    }

    // --- Payment Expiry (#54) ---

    /// Admin sets the global payment timeout in seconds.
    pub fn set_payment_timeout(env: Env, timeout_seconds: u64) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        if timeout_seconds == 0 {
            panic!("Timeout must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::PaymentTimeout, &timeout_seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn get_payment_timeout(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::PaymentTimeout)
            .unwrap_or(DEFAULT_PAYMENT_TIMEOUT)
    }

    pub fn set_refund_contract(env: Env, admin: Address, refund_contract: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey2::RefundContract, &refund_contract);
    }

    // --- Per-payment expiry bounds (#130) ---

    /// Admin sets the inclusive `[min, max]` bounds for merchant-defined
    /// per-payment expiry overrides (#130). Both values are in seconds.
    pub fn set_payment_expiry_bounds(env: Env, min_seconds: u64, max_seconds: u64) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        if min_seconds == 0 {
            panic!("min_expiry must be greater than 0");
        }
        if min_seconds > max_seconds {
            panic!("min_expiry must be <= max_expiry");
        }
        env.storage()
            .instance()
            .set(&DataKey::MinPaymentExpiry, &min_seconds);
        env.storage()
            .instance()
            .set(&DataKey::MaxPaymentExpiry, &max_seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn get_min_payment_expiry(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MinPaymentExpiry)
            .unwrap_or(DEFAULT_MIN_PAYMENT_EXPIRY)
    }

    pub fn get_max_payment_expiry(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::MaxPaymentExpiry)
            .unwrap_or(DEFAULT_MAX_PAYMENT_EXPIRY)
    }

    fn require_expiry_within_bounds(env: &Env, expiry_seconds: u64) {
        let min_expiry: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinPaymentExpiry)
            .unwrap_or(DEFAULT_MIN_PAYMENT_EXPIRY);
        let max_expiry: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MaxPaymentExpiry)
            .unwrap_or(DEFAULT_MAX_PAYMENT_EXPIRY);
        if expiry_seconds < min_expiry {
            panic!("expiry_seconds is below the configured minimum");
        }
        if expiry_seconds > max_expiry {
            panic!("expiry_seconds exceeds the configured maximum");
        }
    }

    /// Expire a pending or authorized payment after its deadline. Callable by anyone.
    /// Returns funds to the customer and emits PaymentExpired event.
    pub fn expire_payment(env: Env, payment_id: u32) {
        Self::require_not_paused(&env);
        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.status != PaymentStatus::Pending && payment.status != PaymentStatus::Authorized {
            panic!("Only pending or authorized payments can expire");
        }

        let now = env.ledger().timestamp();
        let expired = match payment.status {
            PaymentStatus::Pending => {
                if payment.expires_at == 0 {
                    panic!("Payment has no expiry set");
                }
                now >= payment.expires_at
            }
            PaymentStatus::Authorized => {
                if payment.capture_deadline == 0 {
                    panic!("Authorized payment has no capture deadline");
                }
                (env.ledger().sequence() as u64) > payment.capture_deadline
            }
            _ => false,
        };

        if !expired {
            panic!("Payment has not expired yet");
        }

        let client = token::Client::new(&env, &payment.token);
        client.transfer(
            &env.current_contract_address(),
            &payment.customer,
            &payment.amount,
        );

        let old_status = payment.status;
        payment.status = PaymentStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Update stats (#70)
        Self::inc_global_expired(&env);

        events::emit_payment_expired(
            &env,
            payment_id,
            payment.customer.clone(),
            payment.amount,
            env.ledger().timestamp(),
        );
        events::emit_payment_status_changed(&env, payment_id, old_status, PaymentStatus::Expired);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // --- Partial Refund (#55) ---

    /// Process a partial refund on a disputed payment. Admin only.
    /// `refund_amount` must be <= (payment.amount - payment.refunded_amount).
    pub fn partial_refund(env: Env, payment_id: u32, refund_amount: i128) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        if refund_amount <= 0 {
            panic!("Refund amount must be positive");
        }

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.status != PaymentStatus::Disputed && payment.status != PaymentStatus::Pending {
            panic!("Payment must be pending or disputed for partial refund");
        }

        let remaining = payment.amount - payment.refunded_amount;
        if refund_amount > remaining {
            panic!("Refund amount exceeds remaining balance");
        }

        let client = token::Client::new(&env, &payment.token);
        client.transfer(
            &env.current_contract_address(),
            &payment.customer,
            &refund_amount,
        );

        payment.refunded_amount += refund_amount;

        // If fully refunded, mark as Refunded
        if payment.refunded_amount >= payment.amount {
            payment.status = PaymentStatus::Refunded;
        }

        // Reverse proportional loyalty points on refund (#411)
        Self::reverse_points_for_refund(&env, &payment.customer, refund_amount, payment.amount);

        // Update stats (#70) — count each partial refund call
        Self::inc_global_refunded(&env, &payment.token, refund_amount);
        Self::inc_merchant_refunded(&env, &payment.merchant, &payment.token, refund_amount);

        let remaining = payment.amount - payment.refunded_amount;
        events::emit_payment_partial_refund(
            &env,
            payment_id,
            payment.customer.clone(),
            refund_amount,
            remaining,
        );

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // --- Merchant Allowlist (#58) ---

    /// Admin approves a merchant address.
    /// Requires the merchant to have deposited at least the minimum collateral (#129).
    pub fn approve_merchant(env: Env, merchant: Address) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        // Enforce minimum collateral before approval (#129)
        let min_collateral = Self::get_min_collateral_internal(&env);
        let collateral: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantCollateral(merchant.clone()))
            .unwrap_or(0);
        if collateral < min_collateral {
            panic!("Merchant collateral below minimum required");
        }

        env.storage()
            .persistent()
            .set(&DataKey::MerchantApproved(merchant), &true);
    }

    /// Admin revokes a merchant address.
    pub fn revoke_merchant(env: Env, merchant: Address) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        env.storage()
            .persistent()
            .set(&DataKey::MerchantApproved(merchant), &false);
    }

    /// Check if a merchant is approved.
    pub fn is_merchant_approved(env: Env, merchant: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantApproved(merchant))
            .unwrap_or(false)
    }

    /// Admin toggles open mode (bypasses merchant allowlist).
    pub fn set_merchant_open_mode(env: Env, open: bool) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::MerchantOpenMode, &open);
    }

    pub fn is_merchant_open_mode(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::MerchantOpenMode)
            .unwrap_or(true)
    }

    // --- Merchant Collateral (#129) ---

    /// Admin sets the minimum collateral required for merchant approval.
    pub fn set_min_collateral(env: Env, min_collateral: i128) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        if min_collateral < 0 {
            panic!("min_collateral cannot be negative");
        }

        env.storage()
            .instance()
            .set(&DataKey::MinCollateral, &min_collateral);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the current minimum collateral threshold.
    pub fn get_min_collateral(env: Env) -> i128 {
        Self::get_min_collateral_internal(&env)
    }

    /// Merchant deposits collateral into the contract.
    /// The collateral token is the configured USDC token.
    /// Emits CollateralDeposited event.
    pub fn deposit_collateral(env: Env, merchant: Address, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Deposit amount must be positive");
        }

        let usdc_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::UsdcToken)
            .expect("Collateral token not configured; call set_oracle first");

        Self::require_token_allowed(&env, &usdc_token);

        let token_client = token::Client::new(&env, &usdc_token);
        token_client.transfer(&merchant, &env.current_contract_address(), &amount);

        let key = DataKey::MerchantCollateral(merchant.clone());
        let prev: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_balance = prev.checked_add(amount).expect("Collateral overflow");
        env.storage().persistent().set(&key, &new_balance);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_collateral_deposited(&env, merchant, amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant withdraws collateral from the contract.
    /// Withdrawal is blocked if it would drop the balance below the minimum.
    /// Emits CollateralWithdrawn event.
    pub fn withdraw_collateral(env: Env, merchant: Address, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Withdrawal amount must be positive");
        }

        let key = DataKey::MerchantCollateral(merchant.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);

        if amount > current {
            panic!("Insufficient collateral balance");
        }

        let min_collateral = Self::get_min_collateral_internal(&env);
        let remaining = current - amount;
        if remaining < min_collateral {
            panic!("Withdrawal would drop collateral below minimum required");
        }

        let usdc_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::UsdcToken)
            .expect("Collateral token not configured");

        let token_client = token::Client::new(&env, &usdc_token);
        token_client.transfer(&env.current_contract_address(), &merchant, &amount);

        env.storage().persistent().set(&key, &remaining);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_collateral_withdrawn(&env, merchant, amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the current collateral balance for a merchant.
    pub fn get_collateral_balance(env: Env, merchant: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantCollateral(merchant))
            .unwrap_or(0)
    }

    // --- Notification Keys ---

    /// Merchant registers a notification key for event routing.
    /// The key is included in all payment-related events for this merchant.
    /// Key size is bounded to prevent storage abuse (max 128 bytes).
    pub fn register_notification_key(env: Env, merchant: Address, key: Bytes) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if key.len() > MAX_NOTIFICATION_KEY_LEN {
            panic!("Notification key exceeds maximum length of 128 bytes");
        }

        if key.is_empty() {
            panic!("Notification key cannot be empty");
        }

        let storage_key = DataKey::MerchantNotificationKey(merchant.clone());
        env.storage().persistent().set(&storage_key, &key);
        env.storage().persistent().extend_ttl(
            &storage_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_notification_key_registered(&env, merchant, key);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant removes their notification key.
    /// After removal, events will include empty bytes for the notification key.
    pub fn remove_notification_key(env: Env, merchant: Address) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let storage_key = DataKey::MerchantNotificationKey(merchant.clone());
        env.storage().persistent().remove(&storage_key);

        events::emit_notification_key_removed(&env, merchant);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the notification key for a merchant, if registered.
    pub fn get_notification_key(env: Env, merchant: Address) -> Option<Bytes> {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantNotificationKey(merchant))
    }

    // =========================================================================
    // Notification Key Rotation (#377)
    // =========================================================================

    /// Rotate notification key with overlap window for backward-compatible decryption.
    /// Sets old key's valid_until to now + overlap_window, adds new key with valid_from = now.
    pub fn rotate_notification_key(
        env: Env,
        merchant: Address,
        new_key: Bytes,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if new_key.len() > MAX_NOTIFICATION_KEY_LEN {
            panic!("Notification key exceeds maximum length of 128 bytes");
        }

        if new_key.is_empty() {
            panic!("Notification key cannot be empty");
        }

        let now = env.ledger().timestamp();
        let overlap_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey3::NotificationKeyRotationConfig)
            .unwrap_or(DEFAULT_KEY_OVERLAP_WINDOW_SECONDS);
        let overlap_until = now + overlap_window;

        // Get current key history or create new
        let mut history: soroban_sdk::Vec<NotificationKeyEntry> = env
            .storage()
            .persistent()
            .get(&DataKey3::NotificationKeyHistory(merchant.clone()))
            .unwrap_or(soroban_sdk::Vec::new(&env));

        // Get current active key (if exists)
        let old_key: Option<Bytes> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantNotificationKey(merchant.clone()));

        let old_key_hash = if let Some(ref key) = old_key {
            // Set old key's valid_until
            let old_entry = NotificationKeyEntry {
                key: key.clone(),
                valid_from: 0, // Find actual valid_from from history or use 0
                valid_until: overlap_until,
            };
            history.push_back(old_entry);
            env.crypto().sha256(key).to_bytes()
        } else {
            BytesN::from_array(&env, &[0u8; 32])
        };

        let new_key_hash = env.crypto().sha256(&new_key).to_bytes();

        // Prune old keys if exceeds max history
        while history.len() > MAX_NOTIFICATION_KEY_HISTORY {
            history.remove(0);
        }

        // Store updated history
        env.storage()
            .persistent()
            .set(&DataKey3::NotificationKeyHistory(merchant.clone()), &history);
        env.storage()
            .persistent()
            .extend_ttl(
                &DataKey3::NotificationKeyHistory(merchant.clone()),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

        // Update active key
        env.storage()
            .persistent()
            .set(&DataKey::MerchantNotificationKey(merchant.clone()), &new_key);
        env.storage().persistent().extend_ttl(
            &DataKey::MerchantNotificationKey(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_notification_key_rotated(
            &env,
            merchant,
            old_key_hash,
            new_key_hash,
            overlap_until,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the active notification key (newest key with valid_from <= now).
    pub fn get_active_notification_key(env: Env, merchant: Address) -> Option<Bytes> {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantNotificationKey(merchant))
    }

    /// Get notification key valid at a specific timestamp (historical lookup).
    pub fn get_notification_key_at(
        env: Env,
        merchant: Address,
        timestamp: u64,
    ) -> Option<Bytes> {
        // Check if timestamp is current or future -> return active key
        let now = env.ledger().timestamp();
        if timestamp >= now {
            return env
                .storage()
                .persistent()
                .get(&DataKey::MerchantNotificationKey(merchant.clone()));
        }

        // Look in history for key valid at timestamp
        let history: Option<soroban_sdk::Vec<NotificationKeyEntry>> = env
            .storage()
            .persistent()
            .get(&DataKey3::NotificationKeyHistory(merchant));

        if let Some(hist) = history {
            for entry in hist.iter() {
                if timestamp >= entry.valid_from && timestamp <= entry.valid_until {
                    return Some(entry.key);
                }
            }
        }

        None
    }

    /// Set the notification key rotation overlap window (admin only).
    pub fn set_notification_overlap_window(
        env: Env,
        admin: Address,
        window_seconds: u64,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set overlap window");
        }

        env.storage()
            .instance()
            .set(&DataKey3::NotificationKeyRotationConfig, &window_seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // --- Token Swap Functions ---

    /// Merchant sets their preferred token for receiving payments.
    /// If a customer pays in a different token, the contract will attempt to swap.
    pub fn set_preferred_token(env: Env, merchant: Address, token: Address) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        // Validate token is allowed
        Self::require_token_allowed(&env, &token);

        env.storage()
            .instance()
            .set(&DataKey::PreferredToken(merchant.clone()), &token);

        events::emit_preferred_token_set(&env, merchant, token);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the merchant's preferred token.
    pub fn get_preferred_token(env: Env, merchant: Address) -> Option<Address> {
        env.storage()
            .instance()
            .get(&DataKey::PreferredToken(merchant))
    }

    /// Admin sets the DEX router contract address for token swaps.
    pub fn set_swap_router(env: Env, admin: Address, router: Address) {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set swap router");
        }

        env.storage().instance().set(&DataKey::SwapRouter, &router);

        events::emit_swap_router_set(&env, router);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured swap router address.
    pub fn get_swap_router(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::SwapRouter)
    }

    // --- Token Whitelist Integration ---

    /// Set the token whitelist contract address (admin only)
    pub fn set_token_whitelist_contract(env: Env, admin: Address, whitelist_contract: Address) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set token whitelist contract");
        }

        env.storage()
            .instance()
            .set(&DataKey::TokenWhitelistContract, &whitelist_contract);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the token whitelist contract address
    pub fn get_token_whitelist_contract(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&DataKey::TokenWhitelistContract)
    }

    /// Check if a token is allowed via the whitelist contract.
    /// Delegates to contract-level allowlist first, then global whitelist.
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        if let Some(whitelist_contract) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::TokenWhitelistContract)
        {
            let client = TokenWhitelistClient::new(&env, &whitelist_contract);
            client.is_token_allowed_for_contract(&env.current_contract_address(), &token)
        } else {
            true
        }
    }

    // --- Subscriptions (#60) ---

    /// Subscriber creates a recurring payment. Signs once to authorize future charges.
    pub fn create_subscription(
        env: Env,
        subscriber: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        interval_seconds: u64,
        max_charges: u32,
    ) -> u32 {
        Self::create_subscription_with_trial(
            env,
            subscriber,
            merchant,
            amount,
            token,
            interval_seconds,
            max_charges,
            None,
        )
    }

    /// Subscriber creates a recurring payment with an optional trial period (#133).
    ///
    /// When `trial_period_seconds` is `Some(n)` (n > 0), the first charge is
    /// deferred until `created_at + n`. Charging during the trial returns
    /// `Error::SubscriptionInTrial`. A trial of `0` or `None` behaves like the
    /// historical `create_subscription` (immediate first charge available).
    #[allow(clippy::too_many_arguments)]
    pub fn create_subscription_with_trial(
        env: Env,
        subscriber: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        interval_seconds: u64,
        max_charges: u32,
        trial_period_seconds: Option<u64>,
    ) -> u32 {
        Self::require_not_paused(&env);
        subscriber.require_auth();
        if amount <= 0 {
            panic!("Subscription amount must be positive");
        }
        if interval_seconds == 0 {
            panic!("Interval must be positive");
        }

        Self::require_token_allowed(&env, &token);
        Self::require_merchant_approved(&env, &merchant);

        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::SubscriptionCounter)
            .unwrap_or(0);
        let sub_id = counter;
        counter += 1;
        env.storage()
            .instance()
            .set(&DataKey::SubscriptionCounter, &counter);

        let now = env.ledger().timestamp();
        let trial_seconds = trial_period_seconds.unwrap_or(0);
        let trial_ends_at = if trial_seconds > 0 {
            now + trial_seconds
        } else {
            0
        };

        let sub = Subscription {
            id: sub_id,
            subscriber: subscriber.clone(),
            merchant: merchant.clone(),
            amount,
            token,
            interval_seconds,
            last_charged_at: 0,
            max_charges,
            charges_count: 0,
            active: true,
            paused: false,
            paused_at: 0,
            trial_ends_at,
            pause_authority: PauseAuthority::Subscriber, // Default to subscriber only
            pause_count: 0,
            plan_id: None,
            next_due_ledger: u64::from(env.ledger().sequence()) + Self::seconds_to_ledgers(interval_seconds),
            pending_prorated_credit: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(sub_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(sub_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if trial_ends_at > 0 {
            events::emit_subscription_trial_started(&env, sub_id, trial_ends_at);
        }
        sub_id
    }

    /// Returns the remaining seconds in the subscription's trial period (#133).
    /// Returns 0 if there is no trial or the trial has already ended.
    pub fn get_trial_remaining(env: Env, subscription_id: u32) -> u64 {
        let sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(subscription_id))
            .expect("Subscription not found");
        let now = env.ledger().timestamp();
        if sub.trial_ends_at == 0 || sub.trial_ends_at <= now {
            0
        } else {
            sub.trial_ends_at - now
        }
    }

    /// Charge a subscription. Callable by anyone when the interval has elapsed.
    pub fn charge_subscription(env: Env, subscription_id: u32) {
        Self::require_not_paused(&env);
        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(subscription_id))
            .expect("Subscription not found");

        if !sub.active {
            panic!("Subscription is cancelled");
        }
        if sub.paused {
            panic_with_error!(&env, Error::SubscriptionPaused);
        }
        if sub.max_charges > 0 && sub.charges_count >= sub.max_charges {
            panic!("Max charges reached");
        }

        let now = env.ledger().timestamp();
        if sub.charges_count == 0 && sub.trial_ends_at > now {
            panic_with_error!(&env, Error::SubscriptionInTrial);
        }
        if sub.last_charged_at > 0 && now < sub.last_charged_at + sub.interval_seconds {
            panic!("Interval has not elapsed");
        }
        let trial_just_ended = sub.charges_count == 0 && sub.trial_ends_at > 0;

        let client = token::Client::new(&env, &sub.token);
        let mut charge_amount = sub.amount;
        if sub.pending_prorated_credit > 0 {
            // Apply one-time credit on the first charge after plan change.
            if sub.pending_prorated_credit >= charge_amount {
                charge_amount = 0;
            } else {
                charge_amount -= sub.pending_prorated_credit;
            }
            sub.pending_prorated_credit = 0;
        }
        client.transfer(
            &sub.subscriber,
            &env.current_contract_address(),
            &charge_amount,
        );
        client.transfer(&env.current_contract_address(), &sub.merchant, &charge_amount);

        sub.last_charged_at = now;
        sub.charges_count += 1;
        sub.next_due_ledger = u64::from(env.ledger().sequence()) + Self::seconds_to_ledgers(sub.interval_seconds);

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(subscription_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(subscription_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_charged(
            &env,
            subscription_id,
            sub.subscriber,
            sub.merchant,
            charge_amount,
            now,
        );
        if trial_just_ended {
            events::emit_subscription_trial_ended(&env, subscription_id);
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Cancel a subscription. Subscriber or merchant can cancel.
    pub fn cancel_subscription(env: Env, caller: Address, subscription_id: u32) {
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(subscription_id))
            .expect("Subscription not found");

        if caller != sub.subscriber && caller != sub.merchant {
            panic!("Only subscriber or merchant can cancel");
        }

        sub.active = false;
        env.storage()
            .persistent()
            .set(&DataKey::Subscription(subscription_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(subscription_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_cancelled(&env, subscription_id, caller);
    }

    /// Read a subscription.
    pub fn get_subscription(env: Env, subscription_id: u32) -> Subscription {
        env.storage()
            .persistent()
            .get(&DataKey::Subscription(subscription_id))
            .expect("Subscription not found")
    }

    // --- Subscription Pause / Resume (#124) ---

    /// Subscriber temporarily pauses their subscription.
    /// Charging is blocked while paused. Only the subscriber can pause.
    pub fn pause_subscription(env: Env, subscriber: Address, sub_id: u32) {
        Self::require_not_paused(&env);
        subscriber.require_auth();

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(sub_id))
            .expect("Subscription not found");

        if sub.subscriber != subscriber {
            panic!("Only the subscriber can pause");
        }
        if !sub.active {
            panic!("Subscription is cancelled");
        }
        if sub.paused {
            panic!("Subscription already paused");
        }

        let now = env.ledger().timestamp();
        sub.paused = true;
        sub.paused_at = now;

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(sub_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(sub_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_paused(&env, sub_id, now);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Subscriber resumes a paused subscription.
    /// The next charge interval restarts from the resume timestamp,
    /// so paused time does not count toward the interval.
    pub fn resume_subscription(env: Env, subscriber: Address, sub_id: u32) {
        Self::require_not_paused(&env);
        subscriber.require_auth();

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(sub_id))
            .expect("Subscription not found");

        if sub.subscriber != subscriber {
            panic!("Only the subscriber can resume");
        }
        if !sub.active {
            panic!("Subscription is cancelled");
        }
        if !sub.paused {
            panic!("Subscription is not paused");
        }

        let now = env.ledger().timestamp();
        sub.paused = false;
        sub.paused_at = 0;
        // Reset last_charged_at so the next interval starts from now,
        // ensuring paused duration does not count.
        sub.last_charged_at = now;
        sub.next_due_ledger = u64::from(env.ledger().sequence()) + Self::seconds_to_ledgers(sub.interval_seconds);

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(sub_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(sub_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_resumed(&env, sub_id, now);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// #327: Pause subscription with authority enforcement and pause count tracking.
    pub fn pause_subscription_v2(env: Env, caller: Address, sub_id: u32, reason_hash: BytesN<32>) {
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(sub_id))
            .expect("Subscription not found");

        // Check pause authority
        let is_subscriber = sub.subscriber == caller;
        let is_merchant = sub.merchant == caller;
        match sub.pause_authority {
            PauseAuthority::Subscriber if !is_subscriber => {
                panic_with_error!(&env, Error::UnauthorizedPause)
            }
            PauseAuthority::Merchant if !is_merchant => {
                panic_with_error!(&env, Error::UnauthorizedPause)
            }
            PauseAuthority::Both if !is_subscriber && !is_merchant => {
                panic_with_error!(&env, Error::UnauthorizedPause)
            }
            _ => {}
        }

        if !sub.active || sub.paused {
            panic!("Invalid subscription state");
        }

        // Check pause count limit
        if sub.pause_count >= MAX_SUBSCRIPTION_PAUSES {
            panic_with_error!(&env, Error::PauseCountExceeded);
        }

        sub.paused = true;
        sub.paused_at = env.ledger().sequence() as u64;
        sub.pause_count += 1;

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(sub_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(sub_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_paused_v2(&env, sub_id, caller, env.ledger().sequence());
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// #327: Resume subscription with prorated billing recalculation.
    pub fn resume_subscription_v2(env: Env, caller: Address, sub_id: u32) -> u32 {
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(sub_id))
            .expect("Subscription not found");

        // Only subscriber or merchant can resume
        let is_subscriber = sub.subscriber == caller;
        let is_merchant = sub.merchant == caller;
        if !is_subscriber && !is_merchant {
            panic!("Unauthorized resume");
        }

        if !sub.active || !sub.paused {
            panic!("Invalid subscription state");
        }

        sub.paused = false;
        sub.paused_at = 0;

        // Prorated billing: next_due_ledger = current_ledger + interval_ledgers
        // Convert interval_seconds to approximate ledgers (~5s per ledger)
        let interval_ledgers = ((sub.interval_seconds + 4) / 5) as u32; // Round up
        let next_due_ledger = env.ledger().sequence() + interval_ledgers;
        sub.next_due_ledger = u64::from(next_due_ledger);

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(sub_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(sub_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_resumed_v2(&env, sub_id, caller, next_due_ledger);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        next_due_ledger
    }

    /// #327: Admin force resume a paused subscription.
    pub fn admin_resume_subscription(env: Env, admin: Address, sub_id: u32) -> u32 {
        admin.require_auth();
        Self::require_admin(&env, &admin);

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(sub_id))
            .expect("Subscription not found");

        if !sub.active || !sub.paused {
            panic!("Invalid subscription state");
        }

        sub.paused = false;
        sub.paused_at = 0;

        let interval_ledgers = ((sub.interval_seconds + 4) / 5) as u32;
        let next_due_ledger = env.ledger().sequence() + interval_ledgers;
        sub.next_due_ledger = u64::from(next_due_ledger);

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(sub_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(sub_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_resumed_v2(&env, sub_id, admin, next_due_ledger);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        next_due_ledger
    }

    /// Admin creates or updates a subscription billing plan.
    #[allow(clippy::too_many_arguments)]
    pub fn set_subscription_plan(
        env: Env,
        admin: Address,
        plan_id: u32,
        merchant: Address,
        token: Address,
        amount: i128,
        interval_days: u64,
        active: bool,
    ) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);

        if amount <= 0 {
            panic!("Plan amount must be positive");
        }
        if interval_days == 0 {
            panic!("Plan interval_days must be positive");
        }

        let plan = SubscriptionPlan {
            plan_id,
            merchant,
            token,
            amount,
            interval_days,
            active,
        };

        env.storage()
            .persistent()
            .set(&DataKey3::SubscriptionPlan(plan_id), &plan);
        env.storage().persistent().extend_ttl(
            &DataKey3::SubscriptionPlan(plan_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Read a subscription billing plan by ID.
    pub fn get_subscription_plan(env: Env, plan_id: u32) -> SubscriptionPlan {
        let plan: SubscriptionPlan = env
            .storage()
            .persistent()
            .get(&DataKey3::SubscriptionPlan(plan_id))
            .expect("Subscription plan not found");
        env.storage().persistent().extend_ttl(
            &DataKey3::SubscriptionPlan(plan_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        plan
    }

    /// Subscriber switches to another plan with a one-time prorated credit.
    pub fn change_subscription_plan(env: Env, subscription_id: u32, new_plan_id: u32) {
        Self::require_not_paused(&env);

        let mut sub: Subscription = env
            .storage()
            .persistent()
            .get(&DataKey::Subscription(subscription_id))
            .expect("Subscription not found");
        sub.subscriber.require_auth();

        if !sub.active {
            panic!("Subscription is cancelled");
        }
        if sub.paused {
            panic_with_error!(&env, Error::SubscriptionPaused);
        }

        let new_plan: SubscriptionPlan = env
            .storage()
            .persistent()
            .get(&DataKey3::SubscriptionPlan(new_plan_id))
            .expect("Subscription plan not found");
        if !new_plan.active {
            panic!("Subscription plan inactive");
        }
        if new_plan.merchant != sub.merchant {
            panic!("Plan merchant mismatch");
        }
        if new_plan.token != sub.token {
            panic!("Plan token mismatch");
        }

        let current_ledger = u64::from(env.ledger().sequence());
        let old_plan_id = sub.plan_id.unwrap_or(0);
        let current_plan_amount = sub.amount;
        let current_plan_interval_days = core::cmp::max(1, sub.interval_seconds / 86_400);
        let days_remaining = if sub.next_due_ledger > current_ledger {
            ((sub.next_due_ledger - current_ledger) * LEDGER_CLOSE_TIME_SECONDS) / 86_400
        } else {
            0
        };
        let prorated_credit =
            (current_plan_amount * i128::from(days_remaining)) / i128::from(current_plan_interval_days);

        sub.plan_id = Some(new_plan_id);
        sub.amount = new_plan.amount;
        sub.interval_seconds = new_plan.interval_days * 86_400;
        sub.next_due_ledger = current_ledger + Self::seconds_to_ledgers(sub.interval_seconds);
        sub.pending_prorated_credit = if prorated_credit > 0 { prorated_credit } else { 0 };

        env.storage()
            .persistent()
            .set(&DataKey::Subscription(subscription_id), &sub);
        env.storage().persistent().extend_ttl(
            &DataKey::Subscription(subscription_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().persistent().extend_ttl(
            &DataKey3::SubscriptionPlan(new_plan_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_subscription_plan_changed(
            &env,
            subscription_id,
            old_plan_id,
            new_plan_id,
            sub.pending_prorated_credit,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // --- Payment Categories (#122) ---

    /*
    /// Create a payment with optional category, tags, and release condition.
    /// Category and tags enable on-chain segmentation and analytics.
    /// release_condition, if set, must be satisfied at completion time (#125).
    #[allow(clippy::too_many_arguments)]
    pub fn create_payment_with_extras(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        category: Option<Symbol>,
        tags: Option<Vec<Symbol>>,
        release_condition: Option<OracleCondition>,
    ) -> u32 {
        Self::require_not_paused(&env);
        customer.require_auth();

        if amount <= 0 {
            panic!("Payment amount must be positive");
        }
        if let Some(ref t) = tags {
            if t.len() > MAX_TAGS {
                panic!("Tags list cannot exceed 3 items");
            }
        }

        Self::require_merchant_approved(&env, &merchant);
        Self::enforce_rate_limit(&env, &customer, 1);

        let client = token::Client::new(&env, &token);
        client.transfer(&customer, &env.current_contract_address(), &amount);

        let timeout: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentTimeout)
            .unwrap_or(DEFAULT_PAYMENT_TIMEOUT);
        let now = env.ledger().timestamp();

        let payment_id = Self::next_payment_id(&env);
        let payment = Payment {
            id: payment_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            amount,
            token: token.clone(),
            status: PaymentStatus::Pending,
            created_at: now,
            expires_at: now + timeout,
            refunded_amount: 0,
            reference: None,
            metadata: None,
            split_recipients: None,
            execute_after: 0,
            category: category.clone(),
            tags: tags.clone(),
            capture_deadline: 0,
            release_condition,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::add_customer_payment(&env, &customer, payment_id);

        // Index by category for analytics retrieval
        if let Some(ref cat) = category {
            let cat_key = DataKey::CategoryPayments(merchant.clone(), cat.clone());
            let mut cat_ids: Vec<u32> = env
                .storage()
                .persistent()
                .get(&cat_key)
                .unwrap_or(Vec::new(&env));
            cat_ids.push_back(payment_id);
            env.storage().persistent().set(&cat_key, &cat_ids);
            env.storage().persistent().extend_ttl(
                &cat_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_payment_categorized(
                &env,
                payment_id,
                merchant.clone(),
                cat.clone(),
                tags.clone().unwrap_or(Vec::new(&env)),
            );
        }

        Self::inc_global_created(&env);
        Self::inc_merchant_created(&env, &merchant);
        events::emit_payment_created(&env, payment_id, customer, merchant, amount, token);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        payment_id
    }
    */
    /// Return payment IDs for a merchant + category pair, paginated.
    /// page is 0-indexed. Empty result when page exceeds available data.
    pub fn get_payments_by_category(
        env: Env,
        merchant: Address,
        category: Symbol,
        page: u32,
        page_size: u32,
    ) -> Vec<u32> {
        if page_size == 0 {
            panic!("page_size must be positive");
        }
        let all: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::CategoryPayments(merchant, category))
            .unwrap_or(Vec::new(&env));
        let total = all.len();
        let start = page * page_size;
        if start >= total {
            return Vec::new(&env);
        }
        let end = (start + page_size).min(total);
        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(all.get(i).unwrap());
        }
        result
    }

    /// Expire a batch of payments. Admin only. Cap: 20 IDs.
    /// Already-expired or ineligible IDs are skipped and returned.
    /// Emits PaymentExpired per successfully expired payment and BulkExpireCompleted at end.
    pub fn bulk_expire_payments(env: Env, admin: Address, payment_ids: Vec<u32>) -> Vec<u32> {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can bulk expire payments");
        }

        if payment_ids.len() > 20 {
            panic_with_error!(&env, ExtError::InvalidAmount);
        }

        let now = env.ledger().timestamp();
        let now_ledger = env.ledger().sequence() as u64;
        let mut skipped: Vec<u32> = Vec::new(&env);
        let mut refund_total: i128 = 0;
        let mut expired_count: u32 = 0;

        for payment_id in payment_ids.iter() {
            let entry: Option<Payment> = env.storage().persistent().get(&DataKey::Payment(payment_id));
            let Some(mut payment) = entry else {
                skipped.push_back(payment_id);
                continue;
            };

            let eligible = match payment.status {
                PaymentStatus::Authorized => {
                    payment.capture_deadline > 0 && now_ledger > payment.capture_deadline
                }
                PaymentStatus::Pending | PaymentStatus::Disputed => {
                    payment.expires_at > 0 && now >= payment.expires_at
                }
                _ => false,
            };

            if !eligible {
                skipped.push_back(payment_id);
                continue;
            }

            let refund_amount = payment.amount - payment.refunded_amount;
            if refund_amount > 0 {
                let token_client = token::Client::new(&env, &payment.token);
                token_client.transfer(&env.current_contract_address(), &payment.customer, &refund_amount);
                refund_total = refund_total.checked_add(refund_amount).expect("overflow");
            }

            let old_status = payment.status;
            payment.status = PaymentStatus::Expired;
            env.storage().persistent().set(&DataKey::Payment(payment_id), &payment);
            env.storage().persistent().extend_ttl(
                &DataKey::Payment(payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

            Self::inc_global_expired(&env);
            events::emit_payment_expired(&env, payment_id, payment.customer.clone(), refund_amount, now);
            events::emit_payment_status_changed(&env, payment_id, old_status, PaymentStatus::Expired);
            expired_count += 1;
        }

        events::emit_bulk_expire_completed(&env, expired_count, refund_total);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        skipped
    }

    pub fn pause_contract(env: Env, admin: Address, reason: String) {
        Self::require_admin(&env, &admin);

        if Self::is_paused(env.clone()) {
            panic!("Contract already paused");
        }

        env.storage().instance().set(&DataKey::Paused, &true);
        env.storage().instance().set(&DataKey::PauseReason, &reason);

        events::emit_contract_paused(&env, admin, reason, env.ledger().timestamp());
    }

    pub fn resume_contract(env: Env, admin: Address) {
        Self::require_admin(&env, &admin);

        if !Self::is_paused(env.clone()) {
            panic!("Contract is not paused");
        }

        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().remove(&DataKey::PauseReason);

        events::emit_contract_resumed(&env, admin, env.ledger().timestamp());
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn get_pause_reason(env: Env) -> String {
        env.storage()
            .instance()
            .get(&DataKey::PauseReason)
            .unwrap_or(String::from_str(&env, ""))
    }

    // --- Merchant Withdrawal Queue (#126) ---

    /// Process up to max_count entries from the merchant's withdrawal queue.
    /// Settlement funds were already paid to the merchant when each payment
    /// completed (see `distribute_net_payment`); this just reconciles/clears
    /// the queue entries in FIFO order. Merchant must authorize the call.
    /// Returns the total amount reconciled.
    pub fn process_withdrawal_queue(env: Env, merchant: Address, max_count: u32) -> i128 {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let queue = Self::get_withdrawal_queue(&env, &merchant);
        if queue.is_empty() {
            return 0;
        }

        let max_count = if max_count == 0 {
            DEFAULT_MAX_BATCH_SIZE
        } else {
            max_count
        };
        let process_count = max_count.min(queue.len() as u32) as usize;

        let mut processed_count = 0u32;
        let mut total_withdrawn: i128 = 0;
        let mut remaining_queue = Vec::new(&env);

        // Process the first process_count entries
        for i in 0..process_count {
            let (payment_id, amount) = queue.get(i as u32).unwrap();
            processed_count += 1;
            total_withdrawn += amount;

            let payment: Payment = env
                .storage()
                .persistent()
                .get(&DataKey::Payment(payment_id))
                .expect("Payment not found in queue");
            if payment.status != PaymentStatus::Completed {
                panic!("Payment in queue is not completed");
            }
        }

        // Remove processed entries from queue
        for i in process_count..(queue.len() as usize) {
            remaining_queue.push_back(queue.get(i as u32).unwrap());
        }

        Self::set_withdrawal_queue(&env, &merchant, remaining_queue);
        events::emit_withdrawal_processed(&env, merchant, processed_count);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        total_withdrawn
    }

    /// Move a specific payment to the front of the merchant's withdrawal queue.
    /// Merchant must authorize the call.
    pub fn prioritize_withdrawal(env: Env, merchant: Address, payment_id: u32) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let queue = Self::get_withdrawal_queue(&env, &merchant);
        if queue.is_empty() {
            panic!("Withdrawal queue is empty");
        }

        // Find the payment in the queue
        let mut found_index = None;
        let mut found_amount = 0i128;
        for i in 0..queue.len() {
            let (pid, amount) = queue.get(i).unwrap();
            if pid == payment_id {
                found_index = Some(i);
                found_amount = amount;
                break;
            }
        }

        if found_index.is_none() {
            panic!("Payment not found in withdrawal queue");
        }

        let index = found_index.unwrap();

        // If already at front, do nothing
        if index == 0 {
            return;
        }

        // Remove from current position and insert at front
        let mut new_queue = Vec::new(&env);
        new_queue.push_back((payment_id, found_amount));

        for i in 0..queue.len() {
            if i != index {
                new_queue.push_back(queue.get(i).unwrap());
            }
        }

        Self::set_withdrawal_queue(&env, &merchant, new_queue);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the merchant's current withdrawal queue.
    pub fn get_merchant_withdrawal_queue(env: Env, merchant: Address) -> Vec<(u32, i128)> {
        Self::get_withdrawal_queue(&env, &merchant)
    }

    /// Returns `payment_id`'s current 0-indexed position in the merchant's
    /// withdrawal queue, reflecting any reordering from `prioritize_withdrawal`.
    /// Returns `None` if the payment is not currently queued for this merchant.
    pub fn get_withdrawal_queue_position(env: Env, merchant: Address, payment_id: u32) -> Option<u32> {
        let queue = Self::get_withdrawal_queue(&env, &merchant);
        for i in 0..queue.len() {
            let (pid, _) = queue.get(i).unwrap();
            if pid == payment_id {
                return Some(i);
            }
        }
        None
    }

    // --- Internal Helpers ---

    fn require_not_paused(env: &Env) {
        if env
            .storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract is paused");
        }
    }

    // -------------------------------------------------------------------------
    // #231: Merchant Withdrawal Rate Limiting
    // -------------------------------------------------------------------------

    /// Admin sets global default withdrawal window and cap.
    pub fn set_withdrawal_window(env: Env, admin: Address, window_seconds: u64, cap: i128) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set withdrawal window");
        }
        if window_seconds == 0 {
            panic!("Window must be positive");
        }
        if cap <= 0 {
            panic!("Cap must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey2::WithdrawalWindowSeconds, &window_seconds);
        env.storage()
            .instance()
            .set(&DataKey2::WithdrawalWindowCap, &cap);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant sets a personal (stricter) withdrawal limit.
    pub fn set_withdrawal_limit(env: Env, merchant: Address, window_seconds: u64, cap: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        if window_seconds == 0 {
            panic!("Window must be positive");
        }
        if cap <= 0 {
            panic!("Cap must be positive");
        }
        let limit = WithdrawalLimit {
            window_seconds,
            cap,
        };
        env.storage()
            .persistent()
            .set(&DataKey2::MerchantWithdrawalLimit(merchant.clone()), &limit);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantWithdrawalLimit(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_withdrawal_rate_limit_set(&env, merchant, window_seconds, cap);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin overrides a merchant's withdrawal cap (emergency override).
    pub fn override_withdrawal_limit(env: Env, admin: Address, merchant: Address, cap: i128) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can override withdrawal limit");
        }
        if cap <= 0 {
            panic!("Cap must be positive");
        }
        // Preserve existing window or use global default
        let window_seconds: u64 = env
            .storage()
            .persistent()
            .get::<DataKey2, WithdrawalLimit>(&DataKey2::MerchantWithdrawalLimit(merchant.clone()))
            .map(|l| l.window_seconds)
            .unwrap_or_else(|| {
                env.storage()
                    .instance()
                    .get(&DataKey2::WithdrawalWindowSeconds)
                    .unwrap_or(24 * 60 * 60)
            });
        let limit = WithdrawalLimit {
            window_seconds,
            cap,
        };
        env.storage()
            .persistent()
            .set(&DataKey2::MerchantWithdrawalLimit(merchant.clone()), &limit);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantWithdrawalLimit(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_withdrawal_rate_limit_set(&env, merchant, window_seconds, cap);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn set_default_withdrawal_limits(env: Env, admin: Address, window_seconds: u64, cap: i128) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);

        if window_seconds == 0 {
            panic!("Window must be positive");
        }
        if cap <= 0 {
            panic!("Cap must be positive");
        }

        env.storage()
            .instance()
            .set(&DataKey2::WithdrawalWindowSeconds, &window_seconds);
        env.storage()
            .instance()
            .set(&DataKey2::WithdrawalWindowCap, &cap);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn get_withdrawal_rate_limit(env: Env, merchant: Address) -> (u64, i128) {
        if let Some(limit) = env.storage().persistent().get::<DataKey2, WithdrawalLimit>(
            &DataKey2::MerchantWithdrawalLimit(merchant.clone()),
        ) {
            (limit.window_seconds, limit.cap)
        } else {
            let window_seconds = env.storage().instance().get::<DataKey2, u64>(&DataKey2::WithdrawalWindowSeconds).unwrap_or(86400);
            let cap = env.storage().instance().get::<DataKey2, i128>(&DataKey2::WithdrawalWindowCap).unwrap_or(i128::MAX);
            (window_seconds, cap)
        }
    }

    /// Internal: check and update the rolling withdrawal window for a merchant.
    /// Panics with WithdrawalRateLimitExceeded if the cap would be exceeded.
    fn check_and_update_withdrawal_rate_limit(env: &Env, merchant: &Address, amount: i128) {
        // Determine effective limit: merchant-specific > global default > no limit
        let (window_seconds, cap) =
            if let Some(limit) = env.storage().persistent().get::<DataKey2, WithdrawalLimit>(
                &DataKey2::MerchantWithdrawalLimit(merchant.clone()),
            ) {
                (limit.window_seconds, limit.cap)
            } else if let (Some(w), Some(c)) = (
                env.storage()
                    .instance()
                    .get::<DataKey2, u64>(&DataKey2::WithdrawalWindowSeconds),
                env.storage()
                    .instance()
                    .get::<DataKey2, i128>(&DataKey2::WithdrawalWindowCap),
            ) {
                (w, c)
            } else {
                // No rate limit configured — allow
                return;
            };

        let now = env.ledger().timestamp();
        let state_key = DataKey2::MerchantWithdrawalWindow(merchant.clone());

        let mut state: WithdrawalWindowState = env
            .storage()
            .persistent()
            .get(&state_key)
            .unwrap_or(WithdrawalWindowState {
                window_start: now,
                withdrawn: 0,
            });

        // Reset window if it has elapsed
        if now >= state.window_start + window_seconds {
            state.window_start = now;
            state.withdrawn = 0;
        }

        let new_total = state.withdrawn.checked_add(amount).expect("Overflow");
        if new_total > cap {
            events::emit_withdrawal_rate_limit_exceeded(env, merchant.clone(), new_total, cap);
            panic_with_error!(env, Error::WithdrawalRateLimitExceeded);
        }

        state.withdrawn = new_total;
        env.storage().persistent().set(&state_key, &state);
        env.storage().persistent().extend_ttl(
            &state_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn require_admin(env: &Env, admin: &Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if stored_admin != *admin {
            panic!("Only admin can manage pause state");
        }
    }

    fn seconds_to_ledgers(seconds: u64) -> u64 {
        (seconds + LEDGER_CLOSE_TIME_SECONDS - 1) / LEDGER_CLOSE_TIME_SECONDS
    }

    /// Validates that a token is allowed via the whitelist contract
    fn require_token_allowed(env: &Env, token: &Address) {
        if let Some(whitelist_contract) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::TokenWhitelistContract)
        {
            let client = TokenWhitelistClient::new(env, &whitelist_contract);
            if !client.is_token_allowed_for_contract(&env.current_contract_address(), token) {
                panic_with_error!(env, Error::TokenNotAllowed);
            }
        }
    }

    fn complete_payment_internal(env: &Env, payment_id: u32, scheduled_only: bool) {
        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if scheduled_only {
            if payment.status != PaymentStatus::ScheduledPending {
                panic!("Payment is not scheduled");
            }
            if env.ledger().timestamp() < payment.execute_after {
                panic!("Payment cannot execute before schedule");
            }
        } else if payment.status != PaymentStatus::Pending {
            panic!("Payment is not pending");
        }

        if payment.expires_at > 0 && env.ledger().timestamp() >= payment.expires_at {
            panic!("Payment has expired");
        }

        // #246: Recompute token amount at current oracle rate for dynamic payments
        Self::settle_dynamic_payment_if_needed(env, &mut payment);
        // #235: Check customer spend limit before finalizing
        Self::check_and_update_customer_spend_limit(
            env,
            &payment.merchant,
            &payment.customer,
            payment.amount,
        );

        Self::finalize_payment(env, payment_id, &mut payment);
    }

    /// Merchant authorizes a payment by placing customer funds into escrow immediately.
    /// The payment starts in Authorized status and must be captured on or before
    /// `capture_deadline_ledger`.
    pub fn authorize_payment(
        env: Env,
        merchant: Address,
        customer: Address,
        token: Address,
        amount: i128,
        capture_deadline_ledger: u64,
    ) -> u32 {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Payment amount must be positive");
        }
        if capture_deadline_ledger <= (env.ledger().sequence() as u64) {
            panic!("capture_deadline_ledger must be in the future");
        }

        Self::require_token_allowed(&env, &token);
        Self::require_merchant_approved(&env, &merchant);

        let client = token::Client::new(&env, &token);
        client.transfer_from(
            &env.current_contract_address(),
            &customer,
            &env.current_contract_address(),
            &amount,
        );

        let payment_id = Self::next_payment_id(&env);
        let payment = Payment {
            id: payment_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            amount,
            token: token.clone(),
            status: PaymentStatus::Authorized,
            created_at: env.ledger().timestamp(),
            expires_at: 0,
            refunded_amount: 0,
            reference: None,
            metadata: None,
            split_recipients: None,
            execute_after: 0,
            category: None,
            tags: None,
            capture_deadline: capture_deadline_ledger,
            external_id: None,
            tipping_enabled: false,
            extension_count: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::add_customer_payment(&env, &customer, payment_id);
        Self::inc_global_created(&env);
        Self::inc_merchant_created(&env, &merchant);

        events::emit_payment_authorized(
            &env,
            payment_id,
            customer,
            merchant,
            amount,
            capture_deadline_ledger,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        payment_id
    }

    /// Merchant captures an authorized payment, completing it and releasing funds.
    /// Must be called before capture_deadline.
    pub fn capture_payment(env: Env, merchant: Address, payment_id: u32) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.merchant != merchant {
            panic!("Only the payment merchant can capture");
        }
        if payment.status != PaymentStatus::Authorized {
            panic!("Payment is not authorized");
        }
        if payment.capture_deadline == 0 {
            panic!("Authorized payment has no capture deadline");
        }
        if (env.ledger().sequence() as u64) > payment.capture_deadline {
            panic_with_error!(&env, Error::CapturePastDeadline);
        }

        let captured_amount = payment.amount;
        Self::finalize_payment(&env, payment_id, &mut payment);
        events::emit_payment_captured(&env, payment_id, captured_amount);
    }

    /// Customer voids an authorized payment before capture, releasing escrow back
    /// to the customer and expiring the authorization.
    pub fn void_authorization(env: Env, customer: Address, payment_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if payment.customer != customer {
            panic!("Only the payment customer can void authorization");
        }
        if payment.status != PaymentStatus::Authorized {
            panic!("Only authorized payments can be voided");
        }

        let refund_amount = payment.amount - payment.refunded_amount;
        if refund_amount > 0 {
            let client = token::Client::new(&env, &payment.token);
            client.transfer(
                &env.current_contract_address(),
                &payment.customer,
                &refund_amount,
            );
        }

        let old_status = payment.status;
        payment.status = PaymentStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::inc_global_expired(&env);
        events::emit_payment_expired(
            &env,
            payment_id,
            payment.customer.clone(),
            refund_amount,
            env.ledger().timestamp(),
        );
        events::emit_payment_status_changed(&env, payment_id, old_status, PaymentStatus::Expired);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Execute a token swap via the configured DEX router.
    /// Returns the output amount or an error symbol.
    /// Internal: if a DynamicPayment record exists for this payment, validate expiry.
    /// Full oracle-rate recomputation is deferred to a future implementation.
    fn settle_dynamic_payment_if_needed(env: &Env, payment: &mut Payment) {
        let dyn_opt: Option<DynamicPayment> = env
            .storage()
            .persistent()
            .get(&DataKey2::DynamicPayment(payment.id));
        if let Some(dp) = dyn_opt {
            let now = env.ledger().timestamp();
            if dp.expiry > 0 && now > dp.expiry {
                panic_with_error!(env, Error::DynamicPaymentExpired);
            }

            // Fetch the current price from the currently configured oracle (not the
            // one frozen at creation time), so a stale/replaced price feed is caught.
            let oracle_address: Address = env.storage().instance().get(&DataKey::OracleAddress).expect("Oracle not configured");
            let oracle_client = oracle::OracleClient::new(env, &oracle_address);
            let usdc_token: Address = env.storage().instance().get(&DataKey::UsdcToken).expect("Oracle not configured");
            let current_price_data = oracle_client.lastprice(&dp.token, &usdc_token).expect("Oracle price unavailable");
            let current_price = current_price_data.price;

            // Get merchant-configured slippage tolerance or default to the payment's initial config
            let merchant_slippage: Option<u32> = env.storage().persistent()
                .get(&DataKey3::SlippageToleranceBps(payment.merchant.clone()));
            let slippage_tolerance_bps = merchant_slippage.unwrap_or(dp.slippage_bps);

            // Compute rate deviation
            let rate_deviation = if current_price >= dp.creation_rate {
                current_price - dp.creation_rate
            } else {
                dp.creation_rate - current_price
            };
            
            // Check against tolerance
            let deviation_bps = (rate_deviation * 10_000) / dp.creation_rate;
            if deviation_bps > slippage_tolerance_bps as i128 {
                events::emit_payment_swap_failed(
                    env, 
                    payment.id, 
                    payment.customer.clone(), 
                    dp.token.clone(), 
                    payment.amount, 
                    Symbol::new(env, "StalePrice")
                );
                panic_with_error!(env, Error::SlippageExceeded);
            }

            events::emit_dynamic_payment_settled(env, payment.id, dp.fiat_amount, payment.amount, current_price);
        }
    }

    fn execute_token_swap(
        env: &Env,
        payment_id: u32,
        customer: &Address,
        input_token: &Address,
        output_token: &Address,
        input_amount: i128,
    ) -> Result<i128, Symbol> {
        let router: Address = env
            .storage()
            .instance()
            .get(&DataKey::SwapRouter)
            .expect("Swap router not configured");

        // Get slippage config
        let slippage_cfg = Self::get_slippage_config_internal(env);
        let max_slippage_bps = slippage_cfg.max_bps;

        // For now, we simulate a swap by checking if the router is set
        // In a real implementation, this would call the DEX router contract
        // to execute the swap and return the output amount

        // Placeholder: In production, this would invoke the router contract
        // For now, we return the input amount as if swap happened 1:1
        // This is where you'd integrate with actual DEX contracts like Soroban AMM

        // Simulate swap success with 1:1 rate (placeholder)
        // Real implementation would call router.swap() and handle slippage
        Ok(input_amount)
    }

    /// Shared settlement logic: checks conditions, deducts fees, distributes funds,
    /// updates stats, emits events, and stores receipt. `payment` is mutated in-place.
    fn finalize_payment(env: &Env, payment_id: u32, payment: &mut Payment) {
        // Oracle price condition check (#125)
        /*
        if let Some(ref condition) = payment.release_condition.clone() {
            let oracle_addr: Address = env
                .storage()
                .instance()
                .get(&DataKey::OracleAddress)
                .expect("Oracle not configured for conditional payment");
            let usdc_token: Address = env
                .storage()
                .instance()
                .get(&DataKey::UsdcToken)
                .expect("Oracle not configured for conditional payment");
            let max_oracle_age: u64 = env
                .storage()
                .instance()
                .get(&DataKey::MaxOracleAge)
                .expect("Oracle not configured for conditional payment");

            let oracle_client = oracle::OracleClient::new(env, &oracle_addr);
            let price_data: PriceData = oracle_client
                .lastprice(&condition.asset, &usdc_token)
                .expect("Oracle price unavailable");

            let current_ts = env.ledger().timestamp();
            let age = current_ts.saturating_sub(price_data.timestamp);
            if age > max_oracle_age {
                panic!("Oracle price is stale");
            }

            let met = match condition.direction {
                OracleDirection::Gte => price_data.price >= condition.threshold,
                OracleDirection::Lte => price_data.price <= condition.threshold,
            };

            events::emit_conditional_payment_attempt(
                env,
                payment_id,
                price_data.price,
                condition.threshold,
                met,
            );

            if !met {
                panic_with_error!(env, Error::OracleConditionNotMet);
            }
        }
        */

        // --- Volume cap check (#131) ---
        Self::check_and_update_merchant_volume_cap(
            env,
            payment_id,
            &payment.merchant,
            payment.amount,
        );

        // --- Token Swap: Convert payment token to merchant's preferred token ---
        let mut final_token = payment.token.clone();
        let mut final_amount = payment.amount;

        // Check if swap is needed and possible
        let preferred_token: Option<Address> = env
            .storage()
            .instance()
            .get(&DataKey::PreferredToken(payment.merchant.clone()));

        if let Some(preferred) = preferred_token {
            if payment.token != preferred {
                // Swap is needed
                let router: Option<Address> = env.storage().instance().get(&DataKey::SwapRouter);

                if let Some(router_addr) = router {
                    // Attempt swap
                    let swap_result = Self::execute_token_swap(
                        env,
                        payment_id,
                        &payment.customer,
                        &payment.token,
                        &preferred,
                        payment.amount,
                    );

                    match swap_result {
                        Ok(swapped_amount) => {
                            final_token = preferred.clone();
                            final_amount = swapped_amount;
                            events::emit_payment_swapped_and_settled(
                                env,
                                payment_id,
                                payment.customer.clone(),
                                payment.merchant.clone(),
                                payment.token.clone(),
                                preferred,
                                payment.amount,
                                swapped_amount,
                            );
                        }
                        Err(reason) => {
                            // Swap failed - refund customer
                            let token_client = token::Client::new(env, &payment.token);
                            token_client.transfer(
                                &env.current_contract_address(),
                                &payment.customer,
                                &payment.amount,
                            );

                            let old_status = payment.status;
                            payment.status = PaymentStatus::Refunded;
                            env.storage()
                                .persistent()
                                .set(&DataKey::Payment(payment_id), payment);
                            env.storage().persistent().extend_ttl(
                                &DataKey::Payment(payment_id),
                                PERSISTENT_LIFETIME_THRESHOLD,
                                PERSISTENT_BUMP_AMOUNT,
                            );

                            Self::inc_global_refunded(env, &payment.token, payment.amount);
                            Self::inc_merchant_refunded(
                                env,
                                &payment.merchant,
                                &payment.token,
                                payment.amount,
                            );
                            Self::inc_merchant_failed(env, &payment.merchant); // #226

                            events::emit_payment_swap_failed(
                                env,
                                payment_id,
                                payment.customer.clone(),
                                payment.token.clone(),
                                payment.amount,
                                reason,
                            );
                            events::emit_payment_status_changed(
                                env,
                                payment_id,
                                old_status,
                                PaymentStatus::Refunded,
                            );

                            env.storage()
                                .instance()
                                .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
                            return; // Exit early - payment refunded
                        }
                    }
                } else {
                    // No router configured - cannot swap, use original token
                    final_token = payment.token.clone();
                    final_amount = payment.amount;
                }
            }
        }

        let rolling_before = Self::rolling_merchant_volume(env, &payment.merchant);
        let projected_volume = rolling_before + final_amount;
        let applied_fee_bps = Self::fee_bps_for_volume(env, projected_volume);
        let fee_amount = (final_amount * applied_fee_bps as i128) / 10_000;
        let net_amount = final_amount - fee_amount;

        let fee_recipient: Address = env
            .storage()
            .instance()
            .get(&DataKey::FeeRecipient)
            .expect("Fee recipient not configured");

        let token_client = token::Client::new(env, &final_token);

        if fee_amount > 0 {
            token_client.transfer(&env.current_contract_address(), &fee_recipient, &fee_amount);
            events::emit_fee_collected(
                env,
                payment_id,
                fee_amount,
                fee_recipient,
                final_token.clone(),
            );
            // #242: Accrue referral commission on the fee collected for referred merchants
            Self::accrue_referral_commission(env, &payment.merchant, payment_id, fee_amount);
        }

        let split_transfers = Self::distribute_net_payment(env, payment, net_amount);
        if split_transfers.len() > 0 {
            events::emit_payment_split_completed(env, payment_id, split_transfers);
        }

        let old_status = payment.status;

        // Check if cooling-off is enabled for this payment (#309)
        let cooling_off_config: Option<CoolingOffConfig> = env
            .storage()
            .persistent()
            .get(&DataKey2::CoolingOffConfig(payment_id));

        if let Some(mut config) = cooling_off_config {
            if config.cooling_off_ledgers > 0 {
                // Set status to CoolingOff instead of Completed
                payment.status = PaymentStatus::CoolingOff;
                config.cooling_off_expiry_ledger =
                    env.ledger().sequence() + config.cooling_off_ledgers;
                env.storage()
                    .persistent()
                    .set(&DataKey2::CoolingOffConfig(payment_id), &config);
                env.storage().persistent().extend_ttl(
                    &DataKey2::CoolingOffConfig(payment_id),
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                events::emit_payment_in_cooling_off(
                    env,
                    payment_id,
                    payment.customer.clone(),
                    config.cooling_off_expiry_ledger,
                );
            } else {
                payment.status = PaymentStatus::Completed;
            }
        } else {
            payment.status = PaymentStatus::Completed;
        }
        let original_amount = payment.amount;
        payment.amount = net_amount;
        let completed_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .persistent()
            .set(&DataKey::Settled(payment_id), &true);
        env.storage().persistent().extend_ttl(
            &DataKey::Settled(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        let receipt_hash = Self::compute_receipt_hash(
            env,
            payment_id,
            &payment.customer,
            &payment.merchant,
            net_amount,
            &payment.token,
            completed_at,
        );
        env.storage()
            .persistent()
            .set(&DataKey::PaymentReceipt(payment_id), &receipt_hash);
        env.storage().persistent().extend_ttl(
            &DataKey::PaymentReceipt(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::inc_global_completed(env, &payment.token, net_amount);
        Self::inc_merchant_completed(env, &payment.merchant, &payment.token, net_amount);
        Self::inc_volume_bucket(env, &payment.token, net_amount);
        Self::inc_merchant_volume_bucket(env, &payment.merchant, original_amount);

        // Auto-enqueue completed payment into merchant's withdrawal queue (#126)
        Self::enqueue_withdrawal(env, &payment.merchant, payment_id, net_amount);

        let rolling_after = Self::rolling_merchant_volume(env, &payment.merchant);
        let new_tier_bps = Self::fee_bps_for_volume(env, rolling_after);
        let tier_key = DataKey::MerchantCurrentTierBps(payment.merchant.clone());
        let old_tier_bps: u32 = env
            .storage()
            .persistent()
            .get(&tier_key)
            .unwrap_or(Self::get_fee_bps(env.clone()));
        if old_tier_bps != new_tier_bps {
            env.storage().persistent().set(&tier_key, &new_tier_bps);
            env.storage().persistent().extend_ttl(
                &tier_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_merchant_tier_updated(
                env,
                payment.merchant.clone(),
                new_tier_bps,
                rolling_after,
            );
        }

        events::emit_payment_completed(
            env,
            payment_id,
            payment.merchant.clone(),
            net_amount,
            completed_at,
        );
        events::emit_payment_status_changed(env, payment_id, old_status, PaymentStatus::Completed);
        events::emit_payment_receipt_issued(env, payment_id, receipt_hash);

        // #239: Accrue loyalty points to customer
        Self::accrue_loyalty_points(env, payment_id, &payment.customer, payment.amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn distribute_net_payment(
        env: &Env,
        payment: &Payment,
        net_amount: i128,
    ) -> Vec<SplitTransfer> {
        let token_client = token::Client::new(env, &payment.token);

        if let Some(splits) = &payment.split_recipients {
            if splits.len() == 0 {
                token_client.transfer(
                    &env.current_contract_address(),
                    &payment.merchant,
                    &net_amount,
                );
                return Vec::new(env);
            }

            let mut split_events = Vec::new(env);
            let mut distributed: i128 = 0;
            for split in splits.iter() {
                let share = (net_amount * split.bps as i128) / 10_000;
                if share > 0 {
                    token_client.transfer(
                        &env.current_contract_address(),
                        &split.recipient,
                        &share,
                    );
                }
                distributed += share;
                split_events.push_back(SplitTransfer {
                    recipient: split.recipient,
                    bps: split.bps,
                    amount: share,
                });
            }

            let dust_to_merchant = net_amount - distributed;
            if dust_to_merchant > 0 {
                token_client.transfer(
                    &env.current_contract_address(),
                    &payment.merchant,
                    &dust_to_merchant,
                );
            }
            return split_events;
        }

        token_client.transfer(
            &env.current_contract_address(),
            &payment.merchant,
            &net_amount,
        );
        Vec::new(env)
    }

    fn validate_split_recipients(split_recipients: &Option<Vec<SplitRecipient>>) {
        if let Some(splits) = split_recipients {
            if splits.len() == 0 {
                panic!("split_recipients cannot be empty");
            }

            let mut total_bps: u32 = 0;
            for split in splits.iter() {
                if split.bps == 0 {
                    panic!("split recipient bps must be positive");
                }
                total_bps = total_bps
                    .checked_add(split.bps)
                    .expect("split bps overflow");
            }
            if total_bps != 10_000 {
                panic!("split_recipients must sum to 10000 bps");
            }
        }
    }

    fn validate_fee_tiers(tiers: &Vec<FeeTier>) {
        let mut last_min_volume: i128 = -1;
        for tier in tiers.iter() {
            if tier.fee_bps > MAX_FEE_BPS {
                panic!("tier fee cannot exceed 500 bps (5%)");
            }
            if tier.min_volume < 0 {
                panic!("tier min_volume cannot be negative");
            }
            if tier.min_volume <= last_min_volume {
                panic!("fee tiers must be strictly ascending by min_volume");
            }
            last_min_volume = tier.min_volume;
        }
    }

    /// Validate invoice data and verify total matches payment amount (#128).
    fn validate_invoice_data(env: &Env, invoice: &Option<InvoiceData>, payment_amount: i128) {
        if let Some(inv) = invoice {
            if inv.line_items.len() == 0 {
                panic!("Invoice line_items cannot be empty");
            }
            if (inv.line_items.len() as u32) > MAX_INVOICE_LINE_ITEMS {
                panic!("Invoice line items exceed maximum of 20");
            }

            let mut invoice_subtotal: i128 = 0;
            for item in inv.line_items.iter() {
                if item.quantity == 0 {
                    panic!("Line item quantity must be positive");
                }
                if item.unit_price < 0 {
                    panic!("Line item unit_price cannot be negative");
                }
                let line_amount = (item.quantity as i128)
                    .checked_mul(item.unit_price)
                    .expect("Line item amount overflow");
                invoice_subtotal = invoice_subtotal
                    .checked_add(line_amount)
                    .expect("Invoice subtotal overflow");
            }

            let tax_amount = (invoice_subtotal * inv.tax_bps as i128) / 10_000;
            let invoice_total = invoice_subtotal
                .checked_add(tax_amount)
                .expect("Invoice total overflow");

            if invoice_total != payment_amount {
                panic!("Invoice total does not match payment amount");
            }

            let _ = env; // suppress unused warning
        }
    }

    /// Compute SHA256 hash of serialized invoice data (#128).
    fn compute_invoice_hash(env: &Env, invoice: &InvoiceData) -> BytesN<32> {
        let mut preimage = Bytes::new(env);

        // Serialize line items count
        preimage.extend_from_array(&(invoice.line_items.len() as u32).to_be_bytes());

        // Serialize each line item
        for item in invoice.line_items.iter() {
            // For Symbol, we'll use its XDR representation
            preimage.append(&item.description.to_xdr(env));
            preimage.extend_from_array(&item.quantity.to_be_bytes());
            preimage.extend_from_array(&item.unit_price.to_be_bytes());
        }

        preimage.extend_from_array(&invoice.tax_bps.to_be_bytes());
        preimage.append(&invoice.currency_label.clone().to_xdr(env));

        env.crypto().sha256(&preimage).into()
    }

    fn fee_bps_for_volume(env: &Env, volume: i128) -> u32 {
        let default_fee: u32 = env
            .storage()
            .instance()
            .get(&DataKey::FeeBps)
            .unwrap_or(DEFAULT_FEE_BPS);
        let tiers: Vec<FeeTier> = env
            .storage()
            .instance()
            .get(&DataKey::FeeTiers)
            .unwrap_or(Vec::new(env));

        let mut selected = default_fee;
        for tier in tiers.iter() {
            if volume >= tier.min_volume {
                selected = tier.fee_bps;
            }
        }
        selected
    }

    fn rolling_merchant_volume(env: &Env, merchant: &Address) -> i128 {
        let current_bucket = env.ledger().sequence() / LEDGER_BUCKET_SIZE;
        let mut total = 0i128;
        for i in 0..4u32 {
            let bucket = current_bucket.saturating_sub(i);
            let key = DataKey::MerchantVolumeBucket(merchant.clone(), bucket);
            let bucket_total: i128 = env.storage().persistent().get(&key).unwrap_or(0);
            total += bucket_total;
        }
        total
    }

    fn enforce_rate_limit(env: &Env, customer: &Address, requested_payments: u32) {
        if requested_payments == 0 {
            return;
        }

        let cfg = Self::get_rate_limit_config_internal(env);
        let current_ledger = env.ledger().sequence();
        let key = DataKey::CustomerRateLimit(customer.clone());

        let mut state: CustomerRateLimit =
            env.storage()
                .persistent()
                .get(&key)
                .unwrap_or(CustomerRateLimit {
                    count: 0,
                    window_start_ledger: current_ledger,
                });

        if current_ledger.saturating_sub(state.window_start_ledger) >= cfg.window_size_ledgers {
            state.count = 0;
            state.window_start_ledger = current_ledger;
        }

        let new_count = state.count.saturating_add(requested_payments);
        if new_count > cfg.max_payments {
            panic_with_error!(env, Error::RateLimitExceeded);
        }

        state.count = new_count;
        env.storage().persistent().set(&key, &state);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    /// Validates merchant is approved or open mode is enabled.
    fn require_merchant_approved(env: &Env, merchant: &Address) {
        // Always enforce ban/suspension regardless of open mode
        let status: MerchantStatus = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantStatus(merchant.clone()))
            .unwrap_or(MerchantStatus::Active);

        match status {
            MerchantStatus::Banned => panic!("Merchant is banned"),
            MerchantStatus::Suspended => {
                let expiry: u64 = env
                    .storage()
                    .persistent()
                    .get(&DataKey2::MerchantSuspensionExpiry(merchant.clone()))
                    .unwrap_or(0);
                if expiry == 0 || env.ledger().timestamp() <= expiry {
                    panic!("Merchant is suspended");
                }
                // Suspension expired — fall through to allowlist check
            }
            MerchantStatus::Active => {}
        }

        let open_mode: bool = env
            .storage()
            .instance()
            .get(&DataKey::MerchantOpenMode)
            .unwrap_or(true); // Default: open mode (no allowlist enforcement)
        if open_mode {
            return;
        }

        let approved: bool = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantApproved(merchant.clone()))
            .unwrap_or(false);
        if !approved {
            panic!("Merchant not approved");
        }
    }

    fn require_merchant_reserve(env: &Env, merchant: &Address) {
        if let Some(addr) = env
            .storage()
            .instance()
            .get::<DataKey2, Address>(&DataKey2::RefundContract)
        {
            if !refund_reserve::RefundReserveClient::new(env, &addr)
                .check_merchant_reserve(merchant)
            {
                panic_with_error!(env, Error::InsufficientMerchantReserve);
            }
        }
    }

    /// Validate optional reference string: max 64 bytes (#67).
    fn validate_reference(env: &Env, reference: &Option<String>) {
        if let Some(r) = reference {
            if r.len() > MAX_REFERENCE_LEN {
                panic!("Reference exceeds maximum length of 64 bytes");
            }
            let _ = env; // suppress unused warning
        }
    }

    /// Validate optional metadata map: max 5 keys, each key/value max 32 bytes (#67).
    fn validate_metadata(env: &Env, metadata: &Option<Map<String, String>>) {
        if let Some(m) = metadata {
            if m.len() > MAX_METADATA_KEYS {
                panic!("Metadata exceeds maximum of 5 keys");
            }
            for (k, v) in m.iter() {
                if k.len() > MAX_METADATA_KEY_LEN {
                    panic!("Metadata key exceeds maximum length of 32 bytes");
                }
                if v.len() > MAX_METADATA_KEY_LEN {
                    panic!("Metadata value exceeds maximum length of 32 bytes");
                }
            }
            let _ = env; // suppress unused warning
        }
    }

    /// Compute a simple u32 hash of a reference string for use as a storage key (#67).
    fn reference_hash(_env: &Env, reference: &String) -> u32 {
        let bytes = reference.to_bytes();
        let mut h: u32 = 2166136261u32;
        for b in bytes.iter() {
            h = h.wrapping_mul(16777619).wrapping_add(b as u32);
        }
        h
    }

    /// Append payment_id to the merchant+reference index (#67).
    fn index_payment_by_reference(
        env: &Env,
        merchant: &Address,
        reference: &String,
        payment_id: u32,
    ) {
        let hash = Self::reference_hash(env, reference);
        let key = DataKey::MerchantReference(merchant.clone(), hash);
        let mut ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        ids.push_back(payment_id);
        env.storage().persistent().set(&key, &ids);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    /// Compute sha256(payment_id || customer || merchant || amount || token || completed_at).
    /// All integers encoded big-endian. Addresses encoded as their raw bytes.
    fn compute_receipt_hash(
        env: &Env,
        payment_id: u32,
        customer: &Address,
        merchant: &Address,
        amount: i128,
        token: &Address,
        completed_at: u64,
    ) -> BytesN<32> {
        let mut preimage = Bytes::new(env);
        preimage.extend_from_array(&payment_id.to_be_bytes());
        preimage.append(&customer.to_xdr(env));
        preimage.append(&merchant.to_xdr(env));
        preimage.extend_from_array(&amount.to_be_bytes());
        preimage.append(&token.to_xdr(env));
        preimage.extend_from_array(&completed_at.to_be_bytes());
        env.crypto().sha256(&preimage).into()
    }

    // --- Stats Helpers (#70) ---

    fn load_global_stats(env: &Env) -> GlobalStats {
        env.storage()
            .persistent()
            .get(&DataKey::GlobalStats)
            .unwrap_or(GlobalStats {
                total_payments_created: 0,
                total_payments_completed: 0,
                total_payments_refunded: 0,
                total_payments_expired: 0,
                total_volume_completed: Map::new(env),
                total_volume_refunded: Map::new(env),
            })
    }

    fn save_global_stats(env: &Env, stats: &GlobalStats) {
        env.storage().persistent().set(&DataKey::GlobalStats, stats);
        env.storage().persistent().extend_ttl(
            &DataKey::GlobalStats,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn load_merchant_stats(env: &Env, merchant: &Address) -> MerchantStats {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantStats(merchant.clone()))
            .unwrap_or(MerchantStats {
                payments_created: 0,
                payments_completed: 0,
                payments_refunded: 0,
                volume_completed: Map::new(env),
            })
    }

    fn save_merchant_stats(env: &Env, merchant: &Address, stats: &MerchantStats) {
        let key = DataKey::MerchantStats(merchant.clone());
        env.storage().persistent().set(&key, stats);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn inc_global_created(env: &Env) {
        let mut s = Self::load_global_stats(env);
        s.total_payments_created += 1;
        Self::save_global_stats(env, &s);
    }

    fn inc_global_completed(env: &Env, token: &Address, amount: i128) {
        let mut s = Self::load_global_stats(env);
        s.total_payments_completed += 1;
        let prev = s.total_volume_completed.get(token.clone()).unwrap_or(0);
        s.total_volume_completed.set(token.clone(), prev + amount);
        Self::save_global_stats(env, &s);
    }

    fn inc_global_refunded(env: &Env, token: &Address, amount: i128) {
        let mut s = Self::load_global_stats(env);
        s.total_payments_refunded += 1;
        let prev = s.total_volume_refunded.get(token.clone()).unwrap_or(0);
        s.total_volume_refunded.set(token.clone(), prev + amount);
        Self::save_global_stats(env, &s);
    }

    fn inc_global_expired(env: &Env) {
        let mut s = Self::load_global_stats(env);
        s.total_payments_expired += 1;
        Self::save_global_stats(env, &s);
    }

    fn inc_merchant_created(env: &Env, merchant: &Address) {
        let mut s = Self::load_merchant_stats(env, merchant);
        s.payments_created += 1;
        Self::save_merchant_stats(env, merchant, &s);

        // #226: Track pending count in MerchantSummary
        let key = DataKey2::MerchantSummary(merchant.clone());
        let mut summary: MerchantSummary =
            env.storage()
                .persistent()
                .get(&key)
                .unwrap_or(MerchantSummary {
                    total_volume: 0,
                    completed_count: 0,
                    failed_count: 0,
                    pending_count: 0,
                    volume_by_token: Map::new(env),
                });
        summary.pending_count += 1;
        env.storage().persistent().set(&key, &summary);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn inc_merchant_completed(env: &Env, merchant: &Address, token: &Address, amount: i128) {
        let mut s = Self::load_merchant_stats(env, merchant);
        s.payments_completed += 1;
        let prev = s.volume_completed.get(token.clone()).unwrap_or(0);
        s.volume_completed.set(token.clone(), prev + amount);
        Self::save_merchant_stats(env, merchant, &s);

        // #226: Update MerchantSummary
        let key = DataKey2::MerchantSummary(merchant.clone());
        let mut summary: MerchantSummary =
            env.storage()
                .persistent()
                .get(&key)
                .unwrap_or(MerchantSummary {
                    total_volume: 0,
                    completed_count: 0,
                    failed_count: 0,
                    pending_count: 0,
                    volume_by_token: Map::new(env),
                });
        summary.completed_count += 1;
        if summary.pending_count > 0 {
            summary.pending_count -= 1;
        }
        summary.total_volume += amount;
        let prev_token = summary.volume_by_token.get(token.clone()).unwrap_or(0);
        summary
            .volume_by_token
            .set(token.clone(), prev_token + amount);
        env.storage().persistent().set(&key, &summary);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn inc_merchant_failed(env: &Env, merchant: &Address) {
        let key = DataKey2::MerchantSummary(merchant.clone());
        let mut summary: MerchantSummary =
            env.storage()
                .persistent()
                .get(&key)
                .unwrap_or(MerchantSummary {
                    total_volume: 0,
                    completed_count: 0,
                    failed_count: 0,
                    pending_count: 0,
                    volume_by_token: Map::new(env),
                });
        summary.failed_count += 1;
        if summary.pending_count > 0 {
            summary.pending_count -= 1;
        }
        env.storage().persistent().set(&key, &summary);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn inc_merchant_refunded(env: &Env, merchant: &Address, token: &Address, amount: i128) {
        let mut s = Self::load_merchant_stats(env, merchant);
        s.payments_refunded += 1;
        let _ = (token, amount); // volume tracked globally; merchant count only
        Self::save_merchant_stats(env, merchant, &s);
    }

    fn inc_volume_bucket(env: &Env, token: &Address, amount: i128) {
        let bucket = env.ledger().sequence() / LEDGER_BUCKET_SIZE;
        let key = DataKey::VolumeBucket(token.clone(), bucket);
        let prev: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(prev + amount));
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn inc_merchant_volume_bucket(env: &Env, merchant: &Address, amount: i128) {
        let bucket = env.ledger().sequence() / LEDGER_BUCKET_SIZE;
        let key = DataKey::MerchantVolumeBucket(merchant.clone(), bucket);
        let prev: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(prev + amount));
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn next_payment_id(env: &Env) -> u32 {
        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentCounter)
            .unwrap_or(0);
        let id = counter;
        counter += 1;
        // Counter stays in instance storage — bounded, config-like
        env.storage()
            .instance()
            .set(&DataKey::PaymentCounter, &counter);
        id
    }

    fn add_customer_payment(env: &Env, customer: &Address, payment_id: u32) {
        let key = DataKey::CustomerPayments(customer.clone());
        let mut customer_payments: Vec<u32> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        customer_payments.push_back(payment_id);
        // Persistent: customer index grows with payment volume
        env.storage().persistent().set(&key, &customer_payments);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn get_or_init_version(env: &Env) -> u32 {
        if let Some(version) = env
            .storage()
            .instance()
            .get::<DataKey, u32>(&DataKey::ContractVersion)
        {
            version
        } else {
            let initial_version = 1u32;
            env.storage()
                .instance()
                .set(&DataKey::ContractVersion, &initial_version);
            initial_version
        }
    }

    fn get_rate_limit_config_internal(env: &Env) -> RateLimitConfig {
        env.storage()
            .instance()
            .get(&DataKey::RateLimitConfig)
            .unwrap_or(RateLimitConfig {
                max_payments: DEFAULT_RATE_LIMIT_MAX_PAYMENTS,
                window_size_ledgers: DEFAULT_RATE_LIMIT_WINDOW_SIZE_LEDGERS,
            })
    }

    /// Returns the configured minimum collateral, falling back to the default (#129).
    fn get_min_collateral_internal(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::MinCollateral)
            .unwrap_or(DEFAULT_MIN_COLLATERAL)
    }

    // --- Withdrawal Queue Helpers (#126) ---

    /// Enqueue a completed payment into the merchant's withdrawal queue.
    fn enqueue_withdrawal(env: &Env, merchant: &Address, payment_id: u32, amount: i128) {
        let key = DataKey::WithdrawalQueue(merchant.clone());
        let mut queue: Vec<(u32, i128)> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        queue.push_back((payment_id, amount));
        env.storage().persistent().set(&key, &queue);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_withdrawal_queued(env, merchant.clone(), payment_id, amount);
    }

    /// Get the merchant's withdrawal queue.
    fn get_withdrawal_queue(env: &Env, merchant: &Address) -> Vec<(u32, i128)> {
        let key = DataKey::WithdrawalQueue(merchant.clone());
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env))
    }

    /// Set the merchant's withdrawal queue.
    fn set_withdrawal_queue(env: &Env, merchant: &Address, queue: Vec<(u32, i128)>) {
        let key = DataKey::WithdrawalQueue(merchant.clone());
        env.storage().persistent().set(&key, &queue);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    /// Returns the global slippage config, falling back to defaults (#135).
    fn get_slippage_config_internal(env: &Env) -> SlippageConfig {
        env.storage()
            .instance()
            .get(&DataKey::SlippageConfig)
            .unwrap_or(SlippageConfig {
                default_bps: DEFAULT_SLIPPAGE_BPS,
                min_bps: DEFAULT_MIN_SLIPPAGE_BPS,
                max_bps: DEFAULT_MAX_SLIPPAGE_BPS,
            })
    }

    /// Check merchant volume cap and update window volume. Panics if cap would be exceeded (#131).
    fn check_and_update_merchant_volume_cap(
        env: &Env,
        payment_id: u32,
        merchant: &Address,
        amount: i128,
    ) {
        let cap_key = DataKey::VolumeCap(merchant.clone());
        let cap: VolumeCap = match env.storage().persistent().get(&cap_key) {
            Some(c) => c,
            None => return, // No cap configured — no restriction
        };

        let now = env.ledger().timestamp();
        let bucket = now / cap.window_seconds;
        let vol_key = DataKey::MerchantWindowVolume(merchant.clone(), bucket);
        let current_volume: i128 = env.storage().persistent().get(&vol_key).unwrap_or(0);
        let new_volume = current_volume.checked_add(amount).expect("Volume overflow");

        if new_volume > cap.cap_amount {
            events::emit_volume_capped(
                env,
                merchant.clone(),
                payment_id,
                current_volume,
                cap.cap_amount,
            );
            panic_with_error!(env, Error::MerchantVolumeCapped);
        }

        env.storage().persistent().set(&vol_key, &new_volume);
        env.storage().persistent().extend_ttl(
            &vol_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    // --- Merchant Ban Management & On-Chain Appeal ---

    /// Admin sets the appeal rejection cooldown in seconds.
    pub fn set_appeal_rejection_cooldown(env: Env, admin: Address, seconds: u64) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can configure appeal cooldown");
        }
        env.storage()
            .instance()
            .set(&DataKey2::AppealRejectionCooldownSeconds, &seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin suspends a merchant for a given duration (seconds). Payments are paused.
    pub fn suspend_merchant(
        env: Env,
        admin: Address,
        merchant: Address,
        reason_hash: BytesN<32>,
        duration_seconds: u64,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can suspend merchants");
        }
        if duration_seconds == 0 {
            panic!("Suspension duration must be positive");
        }

        let expiry = env.ledger().timestamp() + duration_seconds;
        env.storage().persistent().set(
            &DataKey2::MerchantStatus(merchant.clone()),
            &MerchantStatus::Suspended,
        );
        env.storage().persistent().set(
            &DataKey2::MerchantSuspensionExpiry(merchant.clone()),
            &expiry,
        );
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantStatus(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantSuspensionExpiry(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_merchant_suspended(&env, merchant, reason_hash, expiry);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin permanently bans a merchant. Banned merchants cannot create or receive payments.
    pub fn ban_merchant(env: Env, admin: Address, merchant: Address, reason_hash: BytesN<32>) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can ban merchants");
        }

        env.storage().persistent().set(
            &DataKey2::MerchantStatus(merchant.clone()),
            &MerchantStatus::Banned,
        );
        env.storage()
            .persistent()
            .set(&DataKey::MerchantApproved(merchant.clone()), &false);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantStatus(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_merchant_banned(&env, merchant, reason_hash);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin reinstates a merchant (clears ban/suspension, restores Active status).
    pub fn reinstate_merchant(env: Env, admin: Address, merchant: Address) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can reinstate merchants");
        }

        env.storage().persistent().set(
            &DataKey2::MerchantStatus(merchant.clone()),
            &MerchantStatus::Active,
        );
        env.storage()
            .persistent()
            .set(&DataKey::MerchantApproved(merchant.clone()), &true);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantStatus(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Banned merchant submits an appeal. Only one active appeal per merchant.
    pub fn submit_appeal(env: Env, merchant: Address, reason_hash: BytesN<32>) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let status: MerchantStatus = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantStatus(merchant.clone()))
            .unwrap_or(MerchantStatus::Active);

        if status != MerchantStatus::Banned {
            panic!("Only banned merchants can submit an appeal");
        }

        // Enforce one-active-appeal guard
        let existing_opt: Option<MerchantAppeal> = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantAppeal(merchant.clone()));
        if let Some(existing) = existing_opt {
            if existing.status == AppealStatus::Pending || existing.status == AppealStatus::ApprovedCoolingOff {
                panic!("An active appeal already exists");
            }
        }

        // Enforce cooldown after rejection
        let cooldown_until: u64 = env
            .storage()
            .persistent()
            .get(&DataKey2::AppealCooldownUntil(merchant.clone()))
            .unwrap_or(0);
        if env.ledger().timestamp() < cooldown_until {
            panic!("Appeal cooldown has not elapsed");
        }

        let appeal = MerchantAppeal {
            merchant: merchant.clone(),
            reason_hash: reason_hash.clone(),
            submitted_at: env.ledger().timestamp(),
            status: AppealStatus::Pending,
            cooling_off_until: 0,
        };

        env.storage()
            .persistent()
            .set(&DataKey2::MerchantAppeal(merchant.clone()), &appeal);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantAppeal(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_appeal_submitted(&env, merchant, reason_hash);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin approves an appeal — merchant enters cooling-off period before full reinstatement.
    pub fn approve_appeal(env: Env, admin: Address, merchant: Address) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can approve appeals");
        }

        let mut appeal: MerchantAppeal = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantAppeal(merchant.clone()))
            .expect("No appeal found");

        if appeal.status != AppealStatus::Pending {
            panic!("Appeal already resolved or not pending");
        }

        // Set cooling-off period (default: 7 days in seconds)
        let cooling_off_period: u64 = env
            .storage()
            .instance()
            .get(&DataKey2::AppealRejectionCooldownSeconds)
            .unwrap_or(7 * 24 * 60 * 60); // Default 7 days
        let cooling_off_until = env.ledger().timestamp() + cooling_off_period;

        appeal.status = AppealStatus::ApprovedCoolingOff;
        appeal.cooling_off_until = cooling_off_until;
        env.storage()
            .persistent()
            .set(&DataKey2::MerchantAppeal(merchant.clone()), &appeal);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantAppeal(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_appeal_approved(&env, merchant, cooling_off_until);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Complete reinstatement after cooling-off period has elapsed. Callable by anyone.
    pub fn complete_reinstatement(env: Env, merchant: Address) {
        Self::require_not_paused(&env);

        let appeal: MerchantAppeal = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantAppeal(merchant.clone()))
            .expect("No appeal found");

        if appeal.status != AppealStatus::ApprovedCoolingOff {
            panic!("Appeal is not in cooling-off period");
        }

        let now = env.ledger().timestamp();
        if now < appeal.cooling_off_until {
            panic!("Cooling-off period has not elapsed");
        }

        // Update appeal status
        let mut updated_appeal = appeal;
        updated_appeal.status = AppealStatus::ApprovedReinstated;
        env.storage()
            .persistent()
            .set(&DataKey2::MerchantAppeal(merchant.clone()), &updated_appeal);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantAppeal(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Reinstate merchant
        env.storage().persistent().set(
            &DataKey2::MerchantStatus(merchant.clone()),
            &MerchantStatus::Active,
        );
        env.storage()
            .persistent()
            .set(&DataKey::MerchantApproved(merchant.clone()), &true);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantStatus(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_merchant_reinstated(&env, merchant, now);
    }

    /// Admin rejects an appeal — merchant remains banned and cooldown is applied.
    pub fn reject_appeal(env: Env, admin: Address, merchant: Address) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can reject appeals");
        }

        let mut appeal: MerchantAppeal = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantAppeal(merchant.clone()))
            .expect("No appeal found");

        if appeal.status != AppealStatus::Pending {
            panic!("Appeal already resolved or not pending");
        }

        appeal.status = AppealStatus::Rejected;
        env.storage()
            .persistent()
            .set(&DataKey2::MerchantAppeal(merchant.clone()), &appeal);

        // Apply cooldown for next appeal
        let cooldown_seconds: u64 = env
            .storage()
            .instance()
            .get(&DataKey2::AppealRejectionCooldownSeconds)
            .unwrap_or(30 * 24 * 60 * 60); // Default 30 days
        let cooldown_until = env.ledger().timestamp() + cooldown_seconds;
        env.storage()
            .persistent()
            .set(&DataKey2::AppealCooldownUntil(merchant.clone()), &cooldown_until);
        env.storage().persistent().extend_ttl(
            &DataKey2::AppealCooldownUntil(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_appeal_rejected(&env, merchant);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the current status of a merchant.
    pub fn get_merchant_status(env: Env, merchant: Address) -> MerchantStatus {
        env.storage()
            .persistent()
            .get(&DataKey2::MerchantStatus(merchant))
            .unwrap_or(MerchantStatus::Active)
    }

    /// Get the active appeal for a merchant, if any.
    pub fn get_merchant_appeal(env: Env, merchant: Address) -> Option<MerchantAppeal> {
        env.storage()
            .persistent()
            .get(&DataKey2::MerchantAppeal(merchant))
    }

    // ─── #226: Merchant Revenue Dashboard ────────────────────────────────────

    /// Returns the full merchant revenue summary.
    pub fn get_merchant_summary(env: Env, merchant: Address) -> MerchantSummary {
        env.storage()
            .persistent()
            .get(&DataKey2::MerchantSummary(merchant))
            .unwrap_or(MerchantSummary {
                total_volume: 0,
                completed_count: 0,
                failed_count: 0,
                pending_count: 0,
                volume_by_token: Map::new(&env),
            })
    }

    /// Returns the completed volume for a specific token for a merchant.
    pub fn get_merchant_volume_by_token(env: Env, merchant: Address, token: Address) -> i128 {
        let summary: MerchantSummary = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantSummary(merchant))
            .unwrap_or(MerchantSummary {
                total_volume: 0,
                completed_count: 0,
                failed_count: 0,
                pending_count: 0,
                volume_by_token: Map::new(&env),
            });
        summary.volume_by_token.get(token).unwrap_or(0)
    }

    /// Returns (completed_count, failed_count, pending_count) for a merchant.
    pub fn get_merchant_payment_counts(env: Env, merchant: Address) -> (u32, u32, u32) {
        let summary: MerchantSummary = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantSummary(merchant))
            .unwrap_or(MerchantSummary {
                total_volume: 0,
                completed_count: 0,
                failed_count: 0,
                pending_count: 0,
                volume_by_token: Map::new(&env),
            });
        (
            summary.completed_count,
            summary.failed_count,
            summary.pending_count,
        )
    }

    /// Admin resets a merchant's dashboard counters.
    pub fn reset_merchant_stats(env: Env, merchant: Address) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let empty = MerchantSummary {
            total_volume: 0,
            completed_count: 0,
            failed_count: 0,
            pending_count: 0,
            volume_by_token: Map::new(&env),
        };
        let key = DataKey2::MerchantSummary(merchant);
        env.storage().persistent().set(&key, &empty);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // ── #239: Customer Loyalty Points ─────────────────────────────────────────

    /// Admin configures the loyalty points system.
    /// points_per_unit: points earned per 1_000_000 units of payment token.
    /// redemption_rate_bps: discount in basis points per 1 point redeemed.
    /// min_payment_floor: minimum payment amount after discount.
    /// expiry_ledgers: ledgers after which unspent points expire (0 = no expiry).
    pub fn configure_loyalty(
        env: Env,
        admin: Address,
        points_per_unit: u32,
        redemption_rate_bps: u32,
        min_payment_floor: i128,
        expiry_ledgers: u32,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin");
        }
        env.storage()
            .instance()
            .set(&DataKey2::LoyaltyPointsPerUnit, &points_per_unit);
        env.storage()
            .instance()
            .set(&DataKey2::LoyaltyRedemptionRateBps, &redemption_rate_bps);
        env.storage()
            .instance()
            .set(&DataKey2::LoyaltyMinPaymentFloor, &min_payment_floor);
        env.storage()
            .instance()
            .set(&DataKey2::LoyaltyExpiryLedgers, &expiry_ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Customer redeems loyalty points as a discount on a pending payment.
    /// Discount = points_to_redeem * redemption_rate_bps / 10_000.
    /// Payment amount cannot drop below min_payment_floor.
    /// Points are non-transferable (tied to customer address).
    pub fn redeem_points(env: Env, customer: Address, payment_id: u32, points_to_redeem: i128) {
        customer.require_auth();

        // Expire stale points first
        Self::maybe_expire_points(&env, &customer);

        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey2::LoyaltyBalance(customer.clone()))
            .unwrap_or(0);
        if points_to_redeem <= 0 || points_to_redeem > balance {
            panic!("Insufficient loyalty points");
        }

        let redemption_rate_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::LoyaltyRedemptionRateBps)
            .unwrap_or(0);
        let min_floor: i128 = env
            .storage()
            .instance()
            .get(&DataKey2::LoyaltyMinPaymentFloor)
            .unwrap_or(0);

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");
        if payment.customer != customer {
            panic!("Not your payment");
        }
        if payment.status != PaymentStatus::Pending {
            panic!("Payment not pending");
        }

        let discount = (points_to_redeem * redemption_rate_bps as i128) / 10_000;
        let new_amount = (payment.amount - discount).max(min_floor);
        let actual_discount = payment.amount - new_amount;
        let actual_points_used = if actual_discount == discount {
            points_to_redeem
        } else {
            // Recalculate points consumed when floor was hit
            if redemption_rate_bps == 0 {
                points_to_redeem
            } else {
                (actual_discount * 10_000) / redemption_rate_bps as i128
            }
        };

        // Burn points
        let new_balance = balance - actual_points_used;
        env.storage()
            .persistent()
            .set(&DataKey2::LoyaltyBalance(customer.clone()), &new_balance);
        env.storage().persistent().extend_ttl(
            &DataKey2::LoyaltyBalance(customer.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Apply discount to payment
        payment.amount = new_amount;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_points_redeemed(
            &env,
            customer,
            payment_id,
            actual_points_used,
            actual_discount,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // =========================================================================
    // #242: Merchant Referral Commission Tracking
    // =========================================================================
    // #235: Merchant-Level Customer Spending Limits
    // =========================================================================

    /// Merchant sets a per-customer spend cap within a rolling window.
    pub fn set_customer_spend_limit(
        env: Env,
        merchant: Address,
        customer: Address,
        amount: i128,
        window_seconds: u64,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        if amount <= 0 {
            panic!("Spend limit amount must be positive");
        }
        if window_seconds == 0 {
            panic!("Window seconds must be positive");
        }
        let key = DataKey2::CustomerSpendLimit(merchant.clone(), customer.clone());
        let limit = SpendLimit {
            amount,
            window_seconds,
        };
        env.storage().persistent().set(&key, &limit);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_customer_spend_limit_set(&env, merchant, customer, amount, window_seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant removes a per-customer spend cap.
    pub fn remove_customer_spend_limit(env: Env, merchant: Address, customer: Address) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        let key = DataKey2::CustomerSpendLimit(merchant.clone(), customer.clone());
        env.storage().persistent().remove(&key);
        events::emit_customer_spend_limit_removed(&env, merchant, customer);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant sets a global default spend cap applied to all customers without an individual override.
    pub fn set_default_spend_limit(env: Env, merchant: Address, amount: i128, window_seconds: u64) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        if amount <= 0 {
            panic!("Spend limit amount must be positive");
        }
        if window_seconds == 0 {
            panic!("Window seconds must be positive");
        }
        let key = DataKey2::DefaultSpendLimit(merchant.clone());
        let limit = SpendLimit {
            amount,
            window_seconds,
        };
        env.storage().persistent().set(&key, &limit);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_default_spend_limit_set(&env, merchant, amount, window_seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the per-customer spend limit for a merchant+customer pair.
    pub fn get_customer_spend_limit(
        env: Env,
        merchant: Address,
        customer: Address,
    ) -> Option<SpendLimit> {
        env.storage()
            .persistent()
            .get(&DataKey2::CustomerSpendLimit(merchant, customer))
    }

    /// Get the default spend limit for a merchant.
    pub fn get_default_spend_limit(env: Env, merchant: Address) -> Option<SpendLimit> {
        env.storage()
            .persistent()
            .get(&DataKey2::DefaultSpendLimit(merchant))
    }

    // ─── #358: Buyer Trust Tier System ───────────────────────────────────────

    /// Merchant assigns a trust tier to a buyer (New/Standard/Trusted/VIP).
    /// Only the merchant may call this.
    pub fn set_buyer_tier(
        env: Env,
        merchant: Address,
        buyer: Address,
        tier: BuyerTrustTierLevel,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        let key = DataKey3::BuyerTrustTier(merchant.clone(), buyer.clone());
        let old_tier: u32 = env
            .storage()
            .persistent()
            .get::<_, BuyerTrustTierLevel>(&key)
            .map(|t| t as u32)
            .unwrap_or(BuyerTrustTierLevel::New as u32);
        env.storage().persistent().set(&key, &tier);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_buyer_tier_updated(&env, merchant, buyer, old_tier, tier as u32);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the trust tier assigned to a buyer by a specific merchant.
    pub fn get_buyer_tier(env: Env, merchant: Address, buyer: Address) -> BuyerTrustTierLevel {
        env.storage()
            .persistent()
            .get(&DataKey3::BuyerTrustTier(merchant, buyer))
            .unwrap_or(BuyerTrustTierLevel::New)
    }

    /// Merchant sets the spending limit for a specific trust tier.
    /// tier_value: 0=New, 1=Standard, 2=Trusted, 3=VIP.
    pub fn set_tier_spending_limit(
        env: Env,
        merchant: Address,
        tier: BuyerTrustTierLevel,
        amount: i128,
        window_seconds: u64,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        if amount <= 0 { panic!("Tier spend limit amount must be positive"); }
        if window_seconds == 0 { panic!("Window seconds must be positive"); }
        let key = DataKey3::TierSpendingLimit(merchant.clone(), tier as u32);
        let limit = SpendLimit { amount, window_seconds };
        env.storage().persistent().set(&key, &limit);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns the spending limit configured for a trust tier by a merchant, if any.
    pub fn get_tier_spending_limit(
        env: Env,
        merchant: Address,
        tier: BuyerTrustTierLevel,
    ) -> Option<SpendLimit> {
        env.storage().persistent().get(&DataKey3::TierSpendingLimit(merchant, tier as u32))
    }

    /// Check and update the customer spend window. Panics with CustomerSpendLimitExceeded if cap would be breached.
    fn check_and_update_customer_spend_limit(
        env: &Env,
        merchant: &Address,
        customer: &Address,
        amount: i128,
    ) {
        // Priority: per-customer override → tier-based limit → global default (#358)
        let limit_opt: Option<SpendLimit> = env
            .storage()
            .persistent()
            .get(&DataKey2::CustomerSpendLimit(merchant.clone(), customer.clone()))
            .or_else(|| {
                // #358: check tier-based limit
                let tier: BuyerTrustTierLevel = env
                    .storage()
                    .persistent()
                    .get(&DataKey3::BuyerTrustTier(merchant.clone(), customer.clone()))
                    .unwrap_or(BuyerTrustTierLevel::New);
                env.storage()
                    .persistent()
                    .get(&DataKey3::TierSpendingLimit(merchant.clone(), tier as u32))
            })
            .or_else(|| {
                env.storage()
                    .persistent()
                    .get(&DataKey2::DefaultSpendLimit(merchant.clone()))
            });

        let limit = match limit_opt {
            Some(l) => l,
            None => return, // No limit configured
        };

        let state_key = DataKey2::CustomerSpendWindowState(merchant.clone(), customer.clone());
        let now = env.ledger().timestamp();

        let mut state: CustomerSpendWindow =
            env.storage()
                .persistent()
                .get(&state_key)
                .unwrap_or(CustomerSpendWindow {
                    window_start: now,
                    spent: 0,
                });

        // Reset window if expired
        if now >= state.window_start + limit.window_seconds {
            state = CustomerSpendWindow {
                window_start: now,
                spent: 0,
            };
        }

        let new_total = state.spent.checked_add(amount).expect("Spend overflow");
        if new_total > limit.amount {
            events::emit_customer_spend_limit_exceeded(
                env,
                merchant.clone(),
                customer.clone(),
                new_total,
                limit.amount,
            );
            panic_with_error!(env, Error::CustomerSpendLimitExceeded);
        }

        state.spent = new_total;
        env.storage().persistent().set(&state_key, &state);
        env.storage().persistent().extend_ttl(
            &state_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    // ─── #216: Recurring Invoice Scheduling ──────────────────────────────────

    /// Merchant creates a recurring invoice schedule.
    /// `max_cycles` = 0 means unlimited.
    pub fn create_recurring_invoice(
        env: Env,
        merchant: Address,
        customer: Address,
        amount: i128,
        token: Address,
        interval_seconds: u64,
        interval_ledgers: u32,
        max_cycles: u32,
        reference_hash: Option<BytesN<32>>,
    ) -> u32 {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }
        if interval_seconds == 0 {
            panic!("Interval must be positive");
        }
        if interval_ledgers == 0 {
            panic!("Interval ledgers must be positive");
        }

        Self::require_token_allowed(&env, &token);
        Self::require_merchant_approved(&env, &merchant);

        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::RecurringInvoiceCounter)
            .unwrap_or(0);
        let invoice_id = counter;
        counter += 1;
        env.storage()
            .instance()
            .set(&DataKey2::RecurringInvoiceCounter, &counter);

        let now_ts = env.ledger().timestamp();
        let now_ledger = env.ledger().sequence();
        let invoice = RecurringInvoice {
            id: invoice_id,
            merchant: merchant.clone(),
            customer: customer.clone(),
            amount,
            token,
            interval_seconds,
            interval_ledgers,
            max_cycles,
            cycles_triggered: 0,
            next_due_at: now_ts,
            next_due_ledger: now_ledger as u64,
            reference_hash,
            active: true,
        };

        env.storage()
            .persistent()
            .set(&DataKey2::RecurringInvoice(invoice_id), &invoice);
        env.storage().persistent().extend_ttl(
            &DataKey2::RecurringInvoice(invoice_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_recurring_invoice_created(&env, invoice_id, merchant, customer, amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        invoice_id
    }

    /// Trigger the next invoice cycle, creating a standard Payment entry.
    /// Callable by anyone (merchant, keeper, etc.) when the interval has elapsed.
    /// Returns the new payment_id.
    pub fn trigger_invoice_cycle(env: Env, invoice_id: u32) -> u32 {
        Self::require_not_paused(&env);

        let mut invoice: RecurringInvoice = env
            .storage()
            .persistent()
            .get(&DataKey2::RecurringInvoice(invoice_id))
            .expect("Recurring invoice not found");

        if !invoice.active {
            panic!("Recurring invoice is cancelled");
        }

        let now_ts = env.ledger().timestamp();
        let now_ledger = env.ledger().sequence() as u64;
        if now_ts < invoice.next_due_at {
            panic!("Invoice interval has not elapsed");
        }
        if now_ledger < invoice.next_due_ledger {
            panic!("Invoice interval has not elapsed (ledger)");
        }

        if invoice.max_cycles > 0 && invoice.cycles_triggered >= invoice.max_cycles {
            panic!("Max cycles reached");
        }

        // Create a standard Payment entry (funds transferred from customer)
        let payment_id = Self::create_payment_with_options(
            env.clone(),
            invoice.customer.clone(),
            invoice.merchant.clone(),
            invoice.amount,
            invoice.token.clone(),
            None,
            None,
            None,
            None,
            None,
        );

        invoice.cycles_triggered += 1;
        invoice.next_due_at = now_ts + invoice.interval_seconds;
        invoice.next_due_ledger = now_ledger + invoice.interval_ledgers as u64;

        // Auto-complete if max_cycles reached
        if invoice.max_cycles > 0 && invoice.cycles_triggered >= invoice.max_cycles {
            invoice.active = false;
            events::emit_recurring_invoice_completed(&env, invoice_id);
        }

        env.storage()
            .persistent()
            .set(&DataKey2::RecurringInvoice(invoice_id), &invoice);
        env.storage().persistent().extend_ttl(
            &DataKey2::RecurringInvoice(invoice_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_invoice_cycle_triggered(
            &env,
            invoice_id,
            payment_id,
            invoice.cycles_triggered,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        payment_id
    }

    /// Returns the loyalty points balance for a customer (after expiry check).
    pub fn get_loyalty_points_balance(env: Env, customer: Address) -> i128 {
        Self::maybe_expire_points(&env, &customer);
        env.storage()
            .persistent()
            .get(&DataKey2::LoyaltyBalance(customer))
            .unwrap_or(0)
    }

    /// Internal: reverse loyalty points proportionally on refund (#411).
    fn reverse_points_for_refund(env: &Env, customer: &Address, refund_amount: i128, original_amount: i128) {
        if original_amount <= 0 || refund_amount <= 0 {
            return;
        }
        let points_per_unit: u32 = match env.storage().instance().get(&DataKey2::LoyaltyPointsPerUnit) {
            Some(v) => v,
            None => return,
        };
        if points_per_unit == 0 {
            return;
        }
        let points_earned = original_amount * points_per_unit as i128 / 1_000_000;
        if points_earned <= 0 {
            return;
        }
        let points_to_reverse = (refund_amount * points_earned) / original_amount;
        if points_to_reverse <= 0 {
            return;
        }
        let balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey2::LoyaltyBalance(customer.clone()))
            .unwrap_or(0);
        let new_balance = (balance - points_to_reverse).max(0);
        let reversed = balance - new_balance;
        env.storage().persistent().set(&DataKey2::LoyaltyBalance(customer.clone()), &new_balance);
        env.storage().persistent().extend_ttl(
            &DataKey2::LoyaltyBalance(customer.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        if reversed > 0 {
            events::emit_points_expired(env, customer.clone(), reversed);
        }
    }

    /// Internal: mint points to customer after a completed payment.
    fn accrue_loyalty_points(env: &Env, payment_id: u32, customer: &Address, payment_amount: i128) {
        let points_per_unit: u32 = match env
            .storage()
            .instance()
            .get(&DataKey2::LoyaltyPointsPerUnit)
        {
            Some(v) => v,
            None => return, // loyalty not configured
        };
        if points_per_unit == 0 {
            return;
        }

        // Expire stale points before accruing
        Self::maybe_expire_points(env, customer);

        let points_earned = payment_amount * points_per_unit as i128 / 1_000_000;
        if points_earned <= 0 {
            return;
        }

        let old_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey2::LoyaltyBalance(customer.clone()))
            .unwrap_or(0);
        let new_balance = old_balance + points_earned;

        env.storage()
            .persistent()
            .set(&DataKey2::LoyaltyBalance(customer.clone()), &new_balance);
        env.storage().persistent().extend_ttl(
            &DataKey2::LoyaltyBalance(customer.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().persistent().set(
            &DataKey2::LoyaltyLastAccrualLedger(customer.clone()),
            &env.ledger().sequence(),
        );
        env.storage().persistent().extend_ttl(
            &DataKey2::LoyaltyLastAccrualLedger(customer.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_points_accrued(
            env,
            customer.clone(),
            payment_id,
            points_earned,
            new_balance,
        );
    }

    /// Cancel a recurring invoice. Callable by merchant or customer.
    pub fn cancel_recurring_invoice(env: Env, caller: Address, invoice_id: u32) {
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut invoice: RecurringInvoice = env
            .storage()
            .persistent()
            .get(&DataKey2::RecurringInvoice(invoice_id))
            .expect("Recurring invoice not found");

        if caller != invoice.merchant && caller != invoice.customer {
            panic!("Only merchant or customer can cancel");
        }

        if !invoice.active {
            panic!("Recurring invoice is already cancelled");
        }

        invoice.active = false;
        env.storage()
            .persistent()
            .set(&DataKey2::RecurringInvoice(invoice_id), &invoice);
        env.storage().persistent().extend_ttl(
            &DataKey2::RecurringInvoice(invoice_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_recurring_invoice_cancelled(&env, invoice_id, caller.clone());

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Internal: burn expired points if expiry window has passed.
    fn maybe_expire_points(env: &Env, customer: &Address) {
        let expiry_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::LoyaltyExpiryLedgers)
            .unwrap_or(0);
        if expiry_ledgers == 0 {
            return;
        }
        let last_accrual: u32 = match env
            .storage()
            .persistent()
            .get(&DataKey2::LoyaltyLastAccrualLedger(customer.clone()))
        {
            Some(v) => v,
            None => return,
        };
        let current = env.ledger().sequence();
        if current >= last_accrual + expiry_ledgers {
            let balance: i128 = env
                .storage()
                .persistent()
                .get(&DataKey2::LoyaltyBalance(customer.clone()))
                .unwrap_or(0);
            if balance > 0 {
                env.storage()
                    .persistent()
                    .set(&DataKey2::LoyaltyBalance(customer.clone()), &0i128);
                events::emit_points_expired(env, customer.clone(), balance);
            }
        }
    }

    /// Get a recurring invoice by ID.
    pub fn get_recurring_invoice(env: Env, invoice_id: u32) -> RecurringInvoice {
        env.storage()
            .persistent()
            .get(&DataKey2::RecurringInvoice(invoice_id))
            .expect("Recurring invoice not found")
    }

    /// Admin sets global referral terms.
    pub fn set_referral_config(env: Env, admin: Address, commission_bps: u32, window_ledgers: u32) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set referral config");
        }
        env.storage()
            .instance()
            .set(&DataKey2::ReferralCommissionBps, &commission_bps);
        env.storage()
            .instance()
            .set(&DataKey2::ReferralWindowLedgers, &window_ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Existing merchant registers a referral for a new merchant.
    /// Fails if the referred address already has a merchant record.
    pub fn register_referral(env: Env, referrer: Address, referred_merchant: Address) {
        Self::require_not_paused(&env);
        referrer.require_auth();

        // Referrer must be an approved merchant
        let referrer_approved: bool = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantApproved(referrer.clone()))
            .unwrap_or(false);
        if !referrer_approved {
            panic!("Referrer is not an approved merchant");
        }

        // Referred must not already have a merchant record
        let already_exists: bool = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantApproved(referred_merchant.clone()))
            .unwrap_or(false);
        if already_exists {
            panic_with_error!(&env, Error::ReferralAlreadyExists);
        }

        let window_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::ReferralWindowLedgers)
            .unwrap_or(0);
        let record = ReferralRecord {
            referrer: referrer.clone(),
            registered_at_ledger: env.ledger().sequence(),
            window_ledgers,
        };
        let key = DataKey2::ReferralRecord(referred_merchant.clone());
        env.storage().persistent().set(&key, &record);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_referral_registered(&env, referrer, referred_merchant);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Referrer withdraws accumulated commission to their address.
    pub fn claim_referral_commission(env: Env, referrer: Address, token: Address) {
        Self::require_not_paused(&env);
        referrer.require_auth();

        let key = DataKey2::PendingCommission(referrer.clone());
        let pending: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if pending <= 0 {
            panic_with_error!(&env, Error::NoCommissionToClaim);
        }

        env.storage().persistent().set(&key, &0i128);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        let claimed_key = DataKey3::TotalClaimedCommission(referrer.clone());
        let total_claimed: i128 = env.storage().persistent().get(&claimed_key).unwrap_or(0);
        let new_total_claimed = total_claimed.saturating_add(pending);
        env.storage().persistent().set(&claimed_key, &new_total_claimed);
        env.storage().persistent().extend_ttl(
            &claimed_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        let token_client = token::Client::new(&env, &token);
        token_client.transfer(&env.current_contract_address(), &referrer, &pending);

        events::emit_commission_claimed(&env, referrer, pending);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get pending commission balance for a referrer.
    pub fn get_pending_commission(env: Env, referrer: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey2::PendingCommission(referrer))
            .unwrap_or(0)
    }

    /// Get the referral record for a referred merchant.
    pub fn get_referral_record(env: Env, referred_merchant: Address) -> Option<ReferralRecord> {
        env.storage()
            .persistent()
            .get(&DataKey2::ReferralRecord(referred_merchant))
    }

    /// Get a referrer's aggregate commission summary: (total_earned, total_claimed).
    /// A referrer with no activity returns (0, 0).
    pub fn get_referral_commission_summary(env: Env, referrer: Address) -> (i128, i128) {
        let total_earned: i128 = env
            .storage()
            .persistent()
            .get(&DataKey3::TotalEarnedCommission(referrer.clone()))
            .unwrap_or(0);
        let total_claimed: i128 = env
            .storage()
            .persistent()
            .get(&DataKey3::TotalClaimedCommission(referrer))
            .unwrap_or(0);
        (total_earned, total_claimed)
    }

    /// Internal: accrue referral commission when a referred merchant's payment is finalized.
    fn accrue_referral_commission(
        env: &Env,
        merchant: &Address,
        payment_id: u32,
        fee_amount: i128,
    ) {
        if fee_amount <= 0 {
            return;
        }

        let record_opt: Option<ReferralRecord> = env
            .storage()
            .persistent()
            .get(&DataKey2::ReferralRecord(merchant.clone()));
        let record = match record_opt {
            Some(r) => r,
            None => return,
        };

        // Check window has not expired
        let current_ledger = env.ledger().sequence();
        if record.window_ledgers > 0
            && current_ledger
                > record
                    .registered_at_ledger
                    .saturating_add(record.window_ledgers)
        {
            return;
        }

        let commission_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::ReferralCommissionBps)
            .unwrap_or(0);
        if commission_bps == 0 {
            return;
        }

        let commission = (fee_amount * commission_bps as i128) / 10_000;
        if commission <= 0 {
            return;
        }

        let pending_key = DataKey2::PendingCommission(record.referrer.clone());
        let current: i128 = env.storage().persistent().get(&pending_key).unwrap_or(0);
        let new_total = current.saturating_add(commission);
        env.storage().persistent().set(&pending_key, &new_total);
        env.storage().persistent().extend_ttl(
            &pending_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        let earned_key = DataKey3::TotalEarnedCommission(record.referrer.clone());
        let total_earned: i128 = env.storage().persistent().get(&earned_key).unwrap_or(0);
        let new_total_earned = total_earned.saturating_add(commission);
        env.storage().persistent().set(&earned_key, &new_total_earned);
        env.storage().persistent().extend_ttl(
            &earned_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_commission_accrued(
            env,
            record.referrer,
            merchant.clone(),
            payment_id,
            commission,
        );
    }

    // ─────────────────────────────────────────────────────────────────
    // #265: Merchant Tip and Gratuity Support
    // ─────────────────────────────────────────────────────────────────

    /// Create a payment with tipping enabled. Identical to `create_payment`
    /// except `tipping_enabled` is stored on the payment record so that
    /// `complete_payment_with_tip` may attach a gratuity at settlement.
    #[allow(clippy::too_many_arguments)]
    pub fn create_payment_with_tipping(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        reference: Option<String>,
        metadata: Option<Map<String, String>>,
        idempotency_key: Option<BytesN<32>>,
    ) -> u32 {
        let payment_id = Self::create_payment_with_expiry(
            env.clone(),
            customer,
            merchant,
            amount,
            token,
            reference,
            metadata,
            None,
            None,
            idempotency_key,
            None,
        );
        // Flip the tipping flag on the freshly-stored payment.
        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");
        payment.tipping_enabled = true;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        payment_id
    }

    /// Admin completes a payment and optionally forwards a customer-provided
    /// gratuity directly to the merchant (fee-exempt).
    ///
    /// * `tip_amount == 0` behaves identically to `complete_payment`.
    /// * `tip_amount > 0` requires `payment.tipping_enabled == true`.
    /// * `tip_amount` must not exceed `max_tip_bps` percent of the base amount.
    /// * The customer must auth this call (to authorise the tip pull).
    pub fn complete_payment_with_tip(
        env: Env,
        payment_id: u32,
        customer: Address,
        tip_amount: i128,
    ) {
        Self::require_not_paused(&env);
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        admin.require_auth();

        let payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        if tip_amount < 0 {
            panic!("tip_amount cannot be negative");
        }

        if tip_amount > 0 {
            if !payment.tipping_enabled {
                panic_with_error!(&env, Error::TippingNotEnabled);
            }
            let max_tip_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey2::MaxTipBps)
                .unwrap_or(DEFAULT_MAX_TIP_BPS);
            let max_tip = (payment.amount * max_tip_bps as i128) / 10_000;
            if tip_amount > max_tip {
                panic_with_error!(&env, Error::TipExceedsMaxBps);
            }
            // Customer must auth the tip transfer.
            customer.require_auth();
            let token_client = token::Client::new(&env, &payment.token);
            
            // Check for tip split configuration (#370)
            let split_config: Option<soroban_sdk::Vec<TipSplitBeneficiary>> = env
                .storage()
                .persistent()
                .get(&DataKey3::TipSplitConfig(payment.merchant.clone()));
            
            match split_config {
                Some(beneficiaries) => {
                    // Multi-beneficiary distribution
                    for beneficiary in beneficiaries.iter() {
                        let allocated_amount = (tip_amount * beneficiary.bps as i128) / 10_000;
                        
                        // Skip zero allocations (rounding edge case)
                        if allocated_amount > 0 {
                            token_client.transfer(
                                &customer,
                                &beneficiary.recipient,
                                &allocated_amount
                            );
                            
                            events::emit_tip_split(
                                &env,
                                payment_id,
                                beneficiary.recipient.clone(),
                                allocated_amount,
                                payment.token.clone(),
                            );
                        }
                    }
                }
                None => {
                    // Fallback: single transfer to merchant (backward compatible)
                    token_client.transfer(&customer, &payment.merchant, &tip_amount);
                    events::emit_tip_received(
                        &env,
                        payment_id,
                        payment.merchant.clone(),
                        tip_amount,
                        payment.token.clone(),
                    );
                }
            }
        }

        Self::complete_payment_internal(&env, payment_id, false);
    }

    /// Admin sets the global maximum tip in basis points.
    /// Must be ≤ 10 000 bps (100%).  Default is 3 000 bps (30%).
    pub fn set_max_tip_bps(env: Env, admin: Address, max_bps: u32) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set max tip bps");
        }
        if max_bps > 10_000 {
            panic!("max_tip_bps cannot exceed 10 000");
        }
        env.storage().instance().set(&DataKey2::MaxTipBps, &max_bps);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin sets KYB verification hash for a merchant (#310)
    pub fn set_merchant_kyb(
        env: Env,
        admin: Address,
        merchant: Address,
        kyb_hash: BytesN<32>,
        expiry_ledger: u64,
        jurisdiction: String,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set merchant KYB");
        }

        let kyb = MerchantKYB {
            kyb_hash: kyb_hash.clone(),
            expiry_ledger,
            jurisdiction: jurisdiction.clone(),
            revoked: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey2::MerchantKYB(merchant.clone()), &kyb);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantKYB(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_merchant_kyb_set(&env, merchant, kyb_hash, expiry_ledger, jurisdiction);
    }

    /// Admin toggles KYB enforcement globally (#310)
    pub fn set_kyb_enforcement(env: Env, admin: Address, enabled: bool) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set KYB enforcement");
        }

        env.storage()
            .instance()
            .set(&DataKey3::KYBRequired, &enabled);
        // Keep the legacy key in sync for backward compatibility across upgrades.
        env.storage()
            .instance()
            .set(&DataKey2::KYBEnforcementEnabled, &enabled);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin renews an existing merchant KYB record with a new hash/expiry (#310)
    pub fn renew_merchant_kyb(
        env: Env,
        admin: Address,
        merchant: Address,
        new_kyb_hash: BytesN<32>,
        new_expiry_ledger: u64,
        jurisdiction: String,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can renew merchant KYB");
        }

        let kyb = MerchantKYB {
            kyb_hash: new_kyb_hash.clone(),
            expiry_ledger: new_expiry_ledger,
            jurisdiction: jurisdiction.clone(),
            revoked: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey2::MerchantKYB(merchant.clone()), &kyb);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantKYB(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_merchant_kyb_set(&env, merchant, new_kyb_hash, new_expiry_ledger, jurisdiction);
    }

    /// Admin revokes a merchant's KYB verification (#310)
    pub fn revoke_merchant_kyb(env: Env, admin: Address, merchant: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can revoke merchant KYB");
        }

        if let Some(mut kyb) = env
            .storage()
            .persistent()
            .get::<_, MerchantKYB>(&DataKey2::MerchantKYB(merchant.clone()))
        {
            kyb.revoked = true;
            env.storage()
                .persistent()
                .set(&DataKey2::MerchantKYB(merchant.clone()), &kyb);
            env.storage().persistent().extend_ttl(
                &DataKey2::MerchantKYB(merchant.clone()),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }

        events::emit_merchant_kyb_revoked(&env, merchant);
    }

    /// Get merchant KYB status (#310)
    pub fn get_merchant_kyb_status(env: Env, merchant: Address) -> KYBStatus {
        let current_ledger = env.ledger().sequence() as u64;

        if let Some(kyb) = env
            .storage()
            .persistent()
            .get::<_, MerchantKYB>(&DataKey2::MerchantKYB(merchant.clone()))
        {
            env.storage().persistent().extend_ttl(
                &DataKey2::MerchantKYB(merchant),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

            let verified = !kyb.revoked && current_ledger <= kyb.expiry_ledger;
            KYBStatus {
                verified,
                expiry_ledger: kyb.expiry_ledger,
                jurisdiction: kyb.jurisdiction,
            }
        } else {
            KYBStatus {
                verified: false,
                expiry_ledger: 0,
                jurisdiction: String::from_slice(&env, ""),
            }
        }
    }

    /// Check if KYB enforcement is enabled (#310)
    pub fn is_kyb_enforcement_enabled(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey3::KYBRequired)
            .or_else(|| {
                env.storage()
                    .instance()
                    .get::<_, bool>(&DataKey2::KYBEnforcementEnabled)
            })
            .unwrap_or(false)
    }
    /// Returns the current maximum tip in basis points (default: 3 000).
    pub fn get_max_tip_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey2::MaxTipBps)
            .unwrap_or(DEFAULT_MAX_TIP_BPS)
    }

    // =========================================================================
    // Tip Split Configuration (#370)
    // =========================================================================

    /// Configure tip split allocation for a merchant.
    /// Validates that basis points sum to exactly 10,000 and beneficiary count <= MAX_TIP_SPLIT_BENEFICIARIES.
    pub fn set_tip_split_config(
        env: Env,
        merchant: Address,
        caller: Address,
        split_map: soroban_sdk::Vec<(Address, u32)>,
    ) {
        caller.require_auth();
        
        // Validate caller is merchant (in production, add admin override if needed)
        if caller != merchant {
            panic!("Only merchant can configure tip splits");
        }
        
        // Validate non-empty
        if split_map.is_empty() {
            panic!("Tip split configuration cannot be empty");
        }
        
        // Validate max beneficiaries
        if split_map.len() > MAX_TIP_SPLIT_BENEFICIARIES {
            panic!("Tip split beneficiaries exceed maximum allowed");
        }
        
        // Validate total bps sum to 10,000
        let mut total_bps: u32 = 0;
        for entry in split_map.iter() {
            total_bps = total_bps.checked_add(entry.1).expect("BPS overflow");
        }
        if total_bps != 10_000 {
            panic!("Tip split basis points must sum to exactly 10,000");
        }
        
        // Convert to Vec<TipSplitBeneficiary> and store
        let mut beneficiaries: soroban_sdk::Vec<TipSplitBeneficiary> = soroban_sdk::Vec::new(&env);
        for entry in split_map.iter() {
            beneficiaries.push_back(TipSplitBeneficiary {
                recipient: entry.0,
                bps: entry.1,
            });
        }
        
        env.storage()
            .persistent()
            .set(&DataKey3::TipSplitConfig(merchant.clone()), &beneficiaries);
        env.storage()
            .persistent()
            .extend_ttl(
                &DataKey3::TipSplitConfig(merchant),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
    }

    /// Retrieve tip split configuration for a merchant.
    /// Returns None if no configuration exists (fallback to default behavior).
    pub fn get_tip_split_config(
        env: Env,
        merchant: Address,
    ) -> Option<soroban_sdk::Vec<(Address, u32)>> {
        let config: Option<soroban_sdk::Vec<TipSplitBeneficiary>> = env
            .storage()
            .persistent()
            .get(&DataKey3::TipSplitConfig(merchant));
        
        config.map(|beneficiaries| {
            let mut result: soroban_sdk::Vec<(Address, u32)> = soroban_sdk::Vec::new(&env);
            for b in beneficiaries.iter() {
                result.push_back((b.recipient, b.bps));
            }
            result
        })
    }

    /// Remove tip split configuration for a merchant.
    /// Reverts to default behavior (100% to merchant).
    pub fn remove_tip_split_config(
        env: Env,
        merchant: Address,
        caller: Address,
    ) {
        caller.require_auth();
        
        // Validate caller is merchant (in production, add admin override if needed)
        if caller != merchant {
            panic!("Only merchant can remove tip split configuration");
        }
        
        env.storage()
            .persistent()
            .remove(&DataKey3::TipSplitConfig(merchant));
    }

    // =========================================================================
    // Consent Records (#307): Zero-Amount Agreement Signing
    // =========================================================================

    /// Create a consent record for a customer to sign terms without token transfer.
    /// Merchant calls this to create a record keyed by (merchant, customer, terms_version).
    /// Returns the consent_id.
    pub fn create_consent_record(
        env: Env,
        merchant: Address,
        customer: Address,
        terms_hash: BytesN<32>,
        terms_version: String,
        expiry_ledger: u64,
    ) -> u32 {
        merchant.require_auth();
        Self::require_not_paused(&env);

        // Validate inputs
        if terms_version.len() == 0 || terms_version.len() > 64 {
            panic!("terms_version must be 1-64 bytes");
        }
        if expiry_ledger <= env.ledger().sequence() as u64 {
            panic!("expiry_ledger must be in the future");
        }

        // Get next consent ID
        let consent_id = Self::next_consent_id(&env);

        // Create consent record
        let now = env.ledger().timestamp();
        let consent = ConsentRecord {
            id: consent_id,
            merchant: merchant.clone(),
            customer: customer.clone(),
            terms_hash: terms_hash.clone(),
            terms_version: terms_version.clone(),
            created_at: now,
            expires_at: expiry_ledger,
            is_signed: false,
            signed_at: 0,
            is_revoked: false,
        };

        // Store consent record
        env.storage()
            .persistent()
            .set(&DataKey2::ConsentRecord(consent_id), &consent);

        // Index by (merchant, customer, terms_version)
        env.storage().persistent().set(
            &DataKey2::ConsentIndex(merchant.clone(), customer.clone(), terms_version.clone()),
            &consent_id,
        );

        // Extend TTL
        env.storage().persistent().extend_ttl(
            &DataKey2::ConsentRecord(consent_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().persistent().extend_ttl(
            &DataKey2::ConsentIndex(merchant.clone(), customer.clone(), terms_version.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_consent_record_created(
            &env,
            consent_id,
            merchant,
            customer,
            terms_hash,
            terms_version,
        );
        consent_id
    }
    // ── #329: Failed Auto-Debit Retry Queue ───────────────────────────────────

    /// Admin configures the retry back-off schedule.
    /// `base_retry_interval`: ledgers before first retry (default 100).
    /// `max_retry_interval`: back-off cap in ledgers (default 3 200).
    /// `max_retry_attempts`: maximum retry attempts before abandonment (default 5).
    pub fn set_retry_config(
        env: Env,
        admin: Address,
        base_retry_interval: u64,
        max_retry_interval: u64,
        max_retry_attempts: u32,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set retry config");
        }
        if base_retry_interval == 0 {
            panic!("base_retry_interval must be positive");
        }
        if max_retry_interval < base_retry_interval {
            panic!("max_retry_interval must be >= base_retry_interval");
        }
        if max_retry_attempts == 0 {
            panic!("max_retry_attempts must be at least 1");
        }
        let cfg = RetryConfig {
            base_retry_interval,
            max_retry_interval,
            max_retry_attempts,
        };
        env.storage().persistent().set(&DataKey2::RetryConfig, &cfg);
        env.storage().persistent().extend_ttl(
            &DataKey2::RetryConfig,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant initiates a pull-payment from a pre-approved customer allowance.
    /// If the customer's balance is sufficient the payment settles immediately.
    /// On insufficient balance the contract stores a `FailedDebitRecord` and
    /// emits `DebitFailed` instead of reverting, returning the new record ID.
    pub fn initiate_allowed_payment(
        env: Env,
        merchant: Address,
        customer: Address,
        token: Address,
        amount: i128,
        plan_id: u32,
        invoice_id: Option<u32>,
    ) -> u32 {
        Self::require_not_paused(&env);
        merchant.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        Self::require_token_allowed(&env, &token);
        Self::require_merchant_approved(&env, &merchant);

        let cfg: RetryConfig = env
            .storage()
            .persistent()
            .get(&DataKey2::RetryConfig)
            .unwrap_or(RetryConfig {
                base_retry_interval: DEFAULT_BASE_RETRY_INTERVAL,
                max_retry_interval: DEFAULT_MAX_RETRY_INTERVAL,
                max_retry_attempts: DEFAULT_MAX_RETRY_ATTEMPTS,
            });

        let client = token::Client::new(&env, &token);
        let result = client.try_transfer_from(
            &env.current_contract_address(),
            &customer,
            &merchant,
            &amount,
        );

        let record_id: u32 = env
            .storage()
            .persistent()
            .get(&DataKey2::FailedDebitCounter)
            .unwrap_or(0u32)
            + 1;
        env.storage()
            .persistent()
            .set(&DataKey2::FailedDebitCounter, &record_id);
        env.storage().persistent().extend_ttl(
            &DataKey2::FailedDebitCounter,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        if result.is_ok() {
            let rec = FailedDebitRecord {
                id: record_id,
                plan_id,
                invoice_id,
                merchant: merchant.clone(),
                customer: customer.clone(),
                token: token.clone(),
                amount,
                attempt_number: 1,
                next_retry_ledger: 0,
                status: FailedDebitStatus::Succeeded,
                created_at: env.ledger().timestamp(),
            };
            env.storage()
                .persistent()
                .set(&DataKey2::FailedDebit(record_id), &rec);
        } else {
            let next_retry_ledger = (env.ledger().sequence() as u64) + cfg.base_retry_interval;
            let rec = FailedDebitRecord {
                id: record_id,
                plan_id,
                invoice_id,
                merchant: merchant.clone(),
                customer: customer.clone(),
                token: token.clone(),
                amount,
                attempt_number: 1,
                next_retry_ledger,
                status: FailedDebitStatus::Pending,
                created_at: env.ledger().timestamp(),
            };
            env.storage()
                .persistent()
                .set(&DataKey2::FailedDebit(record_id), &rec);
            events::emit_debit_failed(&env, record_id, plan_id, 1, next_retry_ledger);
        }
        env.storage().persistent().extend_ttl(
            &DataKey2::FailedDebit(record_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        record_id
    }

    /// Customer signs a consent record, authenticating their acceptance.
    /// No token transfer occurs. Consent record must not be expired or revoked.
    pub fn sign_consent(env: Env, customer: Address, consent_id: u32) {
        customer.require_auth();
        Self::require_not_paused(&env);

        // Load consent record
        let mut consent: ConsentRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::ConsentRecord(consent_id))
            .expect("Consent record not found");

        // Validate consent state
        if consent.is_revoked {
            panic!("Consent record has been revoked");
        }
        if consent.is_signed {
            panic!("Consent record already signed");
        }
        if consent.expires_at <= env.ledger().sequence() as u64 {
            panic!("Consent record has expired");
        }
        if consent.customer != customer {
            panic!("Only the customer can sign this consent");
        }

        // Mark as signed
        consent.is_signed = true;
        consent.signed_at = env.ledger().timestamp();

        // Store updated consent
        env.storage()
            .persistent()
            .set(&DataKey2::ConsentRecord(consent_id), &consent);
        env.storage().persistent().extend_ttl(
            &DataKey2::ConsentRecord(consent_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Emit event
        events::emit_consent_signed(&env, consent_id, customer, consent.signed_at);
    }

    /// Check if a consent record is valid (signed, not expired, not revoked).
    pub fn is_consent_valid(
        env: Env,
        merchant: Address,
        customer: Address,
        terms_version: String,
    ) -> bool {
        // Try to get consent ID from index
        let consent_id_opt: Option<u32> = env.storage().persistent().get(&DataKey2::ConsentIndex(
            merchant,
            customer,
            terms_version,
        ));

        if let Some(consent_id) = consent_id_opt {
            if let Some(consent) = env
                .storage()
                .persistent()
                .get::<_, ConsentRecord>(&DataKey2::ConsentRecord(consent_id))
            {
                // Extend TTL on read
                env.storage().persistent().extend_ttl(
                    &DataKey2::ConsentRecord(consent_id),
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );

                // Check validity: signed, not expired, not revoked
                return consent.is_signed
                    && !consent.is_revoked
                    && consent.expires_at > env.ledger().sequence() as u64;
            }
        }

        false
    }

    /// Revoke a consent record (e.g., on account closure).
    /// Only the merchant or admin can revoke.
    pub fn revoke_consent(env: Env, caller: Address, consent_id: u32) {
        caller.require_auth();
        Self::require_not_paused(&env);

        // Load consent record
        let mut consent: ConsentRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::ConsentRecord(consent_id))
            .expect("Consent record not found");

        // Validate caller is merchant or admin
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if caller != consent.merchant && caller != admin {
            panic!("Only merchant or admin can revoke consent");
        }

        // Mark as revoked
        consent.is_revoked = true;

        // Store updated consent
        env.storage()
            .persistent()
            .set(&DataKey2::ConsentRecord(consent_id), &consent);
        env.storage().persistent().extend_ttl(
            &DataKey2::ConsentRecord(consent_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Emit event
        events::emit_consent_revoked(&env, consent_id, caller, env.ledger().timestamp());
    }

    /// Get a consent record by ID.
    pub fn get_consent_record(env: Env, consent_id: u32) -> ConsentRecord {
        let consent: ConsentRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::ConsentRecord(consent_id))
            .expect("Consent record not found");

        env.storage().persistent().extend_ttl(
            &DataKey2::ConsentRecord(consent_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        consent
    }

    /// Get consent ID by (merchant, customer, terms_version) triple.
    pub fn get_consent_id(
        env: Env,
        merchant: Address,
        customer: Address,
        terms_version: String,
    ) -> Option<u32> {
        env.storage()
            .persistent()
            .get(&DataKey2::ConsentIndex(merchant, customer, terms_version))
    }

    fn next_consent_id(env: &Env) -> u32 {
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::ConsentRecordCounter)
            .unwrap_or(0);
        let next_id = counter.checked_add(1).expect("consent ID overflow");
        env.storage()
            .instance()
            .set(&DataKey2::ConsentRecordCounter, &next_id);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        next_id
    }

    /// Returns all currently-active consent records for `customer`.
    /// Records are excluded when revoked, expired, or not yet signed.
    pub fn list_active_consent_records(env: Env, customer: Address) -> Vec<ConsentRecord> {
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::ConsentRecordCounter)
            .unwrap_or(0);
        let now = env.ledger().sequence() as u64;
        let mut result = Vec::new(&env);
        for consent_id in 0..counter {
            let consent: ConsentRecord = match env
                .storage()
                .persistent()
                .get(&DataKey2::ConsentRecord(consent_id))
            {
                Some(c) => c,
                None => continue,
            };
            if consent.customer != customer {
                continue;
            }
            if consent.is_revoked || !consent.is_signed || consent.expires_at <= now {
                continue;
            }
            result.push_back(consent);
        }
        result
    }

    /// Retry a failed debit. Enforces the exponential back-off schedule.
    /// Returns `RetryNotDue` if the back-off interval has not elapsed.
    /// On max attempts the record is moved to `AbandonedDebit` status.
    pub fn retry_failed_debit(env: Env, record_id: u32) {
        Self::require_not_paused(&env);

        let mut rec: FailedDebitRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::FailedDebit(record_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::DebitRecordNotFound));

        if rec.status == FailedDebitStatus::Abandoned {
            panic_with_error!(&env, Error::DebitAlreadyAbandoned);
        }
        if rec.status == FailedDebitStatus::Succeeded {
            panic_with_error!(&env, Error::DebitAlreadySucceeded);
        }

        let current_ledger = env.ledger().sequence() as u64;
        if current_ledger < rec.next_retry_ledger {
            panic_with_error!(&env, Error::RetryNotDue);
        }

        let cfg: RetryConfig = env
            .storage()
            .persistent()
            .get(&DataKey2::RetryConfig)
            .unwrap_or(RetryConfig {
                base_retry_interval: DEFAULT_BASE_RETRY_INTERVAL,
                max_retry_interval: DEFAULT_MAX_RETRY_INTERVAL,
                max_retry_attempts: DEFAULT_MAX_RETRY_ATTEMPTS,
            });

        let client = token::Client::new(&env, &rec.token);
        let result = client.try_transfer_from(
            &env.current_contract_address(),
            &rec.customer,
            &rec.merchant,
            &rec.amount,
        );

        if result.is_ok() {
            rec.status = FailedDebitStatus::Succeeded;
            env.storage()
                .persistent()
                .set(&DataKey2::FailedDebit(record_id), &rec);
            env.storage().persistent().extend_ttl(
                &DataKey2::FailedDebit(record_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_debit_retry_succeeded(&env, record_id, rec.plan_id, rec.amount);

            // If this debit is linked to a recurring invoice, advance the invoice cycle
            if let Some(invoice_id) = rec.invoice_id {
                Self::advance_recurring_invoice_on_retry_success(&env, invoice_id, record_id);
            }
        } else {
            rec.attempt_number += 1;
            if rec.attempt_number > cfg.max_retry_attempts {
                rec.status = FailedDebitStatus::Abandoned;
                env.storage()
                    .persistent()
                    .set(&DataKey2::FailedDebit(record_id), &rec);
                env.storage().persistent().extend_ttl(
                    &DataKey2::FailedDebit(record_id),
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                events::emit_debit_abandoned(&env, record_id, rec.plan_id);
            } else {
                // Double the back-off, capped at max_retry_interval
                let raw_interval = cfg.base_retry_interval.saturating_mul(
                    1u64.checked_shl((rec.attempt_number - 1).min(63) as u32)
                        .unwrap_or(u64::MAX),
                );
                let interval = raw_interval.min(cfg.max_retry_interval);
                rec.next_retry_ledger = current_ledger + interval;
                env.storage()
                    .persistent()
                    .set(&DataKey2::FailedDebit(record_id), &rec);
                env.storage().persistent().extend_ttl(
                    &DataKey2::FailedDebit(record_id),
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                events::emit_debit_failed(
                    &env,
                    record_id,
                    rec.plan_id,
                    rec.attempt_number,
                    rec.next_retry_ledger,
                );
            }
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Internal helper to advance a recurring invoice when a linked debit retry succeeds.
    fn advance_recurring_invoice_on_retry_success(env: &Env, invoice_id: u32, payment_id: u32) {
        let mut invoice: RecurringInvoice = env
            .storage()
            .persistent()
            .get(&DataKey2::RecurringInvoice(invoice_id))
            .expect("Recurring invoice not found");

        if !invoice.active {
            return;
        }

        invoice.cycles_triggered += 1;
        invoice.next_due_at = invoice.next_due_at.saturating_add(invoice.interval_seconds);
        invoice.next_due_ledger = invoice.next_due_ledger.saturating_add(invoice.interval_ledgers as u64);

        if invoice.max_cycles > 0 && invoice.cycles_triggered >= invoice.max_cycles {
            invoice.active = false;
            events::emit_recurring_invoice_completed(env, invoice_id);
        }

        env.storage()
            .persistent()
            .set(&DataKey2::RecurringInvoice(invoice_id), &invoice);
        env.storage().persistent().extend_ttl(
            &DataKey2::RecurringInvoice(invoice_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_invoice_cycle_triggered(env, invoice_id, payment_id, invoice.cycles_triggered);
    }

    /// Customer triggers an early retry regardless of the back-off schedule,
    /// e.g. after topping up their wallet.
    pub fn trigger_early_retry(env: Env, customer: Address, record_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut rec: FailedDebitRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::FailedDebit(record_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::DebitRecordNotFound));

        if rec.customer != customer {
            panic!("Only the debited customer may trigger an early retry");
        }
        if rec.status == FailedDebitStatus::Abandoned {
            panic_with_error!(&env, Error::DebitAlreadyAbandoned);
        }
        if rec.status == FailedDebitStatus::Succeeded {
            panic_with_error!(&env, Error::DebitAlreadySucceeded);
        }

        let cfg: RetryConfig = env
            .storage()
            .persistent()
            .get(&DataKey2::RetryConfig)
            .unwrap_or(RetryConfig {
                base_retry_interval: DEFAULT_BASE_RETRY_INTERVAL,
                max_retry_interval: DEFAULT_MAX_RETRY_INTERVAL,
                max_retry_attempts: DEFAULT_MAX_RETRY_ATTEMPTS,
            });

        let client = token::Client::new(&env, &rec.token);
        let result = client.try_transfer_from(
            &env.current_contract_address(),
            &rec.customer,
            &rec.merchant,
            &rec.amount,
        );

        if result.is_ok() {
            rec.status = FailedDebitStatus::Succeeded;
            env.storage()
                .persistent()
                .set(&DataKey2::FailedDebit(record_id), &rec);
            env.storage().persistent().extend_ttl(
                &DataKey2::FailedDebit(record_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_debit_retry_succeeded(&env, record_id, rec.plan_id, rec.amount);
            if let Some(invoice_id) = rec.invoice_id {
                Self::advance_recurring_invoice_on_retry_success(&env, invoice_id, record_id);
            }
        } else {
            rec.attempt_number += 1;
            if rec.attempt_number > cfg.max_retry_attempts {
                rec.status = FailedDebitStatus::Abandoned;
                env.storage()
                    .persistent()
                    .set(&DataKey2::FailedDebit(record_id), &rec);
                env.storage().persistent().extend_ttl(
                    &DataKey2::FailedDebit(record_id),
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                events::emit_debit_abandoned(&env, record_id, rec.plan_id);
            } else {
                let raw_interval = cfg.base_retry_interval.saturating_mul(
                    1u64.checked_shl((rec.attempt_number - 1).min(63) as u32)
                        .unwrap_or(u64::MAX),
                );
                let interval = raw_interval.min(cfg.max_retry_interval);
                rec.next_retry_ledger = (env.ledger().sequence() as u64) + interval;
                env.storage()
                    .persistent()
                    .set(&DataKey2::FailedDebit(record_id), &rec);
                env.storage().persistent().extend_ttl(
                    &DataKey2::FailedDebit(record_id),
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                events::emit_debit_failed(
                    &env,
                    record_id,
                    rec.plan_id,
                    rec.attempt_number,
                    rec.next_retry_ledger,
                );
            }
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Read a failed debit record by ID.
    pub fn get_failed_debit(env: Env, record_id: u32) -> FailedDebitRecord {
        env.storage()
            .persistent()
            .get(&DataKey2::FailedDebit(record_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::DebitRecordNotFound))
    }

    /// Read the current retry configuration.
    pub fn get_retry_config(env: Env) -> RetryConfig {
        env.storage()
            .persistent()
            .get(&DataKey2::RetryConfig)
            .unwrap_or(RetryConfig {
                base_retry_interval: DEFAULT_BASE_RETRY_INTERVAL,
                max_retry_interval: DEFAULT_MAX_RETRY_INTERVAL,
                max_retry_attempts: DEFAULT_MAX_RETRY_ATTEMPTS,
            })
    }

    // ── #351: Recurring Payment Scheduler ────────────────────────────────────

    /// Create a recurring payment schedule.
    /// The payer must separately call `token::approve` on the token contract to grant
    /// this contract the spending allowance for `amount * max_cycles` (or the
    /// pre-approved spending module equivalently) before each execution.
    /// Returns the schedule_id.
    pub fn create_recurring_payment(
        env: Env,
        payer: Address,
        payee: Address,
        token: Address,
        amount: i128,
        interval_seconds: u64,
        max_cycles: u32,
    ) -> u32 {
        Self::require_not_paused(&env);
        payer.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }
        if interval_seconds == 0 {
            panic!("Interval must be positive");
        }

        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey3::RecurringCounter)
            .unwrap_or(0);
        counter += 1;
        env.storage()
            .instance()
            .set(&DataKey3::RecurringCounter, &counter);

        let schedule_id = counter;
        let now = env.ledger().timestamp();

        let schedule = RecurringPaymentSchedule {
            schedule_id,
            payer: payer.clone(),
            payee: payee.clone(),
            token: token.clone(),
            amount,
            interval_seconds,
            max_cycles,
            cycles_executed: 0,
            next_due: now,
            active: true,
        };

        env.storage()
            .persistent()
            .set(&DataKey3::RecurringSchedule(schedule_id), &schedule);
        env.storage().persistent().extend_ttl(
            &DataKey3::RecurringSchedule(schedule_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        schedule_id
    }

    /// Execute a recurring payment schedule.
    /// Callable by anyone once `ledger_timestamp >= next_due`.
    /// Transfers `amount` from payer to payee via a pre-approved token allowance.
    /// Auto-cancels the schedule when `max_cycles` is reached.
    pub fn execute_recurring(env: Env, schedule_id: u32) {
        Self::require_not_paused(&env);

        let mut schedule: RecurringPaymentSchedule = env
            .storage()
            .persistent()
            .get(&DataKey3::RecurringSchedule(schedule_id))
            .expect("Recurring schedule not found");

        if !schedule.active {
            panic!("Recurring schedule is not active");
        }

        let now = env.ledger().timestamp();
        if now < schedule.next_due {
            panic!("Next execution is not yet due");
        }

        let client = token::Client::new(&env, &schedule.token);
        client.transfer_from(
            &env.current_contract_address(),
            &schedule.payer,
            &schedule.payee,
            &schedule.amount,
        );

        schedule.cycles_executed += 1;
        let cycle = schedule.cycles_executed;
        let next_due = now + schedule.interval_seconds;
        schedule.next_due = next_due;

        let auto_cancelled = schedule.max_cycles > 0 && cycle >= schedule.max_cycles;
        if auto_cancelled {
            schedule.active = false;
        }

        env.storage()
            .persistent()
            .set(&DataKey3::RecurringSchedule(schedule_id), &schedule);
        env.storage().persistent().extend_ttl(
            &DataKey3::RecurringSchedule(schedule_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_recurring_payment_executed(
            &env,
            schedule_id,
            cycle,
            schedule.amount,
            if auto_cancelled { 0 } else { next_due },
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Payer cancels their recurring payment schedule at any time.
    pub fn cancel_recurring_payment(env: Env, payer: Address, schedule_id: u32) {
        Self::require_not_paused(&env);
        payer.require_auth();

        let mut schedule: RecurringPaymentSchedule = env
            .storage()
            .persistent()
            .get(&DataKey3::RecurringSchedule(schedule_id))
            .expect("Recurring schedule not found");

        if schedule.payer != payer {
            panic!("Only the payer can cancel this schedule");
        }

        if !schedule.active {
            panic!("Recurring schedule is already inactive");
        }

        schedule.active = false;

        env.storage()
            .persistent()
            .set(&DataKey3::RecurringSchedule(schedule_id), &schedule);
        env.storage().persistent().extend_ttl(
            &DataKey3::RecurringSchedule(schedule_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_recurring_payment_cancelled(&env, schedule_id, payer);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get a recurring payment schedule by ID.
    pub fn get_recurring_schedule(env: Env, schedule_id: u32) -> RecurringPaymentSchedule {
        env.storage()
            .persistent()
            .get(&DataKey3::RecurringSchedule(schedule_id))
            .expect("Recurring schedule not found")
    }

    // ─── On-Chain Dispute Mediation DAO ──────────────────────────────────────

    /// Admin configures the DAO mediator panel used to resolve escalated disputes.
    ///
    /// `dao_members` — addresses allowed to vote on mediation cases (max 20).
    /// `vote_window_seconds` — how long the vote window stays open after escalation.
    /// `min_votes` — minimum votes cast (either side) before a verdict is executable.
    pub fn configure_dao(
        env: Env,
        dao_members: Vec<Address>,
        vote_window_seconds: u64,
        min_votes: u32,
    ) {
        Self::require_not_paused(&env);
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();

        if dao_members.is_empty() {
            panic!("DAO must have at least one member");
        }
        if vote_window_seconds == 0 {
            panic!("Vote window must be positive");
        }
        if min_votes == 0 {
            panic!("Minimum votes must be at least 1");
        }

        env.storage().instance().set(&DataKey3::DaoMembers, &dao_members);
        env.storage().instance().set(&DataKey3::DaoVoteWindowSeconds, &vote_window_seconds);
        env.storage().instance().set(&DataKey3::DaoMinVotes, &min_votes);

        events::emit_dao_configured(&env, dao_members.len() as u32, vote_window_seconds, min_votes);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Escalate a disputed payment to the DAO for mediation.
    ///
    /// Can be called by the payment customer or the contract admin. The payment
    /// must be in `Disputed` status. A new `DaoMediationCase` is opened and the
    /// vote window starts immediately.
    ///
    /// Panics with `DaoNotConfigured` if `configure_dao` has not been called.
    /// Panics with `DaoAlreadyEscalated` if a case already exists for this payment.
    /// Panics with `PaymentNotDisputed` if the payment is not in disputed status.
    pub fn escalate_to_dao(env: Env, initiator: Address, payment_id: u32) -> u32 {
        Self::require_not_paused(&env);
        initiator.require_auth();

        let payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");

        // Only the customer or admin may escalate.
        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        if initiator != payment.customer && initiator != admin {
            panic!("Only the payment customer or admin may escalate");
        }

        if payment.status != PaymentStatus::Disputed
            && payment.status != PaymentStatus::EscalatedDispute
        {
            panic_with_error!(&env, Error::PaymentNotDisputed);
        }

        // Ensure DAO is configured.
        let dao_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey3::DaoMembers)
            .unwrap_or_else(|| panic_with_error!(&env, Error::DaoNotConfigured));
        if dao_members.is_empty() {
            panic_with_error!(&env, Error::DaoNotConfigured);
        }

        let vote_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey3::DaoVoteWindowSeconds)
            .unwrap_or(DEFAULT_DAO_VOTE_WINDOW_SECONDS);

        // Ensure not already escalated (check for existing open case with this payment_id).
        if Self::find_open_dao_case_id_by_payment(&env, payment_id).is_some() {
            panic_with_error!(&env, Error::DaoAlreadyEscalated);
        }

        let case_counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey3::DaoMediationCounter)
            .unwrap_or(0);

        let now = env.ledger().timestamp();
        let vote_window_closes_at = now + vote_window;
        let case_id = case_counter;

        let case = DaoMediationCase {
            case_id,
            payment_id,
            initiated_by: initiator.clone(),
            created_at: now,
            vote_window_closes_at,
            votes_for_merchant: 0,
            votes_for_customer: 0,
            executed: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey3::DaoMediationCase(case_id), &case);
        env.storage().persistent().extend_ttl(
            &DataKey3::DaoMediationCase(case_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .persistent()
            .set(&DataKey3::DaoCaseByPayment(payment_id), &case_id);
        env.storage().persistent().extend_ttl(
            &DataKey3::DaoCaseByPayment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage()
            .instance()
            .set(&DataKey3::DaoMediationCounter, &(case_counter + 1));

        events::emit_dispute_escalated_to_dao(&env, payment_id, case_id, initiator, vote_window_closes_at);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        case_id
    }

    /// Cancel an active DAO escalation for a payment before any votes are cast.
    ///
    /// Admin-gated: only the contract admin may call this function.
    /// Panics with `DaoCaseNotFoundForPayment` if there is no active case for the payment.
    /// Panics with `DaoEscalationHasVotes` if voting activity has already started.
    pub fn cancel_dao_escalation(env: Env, payment_id: u32) {
        Self::require_not_paused(&env);

        let admin: Address = env.storage().instance().get(&DataKey::Admin).expect("Not initialized");
        admin.require_auth();

        let case_id = Self::find_open_dao_case_id_by_payment(&env, payment_id)
            .unwrap_or_else(|| panic_with_error!(&env, Error2::DaoCaseNotFoundForPayment));
        let case: DaoMediationCase = env
            .storage()
            .persistent()
            .get(&DataKey3::DaoMediationCase(case_id))
            .expect("Mediation case not found");

        if case.votes_for_merchant > 0 || case.votes_for_customer > 0 {
            panic_with_error!(&env, Error2::DaoEscalationHasVotes);
        }

        env.storage()
            .persistent()
            .remove(&DataKey3::DaoMediationCase(case_id));
        events::emit_dao_escalation_cancelled(&env, payment_id, case_id);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Cast a vote on a DAO mediation case.
    ///
    /// `for_merchant` — true to rule in the merchant's favour, false for the customer.
    /// Only registered DAO members may vote. Each member may vote once per case.
    /// Panics with `DaoVoteWindowClosed` if the vote window has expired.
    pub fn dao_vote(env: Env, voter: Address, case_id: u32, for_merchant: bool) {
        Self::require_not_paused(&env);
        voter.require_auth();

        let dao_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey3::DaoMembers)
            .unwrap_or_else(|| panic_with_error!(&env, Error::DaoNotConfigured));
        if !dao_members.contains(&voter) {
            panic_with_error!(&env, Error::NotADaoMember);
        }

        let mut case: DaoMediationCase = env
            .storage()
            .persistent()
            .get(&DataKey3::DaoMediationCase(case_id))
            .expect("Mediation case not found");

        if case.executed {
            panic_with_error!(&env, Error::DaoCaseAlreadyExecuted);
        }
        if env.ledger().timestamp() > case.vote_window_closes_at {
            panic_with_error!(&env, Error::DaoVoteWindowClosed);
        }

        // Prevent double-voting.
        let vote_key = DataKey3::DaoMediationVote(case_id, voter.clone());
        if env.storage().persistent().has(&vote_key) {
            panic_with_error!(&env, Error::DaoAlreadyVoted);
        }
        env.storage().persistent().set(&vote_key, &for_merchant);
        env.storage().persistent().extend_ttl(
            &vote_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        if for_merchant {
            case.votes_for_merchant += 1;
        } else {
            case.votes_for_customer += 1;
        }

        env.storage()
            .persistent()
            .set(&DataKey3::DaoMediationCase(case_id), &case);
        env.storage().persistent().extend_ttl(
            &DataKey3::DaoMediationCase(case_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_dao_vote_cast(
            &env,
            case_id,
            voter,
            for_merchant,
            case.votes_for_merchant,
            case.votes_for_customer,
        );
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Execute the DAO verdict for a mediation case after the vote window closes.
    ///
    /// Can be called by anyone once the window has elapsed. The side with more
    /// votes wins; ties resolve in the customer's favour (refund). Requires at
    /// least `min_votes` total votes to be executable.
    ///
    /// Panics with `DaoVoteWindowOpen` if the window has not yet closed.
    /// Panics with `DaoMinVotesNotMet` if total votes are below the threshold.
    pub fn execute_dao_verdict(env: Env, case_id: u32) {
        Self::require_not_paused(&env);

        let mut case: DaoMediationCase = env
            .storage()
            .persistent()
            .get(&DataKey3::DaoMediationCase(case_id))
            .expect("Mediation case not found");

        if case.executed {
            panic_with_error!(&env, Error::DaoCaseAlreadyExecuted);
        }
        if env.ledger().timestamp() <= case.vote_window_closes_at {
            panic_with_error!(&env, Error::DaoVoteWindowOpen);
        }

        let min_votes: u32 = env
            .storage()
            .instance()
            .get(&DataKey3::DaoMinVotes)
            .unwrap_or(DEFAULT_DAO_MIN_VOTES);

        let total_votes = case.votes_for_merchant + case.votes_for_customer;
        if total_votes < min_votes {
            panic_with_error!(&env, Error::DaoMinVotesNotMet);
        }

        // Merchant wins only if strictly more votes favour them.
        let merchant_wins = case.votes_for_merchant > case.votes_for_customer;

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(case.payment_id))
            .expect("Payment not found");

        // Guard: the payment must still be in an unresolved dispute state.
        // If a prior verdict (or admin action) already settled it, reject.
        if payment.status != PaymentStatus::Disputed
            && payment.status != PaymentStatus::EscalatedDispute
        {
            panic_with_error!(&env, Error::DaoCaseAlreadyExecuted);
        }

        let old_status = payment.status;
        let token_client = token::Client::new(&env, &payment.token);

        if merchant_wins {
            payment.status = PaymentStatus::Completed;
            env.storage()
                .persistent()
                .set(&DataKey::Settled(case.payment_id), &false);
            env.storage().persistent().extend_ttl(
                &DataKey::Settled(case.payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        } else {
            // Refund the customer for the outstanding escrowed amount.
            let already_refunded = payment.refunded_amount;
            let owed = payment.amount - already_refunded;
            if owed > 0 {
                token_client.transfer(&env.current_contract_address(), &payment.customer, &owed);
            }
            payment.status = PaymentStatus::Refunded;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Payment(case.payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(case.payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_payment_status_changed(&env, case.payment_id, old_status, payment.status);

        // Remove the temporary dispute record if still present.
        env.storage().temporary().remove(&DataKey::Dispute(case.payment_id));

        case.executed = true;
        env.storage()
            .persistent()
            .set(&DataKey3::DaoMediationCase(case_id), &case);
        env.storage().persistent().extend_ttl(
            &DataKey3::DaoMediationCase(case_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_dao_verdict_executed(
            &env,
            case_id,
            case.payment_id,
            merchant_wins,
            case.votes_for_merchant,
            case.votes_for_customer,
        );
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Return a DAO mediation case by its ID.
    pub fn get_dao_mediation_case(env: Env, case_id: u32) -> DaoMediationCase {
        env.storage()
            .persistent()
            .get(&DataKey3::DaoMediationCase(case_id))
            .expect("Mediation case not found")
    }

    /// Return the DAO mediation case for `payment_id`, if one exists.
    pub fn get_dao_case_by_payment(env: Env, payment_id: u32) -> Option<DaoMediationCase> {
        let case_id: u32 = env.storage().persistent().get(&DataKey3::DaoCaseByPayment(payment_id))?;
        env.storage().persistent().extend_ttl(
            &DataKey3::DaoCaseByPayment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        let case_opt: Option<DaoMediationCase> = env
            .storage()
            .persistent()
            .get(&DataKey3::DaoMediationCase(case_id));
        if case_opt.is_some() {
            env.storage().persistent().extend_ttl(
                &DataKey3::DaoMediationCase(case_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        case_opt
    }

    /// Return the list of registered DAO mediator addresses.
    pub fn get_dao_members(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey3::DaoMembers)
            .unwrap_or(Vec::new(&env))
    }
}

#[cfg(test)]
mod test_disputes_by_merchant;

#[cfg(test)]
mod test_tip;

#[cfg(test)]
mod test_consent;

#[cfg(test)]
mod test_token_whitelist;

#[cfg(test)]
mod test_collateral;

#[cfg(test)]
mod test_notification_keys;

#[cfg(test)]
mod test_external_id_multisig_voucher;

#[cfg(test)]
mod test_merchant_ban;

#[cfg(test)]
mod test_dynamic_settlement;
#[cfg(test)]
mod test_loyalty_points;
#[cfg(test)]
mod test_referral;
#[cfg(test)]
mod test_retry_queue;
#[cfg(test)]
mod test_spending_limit;
#[cfg(test)]
mod test_buyer_trust_tier;
#[cfg(test)]
mod test_oracle_staleness;

#[cfg(test)]
mod test_dao_mediation;
#[cfg(test)]
mod test_kyb;

#[cfg(test)]
mod test;

pub use events::*;
