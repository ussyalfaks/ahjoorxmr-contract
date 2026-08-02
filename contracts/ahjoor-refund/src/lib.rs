#![no_std]
#![allow(deprecated, unused_imports, unused_variables, dead_code, unused_mut)]

use ahjoor_token_whitelist::TokenWhitelistClient;
use soroban_sdk::{
    contract, contractimpl, contracttype, token, Address, BytesN, Env, String, Symbol, Vec,
};

// --- Storage TTL Constants ---
const INSTANCE_LIFETIME_THRESHOLD: u32 = 100_000;
const INSTANCE_BUMP_AMOUNT: u32 = 120_000;

const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

/// #582: Minimum allowed `dispute_window`, in seconds (1 hour), below which
/// `auto_approve_refund` would become callable immediately or near-immediately
/// after a refund request, defeating the intended merchant review window.
const MIN_DISPUTE_WINDOW_SECONDS: u64 = 3_600;

// ---------------------------------------------------------------------------
// Minimal payment contract client — only the fields we need from get_payment.
// ---------------------------------------------------------------------------
mod payment_contract {
    use soroban_sdk::{
        contractclient, contracttype, Address, BytesN, Env, Map, String, Symbol, Vec,
    };

    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub enum PaymentStatus {
        Pending = 0,
        Completed = 1,
        Refunded = 2,
        Disputed = 3,
        Expired = 4,
        ScheduledPending = 5,
    }

    #[contracttype]
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct SplitRecipient {
        pub recipient: Address,
        pub bps: u32,
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
        pub expires_at: u64,
        pub refunded_amount: i128,
        pub reference: Option<String>,
        pub metadata: Option<Map<String, String>>,
        pub split_recipients: Option<Vec<SplitRecipient>>,
        pub execute_after: u64,
        pub category: Option<Symbol>,
        pub tags: Option<Vec<Symbol>>,
        pub capture_deadline: u64,
        pub external_id: Option<BytesN<32>>,
        pub tipping_enabled: bool,
        pub extension_count: u32,
    }

    #[allow(dead_code)]
    #[contractclient(name = "PaymentContractClient")]
    pub trait PaymentContractInterface {
        fn get_payment(env: Env, payment_id: u32) -> Payment;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum RefundStatus {
    Requested = 0,
    Approved = 1,
    Rejected = 2,
    Processed = 3,
    UnderAppeal = 4,
    /// Terminal status: refund request was cancelled before approval (#168)
    Cancelled = 5,
    /// Merchant has made a counter-offer; awaiting customer response
    CounterOffered = 6,
    /// #245: Merchant approved a partial amount; funds transferred; no further action possible.
    PartiallyApproved = 7,
    /// #276: Merchant submitted evidence; awaiting admin adjudication.
    EvidenceSubmitted = 8,
    /// #276: Evidence window expired without merchant submission.
    EvidencePeriodExpired = 9,
    /// Escalated to senior arbiter after primary reviewer missed deadline.
    EscalatedToSenior = 10,
    /// Cross-contract refund registered from a whitelisted origin contract.
    CrossContractRefunded = 11,
    /// Auto-approved due to merchant non-response (#335)
    AutoApproved = 12,
}

/// #238: Priority label for refund requests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum Priority {
    High = 0,
    Medium = 1,
    Low = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Refund {
    pub id: u32,
    pub payment_id: u32,
    pub customer: Address,
    pub merchant: Address,
    pub amount: i128,
    pub token: Address,
    pub status: RefundStatus,
    pub reason: String,
    pub reason_code: u32,
    pub requested_at: u64,
    pub approved_at: Option<u64>,
    pub processed_at: Option<u64>,
    pub rejected_at: Option<u64>,
    pub auto_approved_source: Option<String>, // "whitelist" or "dispute_window"
    pub escrow_id: Option<u32>,               // For cross-contract escrow refunds
    pub fee_amount: Option<i128>,             // Fee deducted on processing
    /// #238: Priority label; defaults to Medium.
    pub priority: Priority,
    /// #276: Ledger sequence by which merchant must submit evidence.
    pub merch_response_deadline: u32,
    /// #276: Whether merchant has submitted evidence.
    pub evidence_submitted: bool,
    /// Ledger by which the primary reviewer must resolve (0 = not configured).
    pub primary_review_deadline_ledger: u32,
    /// Ledger by which the senior arbiter must resolve (0 = not escalated).
    pub senior_review_deadline_ledger: u32,
    /// Origin contract address for cross-contract refunds (None for customer-initiated).
    pub origin_contract: Option<Address>,
    /// Ledger by which merchant must respond before auto-approval (#335)
    pub auto_approval_deadline_ledger: u32,
    /// Whether merchant has requested an extension (#335)
    pub extension_requested: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundStats {
    pub total_requested: u32,
    pub total_approved: u32,
    pub total_rejected: u32,
    pub total_processed: u32,
    pub total_amount_refunded: i128,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    PauseReason,
    RefundCounter,
    ContractVersion,
    MigrationCompleted(u32),
    Refund(u32),
    /// Address of the payment contract for cross-contract validation (#64).
    PaymentContractAddress,
    /// Index: customer → Vec<u32> of refund IDs
    CustomerRefunds(Address),
    /// Index: merchant → Vec<u32> of refund IDs
    MerchantRefunds(Address),
    /// Index: payment_id → Vec<u32> of refund IDs
    PaymentRefunds(u32),
    /// Dispute window in seconds; after this period a Requested refund can be auto-approved.
    DisputeWindow,
    /// Cumulative refunded amount per payment_id (#165).
    RefundedAmount(u32),
    /// Cumulative processed (disbursed) refund amount per payment_id (#349).
    PaymentRefundedAmount(u32),
    /// Whitelist of auto-approved merchants (Issue #163)
    AutoApprovedMerchants,
    /// Escrow contract address for cross-contract refund registration (Issue #162)
    EscrowContractAddress,
    /// Global refund statistics (Issue #161)
    GlobalRefundStats,
    /// Per-merchant refund statistics (Issue #161)
    MerchantRefundStats(Address),
    /// Refund processing fee in basis points (Issue #160)
    RefundFeeBps,
    /// Fee recipient address (Issue #160)
    FeeRecipient,
    /// Index: (merchant, reason_code) → Vec<u32> of refund IDs (#157)
    ReasonCodeRefunds(Address, u32),
    /// Count of refunds per (merchant, reason_code) (#157)
    ReasonCodeCount(Address, u32),
    /// Global auto-reject window in seconds (#158)
    AutoRejectWindow,
    /// Per-refund deadline extension in seconds (#158)
    RefundDeadlineExtension(u32),
    /// Appeal window in seconds after rejection (#159)
    AppealWindow,
    /// Ordered queue of pending (Requested) refund IDs (#164)
    PendingRefundQueue,
    /// Tiered refund policy: Vec<(max_seconds_since_payment, refund_bps)>.
    RefundTiers,
    /// Cooldown period in seconds between customer refund requests (#166)
    RefundCooldown,
    /// Last refund request timestamp per customer (#166) — stored in temporary storage
    LastRefundRequest(Address),
    /// List of approved delegate addresses per merchant (#167)
    MerchantDelegates(Address),
    /// Window in seconds during which a customer can cancel their own request (#168)
    CustomerCancelWindow,
    /// Token whitelist contract address
    TokenWhitelistContract,
    /// Fraud score per buyer address
    FraudScore(Address),
    /// Admin-configurable threshold for blocking refund requests
    FraudScoreBlockThreshold,
    /// Last fraud score update timestamp for decay calculation
    FraudScoreLastUpdate(Address),
    /// Fraud score decay interval in seconds (default: 30 days)
    FraudScoreDecayInterval,
    /// Counter-offer record per refund_id
    CounterOffer(u32),
    /// Configurable counter-offer expiry window in seconds
    CounterOfferExpirySeconds,
    /// Default resolution on counter-offer expiry: true = accept original, false = reject
    CounterOfferDefaultResolution,
    /// #217: admin-configurable max batch size for batch refund operations
    MaxBatchSize,
    /// Per-merchant auto-approval threshold amount (#228)
    AutoApproveThreshold(Address),
    /// Global fraud score cap for auto-approval (#228)
    AutoApproveFraudScoreCap,
    /// #238: Sorted admin queue (Vec<u32> of refund IDs ordered by priority then timestamp)
    AdminRefundQueue,
    /// #274: Reserve ratio in basis points (e.g. 200 = 2%)
    ReserveRatioBps,
    /// #274: Per-merchant reserve balance
    MerchantReserve(Address),
    /// #274: Trailing 30-day payment volume per merchant (updated on payment creation)
    MerchantVolume(Address),
    /// #274: Flag: merchant is non-compliant (below reserve minimum)
    MerchantFlagged(Address),
    /// #276: Admin-configurable merchant response window in ledgers
    MerchantResponseWindow,
    /// #276: Evidence submitted per refund_id
    RefundEvidence(u32),
    /// Store-credit voucher record keyed by (merchant, customer)
    StoreCredit(Address, Address),
}

/// Overflow key enum — DataKey is capped at 50 variants by the soroban XDR limit.
#[derive(Clone)]
#[contracttype]
pub enum DataKey2 {
    /// Admin-configurable max bonus BPS above refund amount for store-credit vouchers
    MaxVoucherBonusBps,

    // --- Feature: Escalating Mediation Timeline ---
    /// Admin-configured default primary review window in ledgers
    PrimaryReviewDeadlineLedgers,
    /// Admin-configured senior review window in ledgers
    SeniorReviewDeadlineLedgers,
    /// Registered senior arbiter address
    SeniorArbiter,
    /// Flag: auto-approve refund if senior arbiter also misses their deadline
    AutoApproveOnSeniorMiss,

    // --- Feature: Customer Abuse Score ---
    /// Per-customer abuse record
    CustomerAbuseRecord(Address),
    /// Admin-configurable abuse score block threshold
    AbuseBlockThreshold,
    /// Ledgers a customer is blocked after hitting the abuse threshold
    BlockDurationLedgers,
    /// Ledger window for detecting rapid submissions
    RapidSubmissionWindowLedgers,

    // --- Feature: Cross-Contract Refund Registration ---
    /// Vec<Address> of whitelisted origin contracts
    CrossContractWhitelist,
    /// Vec<u32> tracking cross-contract refund IDs for admin queue
    CrossContractRefundQueue,

    // --- Feature: Merchant Reserve Requirement (#334) ---
    /// Minimum reserve in basis points of monthly volume
    MinReserveBpsOfMonthlyVolume,
    /// Per-merchant reserve balance
    MerchantReserveBalance(Address),
    /// Alert threshold for low reserve in basis points
    ReserveAlertThresholdBps,
    /// Waiver expiry ledger per merchant (#334)
    ReserveWaiverExpiryLedger(Address),

    // --- Feature: Auto-Approval on Non-Response (#335) ---
    /// Deadline for merchant response in ledgers
    MerchantResponseDeadlineLedgers,
    /// Ledger by which merchant must respond for refund (#335)
    RefundMerchantDeadlineLedger(u32),
    /// Whether extension has been used for refund (#335)
    RefundExtensionUsed(u32),
    /// Extension duration in ledgers (#335)
    RefundExtensionLedgers,
    /// Merchant is exempt from auto-approval on non-response (#335)
    MerchantAutoApproveExempt(Address),
    /// Token accepted for merchant reserve deposits (#334)
    ReserveToken,
    /// Admin default refund policy for merchants without their own policy (#320)
    GlobalRefundPolicy,

    // --- Feature: Merchant On-Chain Refund Policy (#320) ---
    /// Merchant refund policy: (eligible_window_ledgers, max_refund_bps, excluded_tags)
    MerchantRefundPolicy(Address),
    /// Policy version snapshot at refund request time (#320)
    RefundPolicySnapshot(u32),

    // --- Feature: Configurable Abuse Score Decay (#355) ---
    /// Decay period in ledgers (e.g. 10_000 ≈ 14 hours at 5s/ledger). Default: 10_000.
    AbuseScoreDecayPeriodLedgers,
    /// Decay factor in basis points per period (e.g. 5000 = halve every period). Default: 5000.
    AbuseScoreDecayFactorBps,

    // --- Feature: Evidence Hash Anchoring (#365) ---
    /// On-chain SHA-256 content hash anchor per (refund_id, submitter) (#365)
    EvidenceHash(u32, Address),
}

mod events;

/// Merchant refund policy for on-chain validation (#320)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundPolicy {
    /// Ledgers within which a refund can be requested
    pub eligible_window_ledgers: u32,
    /// Maximum refund amount as basis points (0-10000)
    pub max_refund_bps: u32,
    /// Payment tags that are excluded from refunds (empty = none excluded)
    pub excluded_tags: Vec<Symbol>,
}

/// Counter-offer record stored during negotiation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CounterOffer {
    pub amount: i128,
    pub offered_at: u64,
    pub expiry: u64,
}

const DEFAULT_COUNTER_OFFER_EXPIRY_SECONDS: u64 = 48 * 60 * 60; // 48 hours
/// Default on expiry: accept the original refund amount
const DEFAULT_COUNTER_OFFER_ACCEPT_ON_EXPIRY: bool = true;

const DEFAULT_MAX_BATCH_SIZE: u32 = 20;

/// Default merchant response window: ~7 days at ~5s/ledger = 120_960 ledgers
const DEFAULT_MERCHANT_RESPONSE_WINDOW_LEDGERS: u32 = 120_960;

/// Default abuse block duration: ~30 days at ~5s/ledger = 518_400 ledgers
const DEFAULT_BLOCK_DURATION_LEDGERS: u64 = 518_400;

/// Default rapid submission window: ~1 hour = 720 ledgers
const DEFAULT_RAPID_SUBMISSION_WINDOW_LEDGERS: u32 = 720;

/// Default primary review deadline: ~3 days = 51_840 ledgers
const DEFAULT_PRIMARY_REVIEW_DEADLINE_LEDGERS: u32 = 51_840;

/// Default senior review deadline: ~2 days = 34_560 ledgers
const DEFAULT_SENIOR_REVIEW_DEADLINE_LEDGERS: u32 = 34_560;

/// Per-customer abuse score record stored in persistent storage.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundAbuseRecord {
    pub total_requests: u32,
    pub denied_count: u32,
    pub auto_denied_count: u32,
    pub rapid_submission_count: u32,
    pub abuse_score: u32,
    /// Ledger until which the customer is blocked (0 = not blocked).
    pub blocked_until_ledger: u64,
    /// Ledger of the last score increment (for decay calculation).
    pub last_increment_ledger: u64,
    /// Ledger of the last refund submission (for rapid-submission detection).
    pub last_submission_ledger: u32,
}

/// #276: Evidence submitted by merchant for a refund dispute.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundEvidence {
    pub evidence_hashes: Vec<BytesN<32>>,
    pub statement_hash: BytesN<32>,
    pub submitted_at_ledger: u32,
}

/// Store-credit voucher record issued as an alternative to token refund.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoreCredit {
    /// Refund ID that originated this credit.
    pub refund_id: u32,
    /// Merchant the credit is redeemable against.
    pub merchant: Address,
    /// Customer who holds the credit.
    pub customer: Address,
    /// Remaining credit balance (decremented on partial redemption).
    pub credit_amount: i128,
    /// Ledger sequence after which the credit is expired.
    pub expiry_ledger: u64,
    /// Whether the one-time expiry extension has been used.
    pub extension_used: bool,
}

/// Default max bonus BPS: 1000 = 10% above refund amount.
const DEFAULT_MAX_VOUCHER_BONUS_BPS: u32 = 1_000;

/// Result of a batch refund operation (#217).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchRefundResult {
    pub processed: Vec<u32>,
    pub skipped: Vec<u32>,
}

/// Optional extended configuration for `initialize`.
/// Groups the extra parameters that would otherwise exceed Soroban's 10-parameter limit.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefundInitConfig {
    pub escrow_contract: Option<Address>,
    pub refund_fee_bps: u32,
    pub fee_recipient: Option<Address>,
    pub auto_reject_window_seconds: u64,
    pub appeal_window_seconds: u64,
    pub refund_tiers: Option<Vec<(u64, u32)>>,
    pub refund_cooldown_seconds: u64,
    pub customer_cancel_window_seconds: u64,
}

#[contract]
pub struct AhjoorRefundContract;

#[contractimpl]
impl AhjoorRefundContract {
    /// Initialize the refund contract.
    /// `config` bundles optional extended settings to stay within Soroban's 10-parameter limit.
    /// Pass `None` for `config` to use all defaults (zero fees, no cooldown, etc.).
    pub fn initialize(
        env: Env,
        admin: Address,
        payment_contract: Address,
        dispute_window: u64,
        config: Option<RefundInitConfig>,
    ) {
        let cfg = config.unwrap_or(RefundInitConfig {
            escrow_contract: None,
            refund_fee_bps: 0,
            fee_recipient: None,
            auto_reject_window_seconds: 0,
            appeal_window_seconds: 0,
            refund_tiers: None,
            refund_cooldown_seconds: 0,
            customer_cancel_window_seconds: 0,
        });
        let escrow_contract = cfg.escrow_contract;
        let refund_fee_bps = cfg.refund_fee_bps;
        let fee_recipient = cfg.fee_recipient;
        let auto_reject_window_seconds = cfg.auto_reject_window_seconds;
        let appeal_window_seconds = cfg.appeal_window_seconds;
        let refund_tiers = cfg.refund_tiers;
        let refund_cooldown_seconds = cfg.refund_cooldown_seconds;
        let customer_cancel_window_seconds = cfg.customer_cancel_window_seconds;
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }

        // Validate fee cap (max 200 bps = 2%)
        if refund_fee_bps > 200 {
            panic!("Refund fee cannot exceed 200 basis points (2%)");
        }

        // #582: Enforce a minimum dispute window so auto_approve_refund cannot
        // become immediately callable.
        if dispute_window < MIN_DISPUTE_WINDOW_SECONDS {
            panic!("DisputeWindowBelowMinimum");
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::RefundCounter, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &1u32);
        env.storage()
            .instance()
            .set(&DataKey::PaymentContractAddress, &payment_contract);
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &dispute_window);

        // Issue #162: Store escrow contract address
        if let Some(escrow_addr) = escrow_contract {
            env.storage()
                .instance()
                .set(&DataKey::EscrowContractAddress, &escrow_addr);
        }

        // Issue #160: Store fee configuration
        env.storage()
            .instance()
            .set(&DataKey::RefundFeeBps, &refund_fee_bps);
        if let Some(recipient) = fee_recipient {
            env.storage()
                .instance()
                .set(&DataKey::FeeRecipient, &recipient);
        }

        let tiers = refund_tiers.unwrap_or(Vec::new(&env));
        Self::validate_refund_tiers(&tiers);
        env.storage().instance().set(&DataKey::RefundTiers, &tiers);

        // Issue #161: Initialize global stats
        let initial_stats = RefundStats {
            total_requested: 0,
            total_approved: 0,
            total_rejected: 0,
            total_processed: 0,
            total_amount_refunded: 0,
        };
        env.storage()
            .instance()
            .set(&DataKey::GlobalRefundStats, &initial_stats);

        // Issue #163: Initialize empty whitelist
        let empty_whitelist: Vec<Address> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::AutoApprovedMerchants, &empty_whitelist);

        // Issue #158: Store auto-reject window (default 30 days = 2_592_000s)
        let effective_auto_reject = if auto_reject_window_seconds == 0 {
            2_592_000u64
        } else {
            auto_reject_window_seconds
        };
        env.storage()
            .instance()
            .set(&DataKey::AutoRejectWindow, &effective_auto_reject);

        // Issue #159: Store appeal window
        env.storage()
            .instance()
            .set(&DataKey::AppealWindow, &appeal_window_seconds);

        // Issue #164: Initialize empty pending refund queue
        let empty_queue: Vec<u32> = Vec::new(&env);
        env.storage()
            .persistent()
            .set(&DataKey::PendingRefundQueue, &empty_queue);

        // Issue #166: Store per-customer cooldown period (0 = no cooldown)
        env.storage()
            .instance()
            .set(&DataKey::RefundCooldown, &refund_cooldown_seconds);

        // Issue #168: Store customer cancel window
        env.storage().instance().set(
            &DataKey::CustomerCancelWindow,
            &customer_cancel_window_seconds,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Request a refund linked to an existing completed payment.
    /// Cross-contract validates: payment exists, status is Completed, merchant matches,
    /// and refund amount does not exceed the original payment amount (#64).
    /// If merchant is whitelisted, auto-approves immediately (Issue #163).
    /// `reason_code`: 0=Defective, 1=NotDelivered, 2=DuplicateCharge, 3=Unauthorized, 4=Other (#157).
    /// Returns the refund ID.
    pub fn request_refund(
        env: Env,
        customer: Address,
        payment_id: u32,
        amount: i128,
        reason: String,
        reason_code: u32,
    ) -> u32 {
        Self::require_not_paused(&env);
        customer.require_auth();

        if amount <= 0 {
            panic!("Refund amount must be positive");
        }

        // #157: Validate reason code (0-4)
        if reason_code > 4 {
            panic!("Invalid reason code: must be 0-4");
        }

        // #166: Enforce per-customer cooldown
        let cooldown: u64 = env
            .storage()
            .instance()
            .get(&DataKey::RefundCooldown)
            .unwrap_or(0);
        if cooldown > 0 {
            let last_request: u64 = env
                .storage()
                .temporary()
                .get(&DataKey::LastRefundRequest(customer.clone()))
                .unwrap_or(0);
            let now_ts = env.ledger().timestamp();
            if last_request > 0 && now_ts.saturating_sub(last_request) < cooldown {
                let next_eligible_at = last_request + cooldown;
                events::emit_refund_cooldown_active(&env, customer.clone(), next_eligible_at);
                panic!("RefundCooldownActive");
            }
        }

        // Fraud score check: block buyers at or above threshold
        if Self::is_buyer_blocked(&env, &customer) {
            let score = Self::get_fraud_score(env.clone(), customer.clone());
            let threshold = Self::get_fraud_score_block_threshold(env.clone());
            panic!(
                "BuyerBlockedForFraud: score={}, threshold={}",
                score, threshold
            );
        }

        // Abuse score check: block customers with active abuse block or threshold exceeded
        Self::check_abuse_block(&env, &customer);

        // --- Cross-contract validation (#64) ---
        let payment_contract_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentContractAddress)
            .expect("Payment contract not configured");

        let payment_client =
            payment_contract::PaymentContractClient::new(&env, &payment_contract_addr);
        let payment = payment_client
            .try_get_payment(&payment_id)
            .unwrap_or_else(|_| panic!("PaymentContractError: payment not found"))
            .unwrap_or_else(|_| panic!("PaymentContractError: payment not found"));

        // Validate payment status is Completed
        if payment.status != payment_contract::PaymentStatus::Completed {
            panic!("PaymentContractError: payment is not completed");
        }

        // Validate merchant matches the payment's merchant
        // (customer is the one requesting, merchant is cached for audit)
        let merchant = payment.merchant.clone();

        // Validate refund amount does not exceed original payment amount
        let already_refunded: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::RefundedAmount(payment_id))
            .unwrap_or(0);

        let mut effective_amount = amount;
        let tiers = Self::get_refund_tiers(env.clone());
        let mut applied_tier_bps: Option<u32> = None;
        let mut tier_max_refundable: Option<i128> = None;

        if !tiers.is_empty() {
            let elapsed = env.ledger().timestamp().saturating_sub(payment.created_at);
            let mut matched = false;
            for i in 0..tiers.len() {
                let (max_seconds, tier_bps) = tiers.get(i).unwrap();
                if elapsed <= max_seconds {
                    matched = true;
                    let absolute_cap = (payment.amount as u128 * tier_bps as u128 / 10_000) as i128;
                    let remaining_under_tier = absolute_cap.saturating_sub(already_refunded);
                    if remaining_under_tier <= 0 {
                        panic!("ExceedsRefundableAmount");
                    }
                    effective_amount = if amount > remaining_under_tier {
                        remaining_under_tier
                    } else {
                        amount
                    };
                    applied_tier_bps = Some(tier_bps);
                    tier_max_refundable = Some(remaining_under_tier);
                    break;
                }
            }

            if !matched {
                panic!("RefundWindowExpired");
            }
        }

        if effective_amount + already_refunded > payment.amount {
            panic!("ExceedsRefundableAmount");
        }
        let policy = Self::refund_policy_for(&env, &merchant);
        Self::enforce_refund_policy(
            &env,
            &policy,
            payment.created_at,
            effective_amount,
            payment.amount,
            &payment.tags,
        );

        // Cache validated payment data — token comes from the payment record
        let token = payment.token.clone();

        // Token whitelist validation
        Self::require_token_allowed(&env, &token);

        // Escrow funds to this contract so approved refunds can be processed.
        let client = token::Client::new(&env, &token);
        client.transfer(
            &customer,
            &env.current_contract_address(),
            &effective_amount,
        );

        let refund_id = Self::next_refund_id(&env);

        let is_whitelisted = Self::is_merchant_auto_approved(&env, &merchant);

        // #228: Check amount-based auto-approval threshold
        let is_threshold_auto_approved = if !is_whitelisted {
            let threshold: i128 = env
                .storage()
                .instance()
                .get(&DataKey::AutoApproveThreshold(merchant.clone()))
                .unwrap_or(0);
            if threshold > 0 && effective_amount <= threshold {
                // Check fraud score cap
                let fraud_cap: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey::AutoApproveFraudScoreCap)
                    .unwrap_or(u32::MAX);
                let buyer_score = Self::get_fraud_score(env.clone(), customer.clone());
                buyer_score < fraud_cap
            } else {
                false
            }
        } else {
            false
        };

        let is_auto_approved = is_whitelisted || is_threshold_auto_approved;

        let initial_status = if is_auto_approved {
            RefundStatus::Approved
        } else {
            RefundStatus::Requested
        };

        let auto_source = if is_whitelisted {
            Some(String::from_str(&env, "whitelist"))
        } else if is_threshold_auto_approved {
            Some(String::from_str(&env, "threshold"))
        } else {
            None
        };

        let now = env.ledger().timestamp();
        let merchant_response_window: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MerchantResponseWindow)
            .unwrap_or(0);
        let merch_response_deadline = env.ledger().sequence() + merchant_response_window;
        let auto_deadline_window: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::MerchantResponseDeadlineLedgers)
            .unwrap_or(0);

        let primary_review_window: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::PrimaryReviewDeadlineLedgers)
            .unwrap_or(DEFAULT_PRIMARY_REVIEW_DEADLINE_LEDGERS);
        let primary_review_deadline_ledger = if is_auto_approved {
            0
        } else {
            env.ledger().sequence() + primary_review_window
        };

        // Abuse score: record submission ledger and detect rapid submissions
        let current_seq = env.ledger().sequence();
        let rapid_window: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::RapidSubmissionWindowLedgers)
            .unwrap_or(DEFAULT_RAPID_SUBMISSION_WINDOW_LEDGERS);
        Self::update_abuse_on_request(&env, &customer, current_seq, rapid_window);

        let refund = Refund {
            id: refund_id,
            payment_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            amount: effective_amount,
            token: token.clone(),
            status: initial_status,
            reason,
            reason_code,
            requested_at: now,
            approved_at: if is_auto_approved { Some(now) } else { None },
            processed_at: None,
            rejected_at: None,
            auto_approved_source: auto_source,
            escrow_id: None,
            fee_amount: None,
            priority: Priority::Medium,
            merch_response_deadline: if is_auto_approved {
                0
            } else {
                merch_response_deadline
            },
            evidence_submitted: false,
            primary_review_deadline_ledger,
            senior_review_deadline_ledger: 0,
            origin_contract: None,
            auto_approval_deadline_ledger: if is_auto_approved {
                0
            } else {
                env.ledger().sequence() + auto_deadline_window
            },
            extension_requested: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage()
            .persistent()
            .set(&DataKey2::RefundPolicySnapshot(refund_id), &policy);
        env.storage().persistent().set(
            &DataKey2::RefundMerchantDeadlineLedger(refund_id),
            &refund.auto_approval_deadline_ledger,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::append_index(&env, &DataKey::CustomerRefunds(customer.clone()), refund_id);
        Self::append_index(&env, &DataKey::MerchantRefunds(merchant.clone()), refund_id);
        Self::append_index(&env, &DataKey::PaymentRefunds(payment_id), refund_id);

        // #157: Update reason code index and count
        Self::append_index(
            &env,
            &DataKey::ReasonCodeRefunds(merchant.clone(), reason_code),
            refund_id,
        );
        let count_key = DataKey::ReasonCodeCount(merchant.clone(), reason_code);
        let prev_count: u32 = env.storage().persistent().get(&count_key).unwrap_or(0);
        env.storage()
            .persistent()
            .set(&count_key, &(prev_count + 1));
        env.storage().persistent().extend_ttl(
            &count_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // #164: Add to pending queue only when not auto-approved
        if !is_auto_approved {
            Self::append_to_pending_queue(&env, refund_id);
        }

        Self::update_stats_on_request(&env, &merchant, effective_amount);

        // #166: Record this request timestamp in temporary storage (before customer is moved)
        if cooldown > 0 {
            let now_for_cooldown = env.ledger().timestamp();
            env.storage().temporary().set(
                &DataKey::LastRefundRequest(customer.clone()),
                &now_for_cooldown,
            );
            env.storage().temporary().extend_ttl(
                &DataKey::LastRefundRequest(customer.clone()),
                INSTANCE_BUMP_AMOUNT,
                INSTANCE_BUMP_AMOUNT,
            );
        }

        events::emit_refund_requested(
            &env,
            refund_id,
            customer,
            effective_amount,
            token,
            refund.reason,
        );
        events::emit_refund_reason_recorded(&env, refund_id, reason_code);

        if let (Some(tier_bps), Some(max_refundable)) = (applied_tier_bps, tier_max_refundable) {
            events::emit_refund_tier_applied(&env, refund_id, tier_bps, max_refundable);
        }

        if is_whitelisted {
            events::emit_refund_auto_approved_whitelist(
                &env,
                refund_id,
                merchant.clone(),
                effective_amount,
            );
        }

        if is_threshold_auto_approved {
            events::emit_refund_auto_approved_by_threshold(&env, refund_id, effective_amount);
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        refund_id
    }

    /// Approve a refund request. Callable by admin or a delegate of the refund's merchant (#167).
    pub fn approve_refund(env: Env, admin: Address, refund_id: u32) {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        // #167: Allow admin OR a delegate of the refund's merchant
        let is_admin = admin == stored_admin;
        let is_delegate = !is_admin && Self::is_merchant_delegate(&env, &refund.merchant, &admin);

        if !is_admin && !is_delegate {
            panic!("Only admin or merchant delegate can approve refunds");
        }

        if refund.status != RefundStatus::Requested
            && refund.status != RefundStatus::EvidenceSubmitted
            && refund.status != RefundStatus::EvidencePeriodExpired
        {
            panic!("Refund is not in a state that can be approved");
        }

        // #276: Block adjudication during open evidence window
        if refund.status == RefundStatus::Requested
            && refund.merch_response_deadline > 0
            && env.ledger().sequence() < refund.merch_response_deadline
        {
            panic!("EvidenceWindowOpen: merchant evidence window has not elapsed");
        }

        // #276: Auto-advance to EvidencePeriodExpired if deadline passed without evidence
        if refund.status == RefundStatus::Requested
            && refund.merch_response_deadline > 0
            && env.ledger().sequence() >= refund.merch_response_deadline
        {
            refund.status = RefundStatus::EvidencePeriodExpired;
            events::emit_evidence_period_expired(&env, refund_id, refund.merchant.clone());
        }

        refund.status = RefundStatus::Approved;
        refund.approved_at = Some(env.ledger().timestamp());

        // #274: Draw from merchant reserve first
        let reserve_key = DataKey::MerchantReserve(refund.merchant.clone());
        let reserve_balance: i128 = env.storage().persistent().get(&reserve_key).unwrap_or(0);
        if reserve_balance > 0 {
            let draw = if reserve_balance >= refund.amount {
                refund.amount
            } else {
                reserve_balance
            };
            let new_reserve = reserve_balance - draw;
            env.storage().persistent().set(&reserve_key, &new_reserve);
            env.storage().persistent().extend_ttl(
                &reserve_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            // Transfer drawn amount from contract to customer
            let client = token::Client::new(&env, &refund.token);
            client.transfer(&env.current_contract_address(), &refund.customer, &draw);
            events::emit_reserve_used_for_refund(&env, refund.merchant.clone(), refund_id, draw);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // #164: Remove from pending queue
        Self::remove_from_pending_queue(&env, refund_id);

        // Update fraud score: decrement on approved refund
        Self::decrement_fraud_score(&env, &refund.customer);

        Self::update_stats_on_approve(&env, &refund.merchant);

        if is_delegate {
            events::emit_refund_approved_by_delegate(&env, refund_id, admin.clone());
        }
        events::emit_refund_approved(&env, refund_id, admin, refund.approved_at.unwrap());

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Reject a refund request. Callable by admin or a delegate of the refund's merchant (#167).
    pub fn reject_refund(env: Env, admin: Address, refund_id: u32, rejection_reason: String) {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        // #167: Allow admin OR a delegate of the refund's merchant
        let is_admin = admin == stored_admin;
        let is_delegate = !is_admin && Self::is_merchant_delegate(&env, &refund.merchant, &admin);

        if !is_admin && !is_delegate {
            panic!("Only admin or merchant delegate can reject refunds");
        }

        if refund.status != RefundStatus::Requested {
            panic!("Refund is not in requested status");
        }

        let now = env.ledger().timestamp();
        refund.status = RefundStatus::Rejected;
        refund.rejected_at = Some(now);

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // #164: Remove from pending queue
        Self::remove_from_pending_queue(&env, refund_id);

        // Update fraud score: increment on rejected refund (+2 for admin, +1 for delegate)
        if is_admin {
            Self::increment_fraud_score(
                &env,
                &refund.customer,
                Symbol::new(&env, "admin_rejected"),
            );
        } else {
            Self::increment_fraud_score(&env, &refund.customer, Symbol::new(&env, "rejected"));
        }

        // Update abuse score on denial (+10)
        Self::increment_abuse_score(&env, &refund.customer, 10, false);

        Self::update_stats_on_reject(&env, &refund.merchant);

        events::emit_refund_rejected(&env, refund_id, admin, rejection_reason, now);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn process_refund(env: Env, admin: Address, refund_id: u32) {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");

        if admin != stored_admin {
            panic!("Only admin can process refunds");
        }

        Self::process_refund_internal(&env, refund_id);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant initiates an immediate refund directly to customer.
    pub fn merchant_refund(
        env: Env,
        merchant: Address,
        payment_id: u32,
        amount: i128,
        reason_code: u32,
    ) -> u32 {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Refund amount must be positive");
        }

        let payment_contract_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentContractAddress)
            .expect("Payment contract not configured");
        let payment_client =
            payment_contract::PaymentContractClient::new(&env, &payment_contract_addr);
        let payment = payment_client
            .try_get_payment(&payment_id)
            .unwrap_or_else(|_| panic!("PaymentContractError: payment not found"))
            .unwrap_or_else(|_| panic!("PaymentContractError: payment not found"));

        if payment.merchant != merchant {
            panic!("Only payment merchant can initiate refund");
        }

        if amount > payment.amount {
            panic!("ExceedsRefundableAmount");
        }

        let already_refunded: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::RefundedAmount(payment_id))
            .unwrap_or(0);
        let remaining = payment.amount.saturating_sub(already_refunded);
        if amount > remaining {
            panic!("MerchantBalanceInsufficient");
        }

        // Token whitelist validation
        Self::require_token_allowed(&env, &payment.token);

        let token_client = token::Client::new(&env, &payment.token);
        token_client.transfer(&merchant, &payment.customer, &amount);

        let refund_id = Self::next_refund_id(&env);
        let now = env.ledger().timestamp();
        let refund = Refund {
            id: refund_id,
            payment_id,
            customer: payment.customer.clone(),
            merchant: merchant.clone(),
            amount,
            token: payment.token.clone(),
            status: RefundStatus::Processed,
            reason: String::from_str(&env, "merchant_initiated"),
            reason_code: 4, // "Other" — merchant-initiated
            requested_at: now,
            approved_at: Some(now),
            processed_at: Some(now),
            rejected_at: None,
            auto_approved_source: Some(String::from_str(&env, "merchant_direct")),
            escrow_id: None,
            fee_amount: None,
            priority: Priority::Medium,
            merch_response_deadline: 0,
            evidence_submitted: false,
            primary_review_deadline_ledger: 0,
            senior_review_deadline_ledger: 0,
            origin_contract: None,
            auto_approval_deadline_ledger: 0,
            extension_requested: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::append_index(
            &env,
            &DataKey::CustomerRefunds(payment.customer.clone()),
            refund_id,
        );
        Self::append_index(&env, &DataKey::MerchantRefunds(merchant.clone()), refund_id);
        Self::append_index(&env, &DataKey::PaymentRefunds(payment_id), refund_id);

        let new_total = already_refunded + amount;
        env.storage()
            .persistent()
            .set(&DataKey::RefundedAmount(payment_id), &new_total);
        env.storage().persistent().extend_ttl(
            &DataKey::RefundedAmount(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::update_stats_on_request(&env, &merchant, amount);
        Self::update_stats_on_process(&env, &merchant, amount);

        events::emit_merchant_initiated_refund(
            &env,
            refund_id,
            payment_id,
            merchant,
            amount,
            reason_code,
        );

        refund_id
    }

    /// Process up to 20 approved refunds atomically.
    pub fn bulk_process_refunds(env: Env, admin: Address, refund_ids: Vec<u32>) {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can process refunds");
        }

        if refund_ids.is_empty() {
            panic!("Refund batch cannot be empty");
        }
        if refund_ids.len() > 20 {
            panic!("BatchTooLarge");
        }

        for i in 0..refund_ids.len() {
            let rid = refund_ids.get(i).unwrap();
            let refund: Refund = env
                .storage()
                .persistent()
                .get(&DataKey::Refund(rid))
                .expect("Refund not found");
            if refund.status != RefundStatus::Approved {
                panic!("Refund is not approved");
            }
        }

        let mut total_amount: i128 = 0;
        for i in 0..refund_ids.len() {
            let rid = refund_ids.get(i).unwrap();
            let refund: Refund = env
                .storage()
                .persistent()
                .get(&DataKey::Refund(rid))
                .expect("Refund not found");
            total_amount += refund.amount;
            Self::process_refund_internal(&env, rid);
        }

        events::emit_bulk_refund_processed(&env, refund_ids.len(), total_amount);
    }

    // -------------------------------------------------------------------------
    // Store-Credit Voucher Issuance (alternative refund settlement)
    // -------------------------------------------------------------------------

    /// Admin (or merchant delegate) approves a refund as a store-credit voucher instead of
    /// returning tokens. No token transfer occurs; a StoreCredit record is created.
    ///
    /// `credit_amount` may exceed the original refund amount by at most `MAX_VOUCHER_BONUS_BPS`
    /// basis points (admin-configurable, default 10%).
    pub fn approve_refund_as_voucher(
        env: Env,
        admin: Address,
        refund_id: u32,
        credit_amount: i128,
        expiry_ledger: u64,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        // Allow admin OR a delegate of the refund's merchant
        let is_admin = admin == stored_admin;
        let is_delegate = !is_admin && Self::is_merchant_delegate(&env, &refund.merchant, &admin);

        if !is_admin && !is_delegate {
            panic!("Only admin or merchant delegate can approve refunds as voucher");
        }

        if refund.status != RefundStatus::Requested
            && refund.status != RefundStatus::EvidenceSubmitted
            && refund.status != RefundStatus::EvidencePeriodExpired
        {
            panic!("Refund is not in a state that can be approved as voucher");
        }

        if credit_amount <= 0 {
            panic!("credit_amount must be positive");
        }

        // Validate bonus cap
        let max_bonus_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::MaxVoucherBonusBps)
            .unwrap_or(DEFAULT_MAX_VOUCHER_BONUS_BPS);

        let max_credit =
            refund.amount + (refund.amount as u128 * max_bonus_bps as u128 / 10_000) as i128;

        if credit_amount > max_credit {
            panic!("VoucherBonusExceedsCap");
        }

        if expiry_ledger <= env.ledger().sequence() as u64 {
            panic!("expiry_ledger must be in the future");
        }

        // Return the escrowed tokens to the contract (they stay locked — no transfer to customer).
        // The customer's escrowed funds remain in the contract; we simply change the refund status
        // and create the credit record. The escrowed amount is effectively retained by the merchant
        // (the contract holds it on their behalf). Return it to the merchant.
        let client = token::Client::new(&env, &refund.token);
        client.transfer(
            &env.current_contract_address(),
            &refund.merchant,
            &refund.amount,
        );

        // Mark refund as processed (voucher path)
        let now = env.ledger().timestamp();
        refund.status = RefundStatus::Processed;
        refund.approved_at = Some(now);
        refund.processed_at = Some(now);

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Remove from pending queue
        Self::remove_from_pending_queue(&env, refund_id);

        // Create or update the StoreCredit record
        let credit_key = DataKey::StoreCredit(refund.merchant.clone(), refund.customer.clone());
        let existing: Option<StoreCredit> = env.storage().persistent().get(&credit_key);

        let new_credit = if let Some(mut existing_credit) = existing {
            // Accumulate onto existing credit
            existing_credit.credit_amount += credit_amount;
            // Use the later expiry
            if expiry_ledger > existing_credit.expiry_ledger {
                existing_credit.expiry_ledger = expiry_ledger;
            }
            existing_credit
        } else {
            StoreCredit {
                refund_id,
                merchant: refund.merchant.clone(),
                customer: refund.customer.clone(),
                credit_amount,
                expiry_ledger,
                extension_used: false,
            }
        };

        env.storage().persistent().set(&credit_key, &new_credit);
        env.storage().persistent().extend_ttl(
            &credit_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::update_stats_on_approve(&env, &refund.merchant);
        Self::update_stats_on_process(&env, &refund.merchant, refund.amount);

        events::emit_store_credit_issued(
            &env,
            refund_id,
            refund.customer.clone(),
            refund.merchant.clone(),
            credit_amount,
            expiry_ledger,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Apply store credit against a payment at settlement time.
    /// Deducts `credit_amount` from the customer's store-credit balance for the given merchant.
    /// Partial redemption is supported; remaining credit is preserved.
    /// Returns the actual amount of credit applied.
    pub fn apply_store_credit(
        env: Env,
        customer: Address,
        merchant: Address,
        payment_id: u32,
        credit_amount: i128,
    ) -> i128 {
        Self::require_not_paused(&env);
        customer.require_auth();

        if credit_amount <= 0 {
            panic!("credit_amount must be positive");
        }

        let credit_key = DataKey::StoreCredit(merchant.clone(), customer.clone());
        let mut credit: StoreCredit = env
            .storage()
            .persistent()
            .get(&credit_key)
            .expect("StoreCreditNotFound");

        // Check expiry
        if env.ledger().sequence() as u64 > credit.expiry_ledger {
            panic!("StoreCreditExpired");
        }

        if credit.credit_amount <= 0 {
            panic!("StoreCreditExhausted");
        }

        // Clamp to available balance
        let applied = if credit_amount > credit.credit_amount {
            credit.credit_amount
        } else {
            credit_amount
        };

        let remaining = credit.credit_amount - applied;
        credit.credit_amount = remaining;

        env.storage().persistent().set(&credit_key, &credit);
        env.storage().persistent().extend_ttl(
            &credit_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_store_credit_redeemed(
            &env, payment_id, customer, merchant, applied, remaining,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        applied
    }

    /// Merchant extends the expiry of a customer's store-credit record once.
    /// Only one extension is permitted per credit record.
    pub fn extend_store_credit_expiry(
        env: Env,
        merchant: Address,
        customer: Address,
        new_expiry_ledger: u64,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let credit_key = DataKey::StoreCredit(merchant.clone(), customer.clone());
        let mut credit: StoreCredit = env
            .storage()
            .persistent()
            .get(&credit_key)
            .expect("StoreCreditNotFound");

        if credit.merchant != merchant {
            panic!("Only the issuing merchant can extend store credit expiry");
        }

        if credit.extension_used {
            panic!("StoreCreditExtensionAlreadyUsed");
        }

        if new_expiry_ledger <= credit.expiry_ledger {
            panic!("new_expiry_ledger must be later than current expiry");
        }

        credit.expiry_ledger = new_expiry_ledger;
        credit.extension_used = true;

        env.storage().persistent().set(&credit_key, &credit);
        env.storage().persistent().extend_ttl(
            &credit_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the store-credit record for a (merchant, customer) pair.
    pub fn get_store_credit(env: Env, merchant: Address, customer: Address) -> StoreCredit {
        env.storage()
            .persistent()
            .get(&DataKey::StoreCredit(merchant, customer))
            .expect("StoreCreditNotFound")
    }

    /// Admin sets the maximum voucher bonus in basis points (e.g. 500 = 5% above refund amount).
    pub fn set_max_voucher_bonus_bps(env: Env, admin: Address, max_bps: u32) {
        Self::require_admin(&env, &admin);
        if max_bps > 10_000 {
            panic!("max_bps cannot exceed 10000");
        }
        env.storage()
            .instance()
            .set(&DataKey2::MaxVoucherBonusBps, &max_bps);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured max voucher bonus BPS.
    pub fn get_max_voucher_bonus_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey2::MaxVoucherBonusBps)
            .unwrap_or(DEFAULT_MAX_VOUCHER_BONUS_BPS)
    }

    pub fn set_refund_tiers(env: Env, admin: Address, tiers: Vec<(u64, u32)>) {
        Self::require_admin(&env, &admin);
        Self::validate_refund_tiers(&tiers);
        env.storage().instance().set(&DataKey::RefundTiers, &tiers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn get_refund_tiers(env: Env) -> Vec<(u64, u32)> {
        env.storage()
            .instance()
            .get(&DataKey::RefundTiers)
            .unwrap_or(Vec::new(&env))
    }

    /// Auto-approve a refund once the dispute window has elapsed without merchant action.
    /// Callable by anyone. Panics if the merchant has already approved or rejected the refund,
    /// or if the dispute window has not yet elapsed.
    pub fn auto_approve_refund(env: Env, refund_id: u32) {
        Self::require_not_paused(&env);

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Requested {
            panic!("Refund has already been acted on");
        }

        let dispute_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .expect("Dispute window not configured");

        let now = env.ledger().timestamp();
        if now <= refund.requested_at + dispute_window {
            panic!("Dispute window has not elapsed");
        }

        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RefundFeeBps)
            .unwrap_or(0);

        let fee_amount = if fee_bps > 0 {
            (refund.amount as u128 * fee_bps as u128 / 10_000) as i128
        } else {
            0
        };

        let customer_amount = refund.amount - fee_amount;

        let client = token::Client::new(&env, &refund.token);
        if customer_amount > 0 {
            client.transfer(
                &env.current_contract_address(),
                &refund.customer,
                &customer_amount,
            );
        }

        // Transfer fee to fee recipient if configured
        if fee_amount > 0 {
            if let Some(fee_recipient) = env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::FeeRecipient)
            {
                client.transfer(&env.current_contract_address(), &fee_recipient, &fee_amount);
                events::emit_refund_fee_collected(&env, refund_id, fee_amount);
            }
        }

        refund.status = RefundStatus::Processed;
        refund.processed_at = Some(now);
        refund.auto_approved_source = Some(String::from_str(&env, "dispute_window"));
        refund.fee_amount = if fee_amount > 0 {
            Some(fee_amount)
        } else {
            None
        };

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::update_stats_on_process(&env, &refund.merchant, refund.amount);

        events::emit_refund_auto_approved(&env, refund_id, refund.customer, refund.amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Add a merchant to the auto-approval whitelist. Admin only.
    pub fn add_to_auto_approve(env: Env, admin: Address, merchant: Address) {
        Self::require_admin(&env, &admin);

        let mut whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::AutoApprovedMerchants)
            .unwrap_or(Vec::new(&env));

        // Check if already whitelisted
        for addr in whitelist.iter() {
            if addr == merchant {
                panic!("Merchant already whitelisted");
            }
        }

        whitelist.push_back(merchant);
        env.storage()
            .persistent()
            .set(&DataKey::AutoApprovedMerchants, &whitelist);
        env.storage().persistent().extend_ttl(
            &DataKey::AutoApprovedMerchants,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Remove a merchant from the auto-approval whitelist. Admin only.
    pub fn remove_from_auto_approve(env: Env, admin: Address, merchant: Address) {
        Self::require_admin(&env, &admin);

        let whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::AutoApprovedMerchants)
            .unwrap_or(Vec::new(&env));

        let mut found = false;
        let mut new_whitelist = Vec::new(&env);
        for addr in whitelist.iter() {
            if addr != merchant {
                new_whitelist.push_back(addr);
            } else {
                found = true;
            }
        }

        if !found {
            panic!("Merchant not in whitelist");
        }

        env.storage()
            .persistent()
            .set(&DataKey::AutoApprovedMerchants, &new_whitelist);
        env.storage().persistent().extend_ttl(
            &DataKey::AutoApprovedMerchants,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the auto-approval whitelist.
    pub fn get_auto_approved_merchants(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::AutoApprovedMerchants)
            .unwrap_or(Vec::new(&env))
    }

    // -------------------------------------------------------------------------
    // #166: Per-Customer Refund Request Cooldown Period
    // -------------------------------------------------------------------------

    /// Admin waives the cooldown for a specific customer for their next request only.
    /// Removes the LastRefundRequest entry from temporary storage.
    pub fn waive_cooldown(env: Env, admin: Address, customer: Address) {
        Self::require_admin(&env, &admin);
        env.storage()
            .temporary()
            .remove(&DataKey::LastRefundRequest(customer));
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured refund cooldown in seconds.
    pub fn get_refund_cooldown(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::RefundCooldown)
            .unwrap_or(0)
    }

    // -------------------------------------------------------------------------
    // #167: Delegated Refund Approval for Merchant Sub-Admins
    // -------------------------------------------------------------------------

    /// Add a delegate that can approve/reject refunds for a specific merchant.
    /// Maximum 5 delegates per merchant. Only the merchant can call this.
    pub fn add_delegate(env: Env, merchant: Address, delegate: Address) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let key = DataKey::MerchantDelegates(merchant.clone());
        let mut delegates: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));

        if delegates.len() >= 5 {
            panic!("Maximum 5 delegates per merchant");
        }

        for addr in delegates.iter() {
            if addr == delegate {
                panic!("Address is already a delegate");
            }
        }

        delegates.push_back(delegate.clone());
        env.storage().persistent().set(&key, &delegates);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_delegate_added(&env, merchant, delegate);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Remove a delegate from a merchant's delegate list. Only the merchant can call this.
    pub fn remove_delegate(env: Env, merchant: Address, delegate: Address) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let key = DataKey::MerchantDelegates(merchant.clone());
        let delegates: Vec<Address> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));

        let mut new_delegates = Vec::new(&env);
        let mut found = false;
        for addr in delegates.iter() {
            if addr == delegate {
                found = true;
            } else {
                new_delegates.push_back(addr);
            }
        }

        if !found {
            panic!("Address is not a delegate");
        }

        env.storage().persistent().set(&key, &new_delegates);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the list of delegates for a merchant.
    pub fn get_delegates(env: Env, merchant: Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantDelegates(merchant))
            .unwrap_or(Vec::new(&env))
    }

    // -------------------------------------------------------------------------
    // #168: Refund Request Expiry with Auto-Cancellation
    // -------------------------------------------------------------------------

    /// Cancel a refund request. Only callable by the requesting customer while status is Requested.
    /// Returns escrowed funds to the customer.
    pub fn cancel_refund_request(env: Env, customer: Address, refund_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.customer != customer {
            panic!("Only the requesting customer can cancel this refund");
        }

        if refund.status != RefundStatus::Requested {
            panic!("Only Requested refunds can be cancelled");
        }

        // Return escrowed funds to customer
        let client = token::Client::new(&env, &refund.token);
        client.transfer(
            &env.current_contract_address(),
            &refund.customer,
            &refund.amount,
        );

        refund.status = RefundStatus::Cancelled;

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Remove from pending queue
        Self::remove_from_pending_queue(&env, refund_id);

        // Update fraud score: increment if cancelled after dispute window (+1)
        // This penalizes buyers who cancel after the merchant has had time to review
        let dispute_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .unwrap_or(0);
        let now = env.ledger().timestamp();
        if now > refund.requested_at + dispute_window {
            Self::increment_fraud_score(
                &env,
                &refund.customer,
                Symbol::new(&env, "cancelled_late"),
            );
        }

        events::emit_refund_request_cancelled(&env, refund_id, customer);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Auto-cancel a refund request that has been in Requested status beyond
    /// `customer_cancel_window_seconds`. Callable by anyone after the window expires.
    /// Returns escrowed funds to the customer.
    pub fn auto_cancel_expired_request(env: Env, refund_id: u32) {
        Self::require_not_paused(&env);

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Requested {
            panic!("Only Requested refunds can be auto-cancelled");
        }

        let cancel_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CustomerCancelWindow)
            .expect("Customer cancel window not configured");

        let now = env.ledger().timestamp();
        if now <= refund.requested_at + cancel_window {
            panic!("Customer cancel window has not elapsed");
        }

        // Return escrowed funds to customer
        let client = token::Client::new(&env, &refund.token);
        client.transfer(
            &env.current_contract_address(),
            &refund.customer,
            &refund.amount,
        );

        let cancelled_by = refund.customer.clone();
        refund.status = RefundStatus::Cancelled;

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Remove from pending queue
        Self::remove_from_pending_queue(&env, refund_id);

        events::emit_refund_request_cancelled(&env, refund_id, cancelled_by);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured customer cancel window in seconds.
    pub fn get_customer_cancel_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::CustomerCancelWindow)
            .expect("Customer cancel window not configured")
    }

    // -------------------------------------------------------------------------
    // Fraud Score Tracking Functions
    // -------------------------------------------------------------------------

    /// Set the fraud score block threshold. Admin only.
    /// Buyers at or above this threshold cannot submit new refund requests.
    pub fn set_fraud_score_block_threshold(env: Env, admin: Address, threshold: u32) {
        Self::require_admin(&env, &admin);

        if threshold == 0 {
            panic!("Threshold must be positive");
        }

        let old_threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::FraudScoreBlockThreshold)
            .unwrap_or(10);

        env.storage()
            .instance()
            .set(&DataKey::FraudScoreBlockThreshold, &threshold);

        events::emit_fraud_score_block_threshold_updated(&env, old_threshold, threshold);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the current fraud score block threshold.
    pub fn get_fraud_score_block_threshold(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FraudScoreBlockThreshold)
            .unwrap_or(10)
    }

    // -------------------------------------------------------------------------
    // #228: Refund Merchant Auto-Approval Threshold
    // -------------------------------------------------------------------------

    /// Merchant sets their auto-approval threshold.
    /// Refund requests with amount <= threshold are auto-approved (unless fraud score cap blocks).
    /// Set to 0 to disable auto-approval by threshold.
    pub fn set_auto_approve_threshold(env: Env, merchant: Address, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount < 0 {
            panic!("Threshold amount cannot be negative");
        }

        env.storage()
            .instance()
            .set(&DataKey::AutoApproveThreshold(merchant.clone()), &amount);

        events::emit_auto_approve_threshold_set(&env, merchant, amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the auto-approval threshold for a merchant. Returns 0 if not set (disabled).
    pub fn get_auto_approve_threshold(env: Env, merchant: Address) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::AutoApproveThreshold(merchant))
            .unwrap_or(0)
    }

    /// Admin sets the global fraud score cap for auto-approval.
    /// Buyers with fraud_score >= cap are excluded from threshold auto-approval.
    pub fn set_auto_approve_fraud_score_cap(env: Env, admin: Address, cap: u32) {
        Self::require_admin(&env, &admin);

        env.storage()
            .instance()
            .set(&DataKey::AutoApproveFraudScoreCap, &cap);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the fraud score cap for auto-approval. Returns u32::MAX if not set (no cap).
    pub fn get_auto_approve_fraud_score_cap(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::AutoApproveFraudScoreCap)
            .unwrap_or(u32::MAX)
    }

    /// Get the fraud score for a buyer.
    pub fn get_fraud_score(env: Env, buyer: Address) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::FraudScore(buyer))
            .unwrap_or(0)
    }

    /// Manually reset a buyer's fraud score. Admin only.
    pub fn reset_fraud_score(env: Env, admin: Address, buyer: Address) {
        Self::require_admin(&env, &admin);

        let current_score = Self::get_fraud_score(env.clone(), buyer.clone());
        if current_score == 0 {
            return; // Nothing to reset
        }

        env.storage()
            .instance()
            .set(&DataKey::FraudScore(buyer.clone()), &0u32);
        env.storage().instance().set(
            &DataKey::FraudScoreLastUpdate(buyer.clone()),
            &env.ledger().timestamp(),
        );

        events::emit_fraud_score_reset(&env, buyer, admin);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Apply time-based decay to a buyer's fraud score.
    /// Called internally when the decay interval has elapsed.
    fn apply_fraud_score_decay(env: &Env, buyer: &Address) {
        let decay_interval: u64 = env
            .storage()
            .instance()
            .get(&DataKey::FraudScoreDecayInterval)
            .unwrap_or(30 * 24 * 60 * 60); // default 30 days

        let last_update: u64 = env
            .storage()
            .instance()
            .get(&DataKey::FraudScoreLastUpdate(buyer.clone()))
            .unwrap_or(0);

        if last_update == 0 {
            return; // No score to decay
        }

        let now = env.ledger().timestamp();
        if now - last_update < decay_interval {
            return; // Not enough time has passed
        }

        let current_score = Self::get_fraud_score(env.clone(), buyer.clone());
        if current_score == 0 {
            return;
        }

        // Decay by 1 point
        let new_score = current_score.saturating_sub(1);
        env.storage()
            .instance()
            .set(&DataKey::FraudScore(buyer.clone()), &new_score);
        env.storage()
            .instance()
            .set(&DataKey::FraudScoreLastUpdate(buyer.clone()), &now);

        events::emit_fraud_score_decay_applied(&env, buyer.clone(), current_score, new_score);
    }

    /// Increment fraud score for a buyer. Called on specific events.
    fn increment_fraud_score(env: &Env, buyer: &Address, reason: Symbol) {
        // Apply decay first
        Self::apply_fraud_score_decay(env, buyer);

        let current_score = Self::get_fraud_score(env.clone(), buyer.clone());
        let new_score = current_score.saturating_add(1);

        env.storage()
            .instance()
            .set(&DataKey::FraudScore(buyer.clone()), &new_score);
        env.storage().instance().set(
            &DataKey::FraudScoreLastUpdate(buyer.clone()),
            &env.ledger().timestamp(),
        );

        events::emit_fraud_score_updated(&env, buyer.clone(), new_score, reason.clone());

        // Check if buyer should be blocked
        let threshold = Self::get_fraud_score_block_threshold(env.clone());
        if new_score >= threshold {
            events::emit_buyer_blocked_for_fraud(&env, buyer.clone(), new_score, threshold);
        }
    }

    /// Decrement fraud score for a buyer. Called on positive events.
    fn decrement_fraud_score(env: &Env, buyer: &Address) {
        // Apply decay first
        Self::apply_fraud_score_decay(env, buyer);

        let current_score = Self::get_fraud_score(env.clone(), buyer.clone());
        if current_score == 0 {
            return;
        }

        let new_score = current_score.saturating_sub(1);
        env.storage()
            .instance()
            .set(&DataKey::FraudScore(buyer.clone()), &new_score);
        env.storage().instance().set(
            &DataKey::FraudScoreLastUpdate(buyer.clone()),
            &env.ledger().timestamp(),
        );

        events::emit_fraud_score_updated(
            &env,
            buyer.clone(),
            new_score,
            Symbol::new(env, "approved"),
        );
    }

    /// Check if a buyer is blocked from requesting refunds.
    fn is_buyer_blocked(env: &Env, buyer: &Address) -> bool {
        let score = Self::get_fraud_score(env.clone(), buyer.clone());
        let threshold = Self::get_fraud_score_block_threshold(env.clone());
        score >= threshold
    }

    // -------------------------------------------------------------------------
    // #157: Structured Refund Reason Codes
    // -------------------------------------------------------------------------

    /// Get refund IDs for a merchant filtered by reason code, with pagination.
    /// reason_code: 0=Defective, 1=NotDelivered, 2=DuplicateCharge, 3=Unauthorized, 4=Other
    pub fn get_refunds_by_reason(
        env: Env,
        merchant: Address,
        reason_code: u32,
        page: u32,
        page_size: u32,
    ) -> Vec<u32> {
        if reason_code > 4 {
            panic!("Invalid reason code: must be 0-4");
        }
        let key = DataKey::ReasonCodeRefunds(merchant, reason_code);
        let all: Vec<u32> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(&env));
        let total = all.len();
        let start = (page * page_size).min(total);
        let end = (start + page_size).min(total);
        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(all.get(i).unwrap());
        }
        result
    }

    /// Get the count of refunds for a merchant + reason_code combination.
    pub fn get_reason_code_count(env: Env, merchant: Address, reason_code: u32) -> u32 {
        if reason_code > 4 {
            panic!("Invalid reason code: must be 0-4");
        }
        env.storage()
            .persistent()
            .get(&DataKey::ReasonCodeCount(merchant, reason_code))
            .unwrap_or(0)
    }

    // -------------------------------------------------------------------------
    // #158: Auto-Reject Stale Refund Requests
    // -------------------------------------------------------------------------

    /// Auto-reject a refund that has been sitting in Requested status beyond the idle window.
    /// Callable by anyone. Returns escrowed funds to customer.
    pub fn auto_reject_stale_refund(env: Env, refund_id: u32) {
        Self::require_not_paused(&env);

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Requested {
            panic!("Refund is not in requested status");
        }

        let auto_reject_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AutoRejectWindow)
            .expect("Auto-reject window not configured");

        let extension: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::RefundDeadlineExtension(refund_id))
            .unwrap_or(0);

        let deadline = refund.requested_at + auto_reject_window + extension;
        let now = env.ledger().timestamp();

        if now <= deadline {
            panic!("Auto-reject window has not elapsed");
        }

        let elapsed_seconds = now - refund.requested_at;

        // Return escrowed funds to customer
        let client = token::Client::new(&env, &refund.token);
        client.transfer(
            &env.current_contract_address(),
            &refund.customer,
            &refund.amount,
        );

        refund.status = RefundStatus::Rejected;
        refund.rejected_at = Some(now);

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // #164: Remove from pending queue
        Self::remove_from_pending_queue(&env, refund_id);

        // Update fraud score: increment on auto-rejected refund (+1)
        Self::increment_fraud_score(&env, &refund.customer, Symbol::new(&env, "auto_rejected"));

        Self::update_stats_on_reject(&env, &refund.merchant);

        events::emit_refund_auto_rejected(&env, refund_id, elapsed_seconds);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Extend the auto-reject deadline for a specific refund. Admin only.
    pub fn extend_refund_deadline(env: Env, admin: Address, refund_id: u32, extra_seconds: u64) {
        Self::require_admin(&env, &admin);

        let refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Requested {
            panic!("Refund is not in requested status");
        }

        let key = DataKey::RefundDeadlineExtension(refund_id);
        let current_extension: u64 = env.storage().persistent().get(&key).unwrap_or(0);

        env.storage()
            .persistent()
            .set(&key, &(current_extension + extra_seconds));
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured auto-reject window in seconds.
    pub fn get_auto_reject_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::AutoRejectWindow)
            .expect("Auto-reject window not configured")
    }

    // -------------------------------------------------------------------------
    // #159: Customer Refund Appeal After Merchant Rejection
    // -------------------------------------------------------------------------

    /// Appeal a rejected refund. Only the original customer can call this,
    /// within the configured appeal window after rejection.
    pub fn appeal_refund(env: Env, customer: Address, refund_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Rejected {
            panic!("Appeal only allowed from Rejected status");
        }

        if refund.customer != customer {
            panic!("Only the original customer can appeal");
        }

        let appeal_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::AppealWindow)
            .expect("Appeal window not configured");

        let rejected_at = refund.rejected_at.expect("Rejection timestamp missing");
        let now = env.ledger().timestamp();

        if now > rejected_at + appeal_window {
            panic!("Appeal window has expired");
        }

        refund.status = RefundStatus::UnderAppeal;

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_refund_appealed(&env, refund_id, customer);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Resolve a refund appeal. Admin only.
    /// If approved: transfers funds to customer (processed).
    /// If rejected: final rejection, funds returned to customer, no further appeal.
    pub fn resolve_appeal(env: Env, admin: Address, refund_id: u32, approve: bool) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::UnderAppeal {
            panic!("Refund is not under appeal");
        }

        let now = env.ledger().timestamp();
        let client = token::Client::new(&env, &refund.token);

        if approve {
            let fee_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::RefundFeeBps)
                .unwrap_or(0);

            let fee_amount = if fee_bps > 0 {
                (refund.amount as u128 * fee_bps as u128 / 10_000) as i128
            } else {
                0
            };

            let customer_amount = refund.amount - fee_amount;

            if customer_amount > 0 {
                client.transfer(
                    &env.current_contract_address(),
                    &refund.customer,
                    &customer_amount,
                );
            }

            if fee_amount > 0 {
                if let Some(fee_recipient) = env
                    .storage()
                    .instance()
                    .get::<DataKey, Address>(&DataKey::FeeRecipient)
                {
                    client.transfer(&env.current_contract_address(), &fee_recipient, &fee_amount);
                    events::emit_refund_fee_collected(&env, refund_id, fee_amount);
                }
            }

            // Update cumulative refunded amount
            let already_refunded: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::RefundedAmount(refund.payment_id))
                .unwrap_or(0);
            let new_total = already_refunded + refund.amount;
            env.storage()
                .persistent()
                .set(&DataKey::RefundedAmount(refund.payment_id), &new_total);
            env.storage().persistent().extend_ttl(
                &DataKey::RefundedAmount(refund.payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

            refund.status = RefundStatus::Processed;
            refund.processed_at = Some(now);
            refund.fee_amount = if fee_amount > 0 {
                Some(fee_amount)
            } else {
                None
            };

            Self::update_stats_on_process(&env, &refund.merchant, refund.amount);
        } else {
            // Final rejection — return escrowed funds to customer
            client.transfer(
                &env.current_contract_address(),
                &refund.customer,
                &refund.amount,
            );

            refund.status = RefundStatus::Rejected;
            refund.rejected_at = Some(now);

            Self::update_stats_on_reject(&env, &refund.merchant);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_appeal_resolved(&env, refund_id, approve);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured appeal window in seconds.
    pub fn get_appeal_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::AppealWindow)
            .expect("Appeal window not configured")
    }

    // -------------------------------------------------------------------------
    // #164: Paginated Admin Refund Queue View
    // -------------------------------------------------------------------------

    /// Get paginated list of pending (Requested) refund IDs.
    /// page_size is capped at 50. Returns (refund_ids, total, has_more).
    pub fn get_pending_refund_queue(env: Env, page: u32, page_size: u32) -> (Vec<u32>, u32, bool) {
        let effective_size = page_size.min(50);
        let queue: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingRefundQueue)
            .unwrap_or(Vec::new(&env));
        let total = queue.len();
        let start = (page * effective_size).min(total);
        let end = (start + effective_size).min(total);
        let mut result = Vec::new(&env);
        for i in start..end {
            result.push_back(queue.get(i).unwrap());
        }
        let has_more = end < total;
        (result, total, has_more)
    }

    /// Get the count of pending (Requested) refunds in the queue.
    pub fn get_pending_refund_count(env: Env) -> u32 {
        let queue: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingRefundQueue)
            .unwrap_or(Vec::new(&env));
        queue.len()
    }

    /// Register a refund from the escrow contract. Only callable by the configured escrow contract.
    /// Creates a refund record in Processed status (no approval needed).
    pub fn register_escrow_refund(
        env: Env,
        escrow_id: u32,
        buyer: Address,
        amount: i128,
        token: Address,
    ) -> u32 {
        Self::require_not_paused(&env);

        // Verify caller is the configured escrow contract
        let escrow_contract_addr: Option<Address> = env
            .storage()
            .instance()
            .get(&DataKey::EscrowContractAddress);

        if let Some(escrow_addr) = escrow_contract_addr {
            if env.current_contract_address() != escrow_addr {
                panic!("Only escrow contract can register escrow refunds");
            }
        } else {
            panic!("Escrow contract not configured");
        }

        if amount <= 0 {
            panic!("Refund amount must be positive");
        }

        let refund_id = Self::next_refund_id(&env);
        let now = env.ledger().timestamp();

        // Use buyer as merchant placeholder for escrow refunds
        let merchant = buyer.clone();

        let refund = Refund {
            id: refund_id,
            payment_id: 0, // No payment_id for escrow refunds
            customer: buyer.clone(),
            merchant: merchant.clone(),
            amount,
            token: token.clone(),
            status: RefundStatus::Processed,
            reason: String::from_str(&env, "escrow_refund"),
            reason_code: 4, // Other
            requested_at: now,
            approved_at: Some(now),
            processed_at: Some(now),
            rejected_at: None,
            auto_approved_source: None,
            escrow_id: Some(escrow_id),
            fee_amount: None,
            priority: Priority::Medium,
            merch_response_deadline: 0,
            evidence_submitted: false,
            primary_review_deadline_ledger: 0,
            senior_review_deadline_ledger: 0,
            origin_contract: None,
            auto_approval_deadline_ledger: 0,
            extension_requested: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::append_index(&env, &DataKey::CustomerRefunds(buyer.clone()), refund_id);

        events::emit_escrow_refund_registered(&env, refund_id, escrow_id, buyer, amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        refund_id
    }

    /// Get global refund statistics.
    pub fn get_global_refund_stats(env: &Env) -> RefundStats {
        env.storage()
            .instance()
            .get(&DataKey::GlobalRefundStats)
            .unwrap_or(RefundStats {
                total_requested: 0,
                total_approved: 0,
                total_rejected: 0,
                total_processed: 0,
                total_amount_refunded: 0,
            })
    }

    /// Get per-merchant refund statistics.
    pub fn get_merchant_refund_stats(env: &Env, merchant: Address) -> RefundStats {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantRefundStats(merchant))
            .unwrap_or(RefundStats {
                total_requested: 0,
                total_approved: 0,
                total_rejected: 0,
                total_processed: 0,
                total_amount_refunded: 0,
            })
    }

    /// Get refund statistics scoped to a single merchant (#584). Alias of
    /// `get_merchant_refund_stats` under the name used by merchant dashboards
    /// and buyer-trust-tier/abuse-scoring consumers.
    pub fn get_refund_stats_by_merchant(env: &Env, merchant: Address) -> RefundStats {
        Self::get_merchant_refund_stats(env, merchant)
    }

    /// Update the refund fee in basis points. Admin only. Max 200 bps (2%).
    pub fn update_refund_fee(env: Env, admin: Address, new_fee_bps: u32) {
        Self::require_admin(&env, &admin);

        if new_fee_bps > 200 {
            panic!("Refund fee cannot exceed 200 basis points (2%)");
        }

        env.storage()
            .instance()
            .set(&DataKey::RefundFeeBps, &new_fee_bps);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the current refund fee in basis points.
    pub fn get_refund_fee(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RefundFeeBps)
            .unwrap_or(0)
    }

    /// Get the fee recipient address.
    pub fn get_fee_recipient(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::FeeRecipient)
    }

    /// Get the configured dispute window in seconds.
    pub fn get_dispute_window(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::DisputeWindow)
            .expect("Dispute window not configured")
    }

    /// Admin sets the dispute window in seconds. Rejects values below
    /// `MIN_DISPUTE_WINDOW_SECONDS` (#582), which would otherwise let
    /// `auto_approve_refund` bypass the intended merchant review period.
    pub fn set_dispute_window(env: Env, admin: Address, dispute_window: u64) {
        Self::require_admin(&env, &admin);
        if dispute_window < MIN_DISPUTE_WINDOW_SECONDS {
            panic!("DisputeWindowBelowMinimum");
        }
        env.storage()
            .instance()
            .set(&DataKey::DisputeWindow, &dispute_window);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get refund details
    pub fn get_refund(env: Env, refund_id: u32) -> Refund {
        env.storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found")
    }

    /// Get refunds by customer with pagination.
    pub fn get_refunds_by_customer(
        env: Env,
        customer: Address,
        limit: u32,
        offset: u32,
    ) -> Vec<u32> {
        Self::paginate(&env, &DataKey::CustomerRefunds(customer), limit, offset)
    }

    /// Get refunds by merchant with pagination.
    pub fn get_refunds_by_merchant(
        env: Env,
        merchant: Address,
        limit: u32,
        offset: u32,
    ) -> Vec<u32> {
        Self::paginate(&env, &DataKey::MerchantRefunds(merchant), limit, offset)
    }

    /// Get the remaining refundable amount for a payment.
    pub fn get_refundable_remaining(env: Env, payment_id: u32) -> i128 {
        let payment_contract_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentContractAddress)
            .expect("Payment contract not configured");
        let payment_client =
            payment_contract::PaymentContractClient::new(&env, &payment_contract_addr);
        let payment = payment_client
            .try_get_payment(&payment_id)
            .unwrap_or_else(|_| panic!("PaymentContractError: payment not found"))
            .unwrap_or_else(|_| panic!("PaymentContractError: payment not found"));
        let already_refunded: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::RefundedAmount(payment_id))
            .unwrap_or(0);
        payment.amount - already_refunded
    }

    /// Get all refund IDs for a given payment ID.
    pub fn get_refunds_by_payment(env: Env, payment_id: u32) -> Vec<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentRefunds(payment_id))
            .unwrap_or(Vec::new(&env))
    }

    /// Get refund counter
    pub fn get_refund_counter(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::RefundCounter)
            .unwrap_or(0)
    }

    /// Get admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized")
    }

    /// Get the configured payment contract address (#64).
    pub fn get_payment_contract(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::PaymentContractAddress)
            .expect("Payment contract not configured")
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

    /// Validates that a token is allowed via the whitelist contract
    fn require_token_allowed(env: &Env, token: &Address) {
        if let Some(whitelist_contract) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::TokenWhitelistContract)
        {
            let client = TokenWhitelistClient::new(env, &whitelist_contract);
            if !client.is_token_allowed_for_contract(&env.current_contract_address(), token) {
                panic!("TokenNotAllowed");
            }
        }
    }

    fn append_index(env: &Env, key: &DataKey, refund_id: u32) {
        let mut ids: Vec<u32> = env.storage().persistent().get(key).unwrap_or(Vec::new(env));
        ids.push_back(refund_id);
        env.storage().persistent().set(key, &ids);
        env.storage().persistent().extend_ttl(
            key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn paginate(env: &Env, key: &DataKey, limit: u32, offset: u32) -> Vec<u32> {
        let all: Vec<u32> = env.storage().persistent().get(key).unwrap_or(Vec::new(env));
        let total = all.len();
        let start = offset.min(total);
        let end = (start + limit).min(total);
        let mut page = Vec::new(env);
        for i in start..end {
            page.push_back(all.get(i).unwrap());
        }
        page
    }

    fn next_refund_id(env: &Env) -> u32 {
        let mut counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RefundCounter)
            .unwrap_or(0);
        let id = counter;
        counter += 1;
        env.storage()
            .instance()
            .set(&DataKey::RefundCounter, &counter);
        id
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

    // --- Helper Functions for Stats and Whitelist ---

    /// Check if `caller` is a registered delegate for `merchant` (#167).
    fn is_merchant_delegate(env: &Env, merchant: &Address, caller: &Address) -> bool {
        let delegates: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantDelegates(merchant.clone()))
            .unwrap_or(Vec::new(env));
        for addr in delegates.iter() {
            if addr == *caller {
                return true;
            }
        }
        false
    }

    fn is_merchant_auto_approved(env: &Env, merchant: &Address) -> bool {
        let whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::AutoApprovedMerchants)
            .unwrap_or(Vec::new(env));

        for addr in whitelist.iter() {
            if addr == *merchant {
                return true;
            }
        }
        false
    }

    fn update_stats_on_request(env: &Env, merchant: &Address, _amount: i128) {
        // Update global stats
        let mut global_stats = Self::get_global_refund_stats(env);
        global_stats.total_requested += 1;
        env.storage()
            .instance()
            .set(&DataKey::GlobalRefundStats, &global_stats);

        // Update merchant stats
        let mut merchant_stats = Self::get_merchant_refund_stats(env, merchant.clone());
        merchant_stats.total_requested += 1;
        env.storage().persistent().set(
            &DataKey::MerchantRefundStats(merchant.clone()),
            &merchant_stats,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::MerchantRefundStats(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn update_stats_on_approve(env: &Env, merchant: &Address) {
        // Update global stats
        let mut global_stats = Self::get_global_refund_stats(env);
        global_stats.total_approved += 1;
        env.storage()
            .instance()
            .set(&DataKey::GlobalRefundStats, &global_stats);

        // Update merchant stats
        let mut merchant_stats = Self::get_merchant_refund_stats(env, merchant.clone());
        merchant_stats.total_approved += 1;
        env.storage().persistent().set(
            &DataKey::MerchantRefundStats(merchant.clone()),
            &merchant_stats,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::MerchantRefundStats(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn update_stats_on_reject(env: &Env, merchant: &Address) {
        // Update global stats
        let mut global_stats = Self::get_global_refund_stats(env);
        global_stats.total_rejected += 1;
        env.storage()
            .instance()
            .set(&DataKey::GlobalRefundStats, &global_stats);

        // Update merchant stats
        let mut merchant_stats = Self::get_merchant_refund_stats(env, merchant.clone());
        merchant_stats.total_rejected += 1;
        env.storage().persistent().set(
            &DataKey::MerchantRefundStats(merchant.clone()),
            &merchant_stats,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::MerchantRefundStats(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn append_to_pending_queue(env: &Env, refund_id: u32) {
        let mut queue: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingRefundQueue)
            .unwrap_or(Vec::new(env));
        queue.push_back(refund_id);
        env.storage()
            .persistent()
            .set(&DataKey::PendingRefundQueue, &queue);
        env.storage().persistent().extend_ttl(
            &DataKey::PendingRefundQueue,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn remove_from_pending_queue(env: &Env, refund_id: u32) {
        let queue: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingRefundQueue)
            .unwrap_or(Vec::new(env));
        let mut new_queue = Vec::new(env);
        for id in queue.iter() {
            if id != refund_id {
                new_queue.push_back(id);
            }
        }
        env.storage()
            .persistent()
            .set(&DataKey::PendingRefundQueue, &new_queue);
        env.storage().persistent().extend_ttl(
            &DataKey::PendingRefundQueue,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn update_stats_on_process(env: &Env, merchant: &Address, amount: i128) {
        // Update global stats
        let mut global_stats = Self::get_global_refund_stats(env);
        global_stats.total_processed += 1;
        global_stats.total_amount_refunded += amount;
        env.storage()
            .instance()
            .set(&DataKey::GlobalRefundStats, &global_stats);

        // Update merchant stats
        let mut merchant_stats = Self::get_merchant_refund_stats(env, merchant.clone());
        merchant_stats.total_processed += 1;
        merchant_stats.total_amount_refunded += amount;
        env.storage().persistent().set(
            &DataKey::MerchantRefundStats(merchant.clone()),
            &merchant_stats,
        );
        env.storage().persistent().extend_ttl(
            &DataKey::MerchantRefundStats(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn validate_refund_tiers(tiers: &Vec<(u64, u32)>) {
        let mut prev_max = 0u64;
        for i in 0..tiers.len() {
            let (max_seconds_since_payment, refund_bps) = tiers.get(i).unwrap();
            if i > 0 && max_seconds_since_payment < prev_max {
                panic!("Refund tiers must be sorted by max_seconds_since_payment");
            }
            if refund_bps > 10_000 {
                panic!("Refund tier bps must be <= 10000");
            }
            prev_max = max_seconds_since_payment;
        }
    }

    fn process_refund_internal(env: &Env, refund_id: u32) {
        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Approved {
            panic!("Refund is not approved");
        }

        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RefundFeeBps)
            .unwrap_or(0);

        let fee_amount = if fee_bps > 0 {
            (refund.amount as u128 * fee_bps as u128 / 10_000) as i128
        } else {
            0
        };

        let customer_amount = refund.amount - fee_amount;

        let client = token::Client::new(env, &refund.token);
        if customer_amount > 0 {
            client.transfer(
                &env.current_contract_address(),
                &refund.customer,
                &customer_amount,
            );
        }

        if fee_amount > 0 {
            if let Some(fee_recipient) = env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::FeeRecipient)
            {
                client.transfer(&env.current_contract_address(), &fee_recipient, &fee_amount);
                events::emit_refund_fee_collected(env, refund_id, fee_amount);
            }
        }

        let already_refunded: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::RefundedAmount(refund.payment_id))
            .unwrap_or(0);
        let new_total = already_refunded + refund.amount;
        env.storage()
            .persistent()
            .set(&DataKey::RefundedAmount(refund.payment_id), &new_total);
        env.storage().persistent().extend_ttl(
            &DataKey::RefundedAmount(refund.payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // #349: Also update the dedicated PaymentRefundedAmount key and emit partial refund event
        let prev_processed: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::PaymentRefundedAmount(refund.payment_id))
            .unwrap_or(0);
        let new_processed_total = prev_processed + refund.amount;
        env.storage()
            .persistent()
            .set(&DataKey::PaymentRefundedAmount(refund.payment_id), &new_processed_total);
        env.storage().persistent().extend_ttl(
            &DataKey::PaymentRefundedAmount(refund.payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        let payment_contract_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::PaymentContractAddress)
            .expect("Payment contract not configured");
        let payment_client =
            payment_contract::PaymentContractClient::new(env, &payment_contract_addr);

        let remaining_amount = if let Ok(Ok(payment)) = payment_client.try_get_payment(&refund.payment_id) {
            let remaining_refundable = payment.amount - new_total;
            if remaining_refundable <= payment.amount / 10 {
                events::emit_partial_refund_cap_applied(env, refund_id, remaining_refundable);
            }
            remaining_refundable
        } else {
            0
        };

        // #349: Emit PartialRefundProcessed with cumulative tracking
        events::emit_partial_refund_processed(env, refund.payment_id, refund.amount, remaining_amount);

        refund.status = RefundStatus::Processed;
        refund.processed_at = Some(env.ledger().timestamp());
        refund.fee_amount = if fee_amount > 0 {
            Some(fee_amount)
        } else {
            None
        };

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::update_stats_on_process(env, &refund.merchant, refund.amount);

        events::emit_refund_processed(
            env,
            refund_id,
            refund.customer,
            customer_amount,
            refund.processed_at.unwrap(),
        );
    }

    // --- Counter-Offer Negotiation ---

    /// Admin configures the default resolution when a counter-offer expires.
    /// `accept` = true means the original refund amount is paid out; false means the refund is rejected.
    pub fn set_counter_offer_resolution(env: Env, admin: Address, accept: bool) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can configure counter-offer default resolution");
        }
        env.storage()
            .instance()
            .set(&DataKey::CounterOfferDefaultResolution, &accept);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Settle an expired counter-offer. Callable by anyone after the expiry timestamp.
    /// Default behaviour: accept the original refund amount (configurable by admin).
    pub fn settle_expired_counter_offer(env: Env, refund_id: u32) {
        Self::require_not_paused(&env);

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::CounterOffered {
            panic!("Refund is not in CounterOffered state");
        }

        let offer: CounterOffer = env
            .storage()
            .persistent()
            .get(&DataKey::CounterOffer(refund_id))
            .expect("Counter-offer not found");

        let now = env.ledger().timestamp();
        if now <= offer.expiry {
            panic!("Counter-offer has not expired yet");
        }

        let accept_on_expiry: bool = env
            .storage()
            .instance()
            .get(&DataKey::CounterOfferDefaultResolution)
            .unwrap_or(DEFAULT_COUNTER_OFFER_ACCEPT_ON_EXPIRY);

        let original_amount = refund.amount;
        let client = token::Client::new(&env, &refund.token);

        if accept_on_expiry {
            // Pay out the original refund amount to the customer
            client.transfer(
                &env.current_contract_address(),
                &refund.customer,
                &original_amount,
            );
            refund.status = RefundStatus::Processed;
            refund.processed_at = Some(now);
            Self::update_stats_on_process(&env, &refund.merchant, original_amount);
        } else {
            // Return escrowed funds to customer and reject
            client.transfer(
                &env.current_contract_address(),
                &refund.customer,
                &original_amount,
            );
            refund.status = RefundStatus::Rejected;
            refund.rejected_at = Some(now);
            Self::update_stats_on_reject(&env, &refund.merchant);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::CounterOffer(refund_id));

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::remove_from_pending_queue(&env, refund_id);

        events::emit_counter_offer_expired(&env, refund_id, accept_on_expiry, original_amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin sets the counter-offer expiry window in seconds.
    pub fn set_counter_offer_expiry_seconds(env: Env, admin: Address, seconds: u64) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can configure counter-offer expiry");
        }
        if seconds == 0 {
            panic!("Expiry must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::CounterOfferExpirySeconds, &seconds);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant submits a counter-offer (partial amount) for a refund request.
    /// Only one counter-offer is permitted per refund.
    pub fn counter_offer_refund(env: Env, merchant: Address, refund_id: u32, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Counter-offer amount must be positive");
        }

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Requested {
            panic!("Refund is not in Requested state");
        }

        if merchant != refund.merchant {
            panic!("Only the merchant can make a counter-offer");
        }

        if amount > refund.amount {
            panic!("Counter-offer cannot exceed original refund amount");
        }

        // Prevent duplicate counter-offers
        if env
            .storage()
            .persistent()
            .has(&DataKey::CounterOffer(refund_id))
        {
            panic!("Counter-offer already submitted for this refund");
        }

        let expiry_seconds: u64 = env
            .storage()
            .instance()
            .get(&DataKey::CounterOfferExpirySeconds)
            .unwrap_or(DEFAULT_COUNTER_OFFER_EXPIRY_SECONDS);

        let now = env.ledger().timestamp();
        let offer = CounterOffer {
            amount,
            offered_at: now,
            expiry: now + expiry_seconds,
        };

        env.storage()
            .persistent()
            .set(&DataKey::CounterOffer(refund_id), &offer);
        env.storage().persistent().extend_ttl(
            &DataKey::CounterOffer(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        refund.status = RefundStatus::CounterOffered;
        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_refund_counter_offered(&env, refund_id, merchant, amount, offer.expiry);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Customer accepts the counter-offer; the counter-offer amount is released immediately.
    pub fn accept_counter_offer(env: Env, customer: Address, refund_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::CounterOffered {
            panic!("Refund is not in CounterOffered state");
        }

        if customer != refund.customer {
            panic!("Only the customer can accept a counter-offer");
        }

        let offer: CounterOffer = env
            .storage()
            .persistent()
            .get(&DataKey::CounterOffer(refund_id))
            .expect("Counter-offer not found");

        let now = env.ledger().timestamp();
        if now > offer.expiry {
            // Expired — auto-escalate to admin
            Self::escalate_counter_offer_to_admin(&env, refund_id, &mut refund);
            return;
        }

        // Transfer counter-offer amount to customer
        let client = token::Client::new(&env, &refund.token);
        client.transfer(
            &env.current_contract_address(),
            &refund.customer,
            &offer.amount,
        );

        refund.status = RefundStatus::Processed;
        refund.processed_at = Some(now);
        refund.fee_amount = None;

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .persistent()
            .remove(&DataKey::CounterOffer(refund_id));

        events::emit_refund_counter_accepted(&env, refund_id, customer, offer.amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Customer rejects the counter-offer; escalates to admin review.
    pub fn reject_counter_offer(env: Env, customer: Address, refund_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::CounterOffered {
            panic!("Refund is not in CounterOffered state");
        }

        if customer != refund.customer {
            panic!("Only the customer can reject a counter-offer");
        }

        env.storage()
            .persistent()
            .remove(&DataKey::CounterOffer(refund_id));

        Self::escalate_counter_offer_to_admin(&env, refund_id, &mut refund);

        events::emit_refund_counter_rejected(&env, refund_id, customer);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Check if a counter-offer has expired and auto-escalate if so.
    /// Anyone can call this to trigger expiry escalation.
    pub fn check_counter_offer_expiry(env: Env, refund_id: u32) {
        Self::require_not_paused(&env);

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::CounterOffered {
            panic!("Refund is not in CounterOffered state");
        }

        let offer: CounterOffer = env
            .storage()
            .persistent()
            .get(&DataKey::CounterOffer(refund_id))
            .expect("Counter-offer not found");

        if env.ledger().timestamp() <= offer.expiry {
            panic!("Counter-offer has not expired yet");
        }

        env.storage()
            .persistent()
            .remove(&DataKey::CounterOffer(refund_id));
        Self::escalate_counter_offer_to_admin(&env, refund_id, &mut refund);
    }

    fn escalate_counter_offer_to_admin(env: &Env, refund_id: u32, refund: &mut Refund) {
        refund.status = RefundStatus::UnderAppeal;
        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    // --- Issue #238: Refund Priority Labelling ---

    /// Admin overrides the priority of any pending refund request.
    pub fn set_refund_priority(env: Env, admin: Address, refund_id: u32, priority: Priority) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");
        if refund.status != RefundStatus::Requested {
            panic!("Can only set priority on Requested refunds");
        }
        refund.priority = priority;
        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_refund_priority_set(&env, refund_id, priority as u32, admin);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // ─── #217: Batch Refund Processing ───────────────────────────────────────

    /// Admin sets the maximum batch size for batch refund operations.
    pub fn set_max_batch_size(env: Env, admin: Address, max_batch_size: u32) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can set max batch size");
        }
        if max_batch_size == 0 {
            panic!("Max batch size must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxBatchSize, &max_batch_size);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Returns pending refunds sorted: High first, then Medium, then Low; within same priority oldest first.
    /// Returns (page_items, total_pending, has_more).
    pub fn get_admin_refund_queue(env: Env, page: u32, page_size: u32) -> (Vec<Refund>, u32, bool) {
        let effective_size = page_size.min(50);
        let queue: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey::PendingRefundQueue)
            .unwrap_or(Vec::new(&env));

        // Collect (priority_ord, requested_at, refund_id) for sorting
        // priority_ord: High=0, Medium=1, Low=2
        let mut entries: Vec<(u32, u64, u32)> = Vec::new(&env);
        for i in 0..queue.len() {
            let rid = queue.get(i).unwrap();
            if let Some(r) = env
                .storage()
                .persistent()
                .get::<DataKey, Refund>(&DataKey::Refund(rid))
            {
                if r.status == RefundStatus::Requested {
                    let pord = r.priority as u32;
                    entries.push_back((pord, r.requested_at, rid));
                }
            }
        }

        // Insertion sort by (priority_ord ASC, requested_at ASC)
        let n = entries.len();
        for i in 1..n {
            let mut j = i;
            while j > 0 {
                let a = entries.get(j - 1).unwrap();
                let b = entries.get(j).unwrap();
                if a.0 > b.0 || (a.0 == b.0 && a.1 > b.1) {
                    entries.set(j - 1, b);
                    entries.set(j, a);
                    j -= 1;
                } else {
                    break;
                }
            }
        }

        let total = entries.len();
        let start = (page * effective_size).min(total);
        let end = (start + effective_size).min(total);
        let mut result: Vec<Refund> = Vec::new(&env);
        for i in start..end {
            let (_, _, rid) = entries.get(i).unwrap();
            if let Some(r) = env
                .storage()
                .persistent()
                .get::<DataKey, Refund>(&DataKey::Refund(rid))
            {
                result.push_back(r);
            }
        }
        let has_more = end < total;
        (result, total, has_more)
    }

    /// Approve multiple pending refund requests in a single transaction.
    /// Skips IDs that are not in Requested status rather than reverting.
    /// Returns a BatchRefundResult with processed and skipped IDs.
    pub fn batch_approve_refunds(
        env: Env,
        admin: Address,
        refund_ids: Vec<u32>,
    ) -> BatchRefundResult {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can batch approve refunds");
        }

        let max_batch: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxBatchSize)
            .unwrap_or(DEFAULT_MAX_BATCH_SIZE);

        if refund_ids.len() > max_batch {
            panic!("Batch size exceeds maximum allowed");
        }

        let mut processed = Vec::new(&env);
        let mut skipped = Vec::new(&env);
        let now = env.ledger().timestamp();

        for i in 0..refund_ids.len() {
            let refund_id = refund_ids.get(i).unwrap();
            let maybe_refund: Option<Refund> =
                env.storage().persistent().get(&DataKey::Refund(refund_id));

            match maybe_refund {
                Some(mut refund) if refund.status == RefundStatus::Requested => {
                    refund.status = RefundStatus::Approved;
                    refund.approved_at = Some(now);
                    env.storage()
                        .persistent()
                        .set(&DataKey::Refund(refund_id), &refund);
                    env.storage().persistent().extend_ttl(
                        &DataKey::Refund(refund_id),
                        PERSISTENT_LIFETIME_THRESHOLD,
                        PERSISTENT_BUMP_AMOUNT,
                    );
                    Self::remove_from_pending_queue(&env, refund_id);
                    Self::decrement_fraud_score(&env, &refund.customer);
                    Self::update_stats_on_approve(&env, &refund.merchant);
                    events::emit_refund_approved(&env, refund_id, admin.clone(), now);
                    processed.push_back(refund_id);
                }
                _ => {
                    skipped.push_back(refund_id);
                }
            }
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        BatchRefundResult { processed, skipped }
    }

    /// Reject multiple pending refund requests in a single transaction.
    /// Skips IDs that are not in Requested status rather than reverting.
    /// Returns a BatchRefundResult with processed and skipped IDs.
    pub fn batch_reject_refunds(
        env: Env,
        admin: Address,
        refund_ids: Vec<u32>,
        reason_hash: BytesN<32>,
    ) -> BatchRefundResult {
        Self::require_not_paused(&env);
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic!("Only admin can batch reject refunds");
        }

        let max_batch: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MaxBatchSize)
            .unwrap_or(DEFAULT_MAX_BATCH_SIZE);

        if refund_ids.len() > max_batch {
            panic!("Batch size exceeds maximum allowed");
        }

        let mut processed = Vec::new(&env);
        let mut skipped = Vec::new(&env);
        let now = env.ledger().timestamp();

        for i in 0..refund_ids.len() {
            let refund_id = refund_ids.get(i).unwrap();
            let maybe_refund: Option<Refund> =
                env.storage().persistent().get(&DataKey::Refund(refund_id));

            match maybe_refund {
                Some(mut refund) if refund.status == RefundStatus::Requested => {
                    refund.status = RefundStatus::Rejected;
                    refund.rejected_at = Some(now);
                    env.storage()
                        .persistent()
                        .set(&DataKey::Refund(refund_id), &refund);
                    env.storage().persistent().extend_ttl(
                        &DataKey::Refund(refund_id),
                        PERSISTENT_LIFETIME_THRESHOLD,
                        PERSISTENT_BUMP_AMOUNT,
                    );
                    Self::remove_from_pending_queue(&env, refund_id);
                    Self::increment_fraud_score(
                        &env,
                        &refund.customer,
                        Symbol::new(&env, "admin_rejected"),
                    );
                    Self::update_stats_on_reject(&env, &refund.merchant);
                    events::emit_refund_rejected(
                        &env,
                        refund_id,
                        admin.clone(),
                        String::from_str(&env, "batch_reject"),
                        now,
                    );
                    processed.push_back(refund_id);
                }
                _ => {
                    skipped.push_back(refund_id);
                }
            }
        }

        // Store reason_hash for audit (emit as event)
        env.events().publish(
            (Symbol::new(&env, "BatchRejectReason"),),
            (admin.clone(), reason_hash),
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        BatchRefundResult { processed, skipped }
    }

    // ─── #274: Merchant Reserve Fund ─────────────────────────────────────────

    /// Admin sets the global reserve ratio in basis points (e.g. 200 = 2%).
    pub fn set_reserve_ratio_bps(env: Env, admin: Address, ratio_bps: u32) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        if ratio_bps > 10_000 {
            panic!("reserve_ratio_bps cannot exceed 10000");
        }
        env.storage()
            .instance()
            .set(&DataKey::ReserveRatioBps, &ratio_bps);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant deposits reserve funds held by the contract.
    pub fn deposit_reserve(env: Env, merchant: Address, token: Address, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        Self::require_token_allowed(&env, &token);
        let client = token::Client::new(&env, &token);
        client.transfer(&merchant, &env.current_contract_address(), &amount);
        let key = DataKey::MerchantReserve(merchant.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let new_balance = current + amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        // Clear flagged status if now compliant
        let flag_key = DataKey::MerchantFlagged(merchant.clone());
        if env.storage().persistent().has(&flag_key) {
            let required = Self::required_reserve_internal(&env, &merchant);
            if new_balance >= required {
                env.storage().persistent().remove(&flag_key);
            }
        }
        events::emit_reserve_deposited(&env, merchant, amount);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant withdraws excess reserve (must maintain minimum).
    pub fn withdraw_reserve(env: Env, merchant: Address, token: Address, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let key = DataKey::MerchantReserve(merchant.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        let required = Self::required_reserve_internal(&env, &merchant);
        if current - amount < required {
            panic!("WithdrawalWouldBreachMinimum: reserve would fall below required minimum");
        }
        let new_balance = current - amount;
        env.storage().persistent().set(&key, &new_balance);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        let client = token::Client::new(&env, &token);
        client.transfer(&env.current_contract_address(), &merchant, &amount);
        events::emit_reserve_withdrawn(&env, merchant, amount);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Check if a merchant is compliant with the reserve requirement.
    /// Flags non-compliant merchants; compliant merchants are unflagged.
    pub fn check_reserve_compliance(env: Env, admin: Address, merchant: Address) -> bool {
        Self::require_admin(&env, &admin);
        let current: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantReserve(merchant.clone()))
            .unwrap_or(0);
        let required = Self::required_reserve_internal(&env, &merchant);
        let compliant = current >= required;
        let flag_key = DataKey::MerchantFlagged(merchant.clone());
        if !compliant {
            env.storage().persistent().set(&flag_key, &true);
            env.storage().persistent().extend_ttl(
                &flag_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            events::emit_merchant_flagged_low_reserve(&env, merchant, current, required);
        } else if env.storage().persistent().has(&flag_key) {
            env.storage().persistent().remove(&flag_key);
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        compliant
    }

    /// Get the current reserve balance for a merchant.
    pub fn get_merchant_reserve(env: Env, merchant: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::MerchantReserve(merchant))
            .unwrap_or(0)
    }

    /// Record payment volume for a merchant (called when a payment is created).
    /// Adds `amount` to the merchant's tracked volume used for reserve compliance.
    pub fn record_payment_volume(env: Env, merchant: Address, amount: i128) {
        Self::require_not_paused(&env);
        if amount <= 0 {
            return;
        }
        let key = DataKey::MerchantVolume(merchant.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        env.storage().persistent().set(&key, &(current + amount));
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        // Block new payment if merchant is flagged
        if env
            .storage()
            .persistent()
            .has(&DataKey::MerchantFlagged(merchant))
        {
            panic!("ReserveBelowMinimum: merchant is non-compliant; deposit reserve before creating payments");
        }
    }

    fn required_reserve_internal(env: &Env, merchant: &Address) -> i128 {
        let ratio_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::ReserveRatioBps)
            .unwrap_or(0);
        if ratio_bps == 0 {
            return 0;
        }
        let volume: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantVolume(merchant.clone()))
            .unwrap_or(0);
        (volume * ratio_bps as i128) / 10_000
    }

    // ─── #276: Merchant Counter-Dispute Evidence Window ───────────────────────

    /// Admin sets the merchant response window in ledgers.
    pub fn set_merchant_response_window(env: Env, admin: Address, window_ledgers: u32) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        if window_ledgers == 0 {
            panic!("window_ledgers must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::MerchantResponseWindow, &window_ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Merchant submits evidence hashes before the deadline.
    /// `content_hash` is a SHA-256 hash of the actual evidence content anchored on-chain (#365).
    /// While the window is open, a second call from the same merchant overwrites the previous hash.
    /// After the window closes, any submission is rejected.
    pub fn submit_refund_evidence(
        env: Env,
        merchant: Address,
        refund_id: u32,
        evidence_hashes: Vec<BytesN<32>>,
        statement_hash: BytesN<32>,
        content_hash: BytesN<32>,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");
        if refund.merchant != merchant {
            panic!("Only the refund merchant can submit evidence");
        }
        if refund.status != RefundStatus::Requested && refund.status != RefundStatus::EvidenceSubmitted {
            panic!("Evidence can only be submitted for Requested or EvidenceSubmitted refunds");
        }
        // Window check: reject if deadline has passed
        if refund.merch_response_deadline > 0
            && env.ledger().sequence() > refund.merch_response_deadline
        {
            // Deadline passed — auto-advance status if not already advanced
            if refund.status != RefundStatus::EvidencePeriodExpired {
                refund.status = RefundStatus::EvidencePeriodExpired;
                env.storage()
                    .persistent()
                    .set(&DataKey::Refund(refund_id), &refund);
                env.storage().persistent().extend_ttl(
                    &DataKey::Refund(refund_id),
                    PERSISTENT_LIFETIME_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
                events::emit_evidence_period_expired(&env, refund_id, merchant);
            }
            panic!("EvidenceDeadlinePassed");
        }
        let num_hashes = evidence_hashes.len();
        let evidence = RefundEvidence {
            evidence_hashes,
            statement_hash: statement_hash.clone(),
            submitted_at_ledger: env.ledger().sequence(),
        };
        env.storage()
            .persistent()
            .set(&DataKey::RefundEvidence(refund_id), &evidence);
        env.storage().persistent().extend_ttl(
            &DataKey::RefundEvidence(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        // #365: Anchor content hash on-chain; overwrites any prior hash while window is open
        env.storage()
            .persistent()
            .set(&DataKey2::EvidenceHash(refund_id, merchant.clone()), &content_hash);
        env.storage().persistent().extend_ttl(
            &DataKey2::EvidenceHash(refund_id, merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        refund.evidence_submitted = true;
        refund.status = RefundStatus::EvidenceSubmitted;
        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        let now = env.ledger().timestamp();
        events::emit_evidence_anchored(&env, refund_id, merchant.clone(), content_hash, now);
        events::emit_merchant_evidence_submitted(
            &env,
            refund_id,
            merchant,
            num_hashes,
            statement_hash,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the evidence record for a refund.
    pub fn get_refund_evidence(env: Env, refund_id: u32) -> Option<RefundEvidence> {
        env.storage()
            .persistent()
            .get(&DataKey::RefundEvidence(refund_id))
    }

    // ─── #365: Evidence Hash Anchoring ───────────────────────────────────────

    /// Returns `true` if the stored content hash for `(refund_id, submitter)` matches `hash`.
    /// Returns `false` if no hash is stored or the stored value differs.
    /// This view never mutates storage.
    pub fn verify_evidence(env: Env, refund_id: u32, submitter: Address, hash: BytesN<32>) -> bool {
        let stored: Option<BytesN<32>> = env
            .storage()
            .persistent()
            .get(&DataKey2::EvidenceHash(refund_id, submitter));
        match stored {
            Some(h) => h == hash,
            None => false,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Feature: Refund Escalating Mediation Timeline
    // ─────────────────────────────────────────────────────────────────────────

    /// Admin configures the primary review deadline window in ledgers.
    pub fn set_primary_review_window(env: Env, admin: Address, ledgers: u32) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        if ledgers == 0 {
            panic!("ledgers must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey2::PrimaryReviewDeadlineLedgers, &ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin configures the senior review deadline window in ledgers.
    pub fn set_senior_review_window(env: Env, admin: Address, ledgers: u32) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        if ledgers == 0 {
            panic!("ledgers must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey2::SeniorReviewDeadlineLedgers, &ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin registers the senior arbiter address.
    pub fn set_senior_arbiter(env: Env, admin: Address, arbiter: Address) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey2::SeniorArbiter, &arbiter);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin enables or disables auto-approval when the senior arbiter misses their deadline.
    pub fn set_auto_approve_on_senior_miss(env: Env, admin: Address, flag: bool) {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey2::AutoApproveOnSeniorMiss, &flag);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured senior arbiter address.
    pub fn get_senior_arbiter(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey2::SeniorArbiter)
    }

    /// Escalate a refund to the senior arbiter after the primary review deadline has passed.
    /// Callable by any party. Panics if primary deadline has not yet passed.
    pub fn escalate_to_senior(env: Env, caller: Address, refund_id: u32) {
        Self::require_not_paused(&env);
        caller.require_auth();

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Requested
            && refund.status != RefundStatus::EvidenceSubmitted
            && refund.status != RefundStatus::EvidencePeriodExpired
        {
            panic!("EscalationNotAllowed: refund is not in an escalatable state");
        }

        if refund.primary_review_deadline_ledger == 0 {
            panic!("EscalationNotAllowed: no primary review deadline configured for this refund");
        }

        if env.ledger().sequence() < refund.primary_review_deadline_ledger {
            panic!("PrimaryDeadlineNotPassed: primary review deadline has not elapsed");
        }

        let senior_arbiter: Address = env
            .storage()
            .instance()
            .get(&DataKey2::SeniorArbiter)
            .expect("SeniorArbiterNotConfigured");

        let senior_window: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::SeniorReviewDeadlineLedgers)
            .unwrap_or(DEFAULT_SENIOR_REVIEW_DEADLINE_LEDGERS);

        refund.status = RefundStatus::EscalatedToSenior;
        refund.senior_review_deadline_ledger = env.ledger().sequence() + senior_window;

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_refund_escalated(&env, refund_id, caller, senior_arbiter);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Senior arbiter resolves an escalated refund dispute.
    /// If approved: processes refund (transfers tokens to customer).
    /// If not approved: returns escrowed tokens to customer and marks rejected.
    pub fn resolve_escalated_refund(
        env: Env,
        senior_arbiter: Address,
        refund_id: u32,
        approved: bool,
        resolution_hash: BytesN<32>,
    ) {
        Self::require_not_paused(&env);
        senior_arbiter.require_auth();

        let stored_arbiter: Address = env
            .storage()
            .instance()
            .get(&DataKey2::SeniorArbiter)
            .expect("SeniorArbiterNotConfigured");

        if senior_arbiter != stored_arbiter {
            panic!("UnauthorizedSeniorArbiter");
        }

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::EscalatedToSenior {
            panic!("RefundNotEscalated");
        }

        let now = env.ledger().timestamp();
        let client = token::Client::new(&env, &refund.token);

        if approved {
            let fee_bps: u32 = env
                .storage()
                .instance()
                .get(&DataKey::RefundFeeBps)
                .unwrap_or(0);
            let fee_amount = if fee_bps > 0 {
                (refund.amount as u128 * fee_bps as u128 / 10_000) as i128
            } else {
                0
            };
            let customer_amount = refund.amount - fee_amount;
            if customer_amount > 0 {
                client.transfer(
                    &env.current_contract_address(),
                    &refund.customer,
                    &customer_amount,
                );
            }
            if fee_amount > 0 {
                if let Some(fee_recipient) = env
                    .storage()
                    .instance()
                    .get::<DataKey, Address>(&DataKey::FeeRecipient)
                {
                    client.transfer(&env.current_contract_address(), &fee_recipient, &fee_amount);
                    events::emit_refund_fee_collected(&env, refund_id, fee_amount);
                }
            }
            let already_refunded: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::RefundedAmount(refund.payment_id))
                .unwrap_or(0);
            env.storage().persistent().set(
                &DataKey::RefundedAmount(refund.payment_id),
                &(already_refunded + refund.amount),
            );
            env.storage().persistent().extend_ttl(
                &DataKey::RefundedAmount(refund.payment_id),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            refund.status = RefundStatus::Processed;
            refund.approved_at = Some(now);
            refund.processed_at = Some(now);
            refund.fee_amount = if fee_amount > 0 {
                Some(fee_amount)
            } else {
                None
            };
            Self::update_stats_on_approve(&env, &refund.merchant);
            Self::update_stats_on_process(&env, &refund.merchant, refund.amount);
        } else {
            client.transfer(
                &env.current_contract_address(),
                &refund.customer,
                &refund.amount,
            );
            refund.status = RefundStatus::Rejected;
            refund.rejected_at = Some(now);
            Self::update_stats_on_reject(&env, &refund.merchant);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::remove_from_pending_queue(&env, refund_id);

        events::emit_escalated_refund_resolved(
            &env,
            refund_id,
            senior_arbiter,
            approved,
            resolution_hash,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Trigger auto-approval when the senior arbiter has missed their deadline.
    /// Callable by anyone after the senior deadline passes.
    /// Only executes if `auto_approve_on_senior_miss` is true.
    pub fn trigger_senior_auto_approve(env: Env, refund_id: u32) {
        Self::require_not_paused(&env);

        let auto_approve: bool = env
            .storage()
            .instance()
            .get(&DataKey2::AutoApproveOnSeniorMiss)
            .unwrap_or(false);

        if !auto_approve {
            panic!("AutoApproveOnSeniorMissDisabled");
        }

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::EscalatedToSenior {
            panic!("RefundNotEscalated");
        }

        if refund.senior_review_deadline_ledger == 0
            || env.ledger().sequence() < refund.senior_review_deadline_ledger
        {
            panic!("SeniorDeadlineNotPassed");
        }

        let now = env.ledger().timestamp();
        let fee_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RefundFeeBps)
            .unwrap_or(0);
        let fee_amount = if fee_bps > 0 {
            (refund.amount as u128 * fee_bps as u128 / 10_000) as i128
        } else {
            0
        };
        let customer_amount = refund.amount - fee_amount;
        let client = token::Client::new(&env, &refund.token);
        if customer_amount > 0 {
            client.transfer(
                &env.current_contract_address(),
                &refund.customer,
                &customer_amount,
            );
        }
        if fee_amount > 0 {
            if let Some(fee_recipient) = env
                .storage()
                .instance()
                .get::<DataKey, Address>(&DataKey::FeeRecipient)
            {
                client.transfer(&env.current_contract_address(), &fee_recipient, &fee_amount);
                events::emit_refund_fee_collected(&env, refund_id, fee_amount);
            }
        }
        let already_refunded: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::RefundedAmount(refund.payment_id))
            .unwrap_or(0);
        env.storage().persistent().set(
            &DataKey::RefundedAmount(refund.payment_id),
            &(already_refunded + refund.amount),
        );
        env.storage().persistent().extend_ttl(
            &DataKey::RefundedAmount(refund.payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        refund.status = RefundStatus::Processed;
        refund.processed_at = Some(now);
        refund.auto_approved_source = Some(String::from_str(&env, "senior_miss"));
        refund.fee_amount = if fee_amount > 0 {
            Some(fee_amount)
        } else {
            None
        };

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::remove_from_pending_queue(&env, refund_id);
        Self::update_stats_on_process(&env, &refund.merchant, refund.amount);

        events::emit_refund_auto_approved_senior_miss(&env, refund_id);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// #586: Admin view of refunds currently awaiting senior review.
    /// Only refunds with status `EscalatedToSenior` are returned (resolved
    /// escalations move to `Processed`/`Rejected` and drop out of this view).
    /// `limit` is capped at 50 per call.
    pub fn list_pending_senior_escalations(env: Env, offset: u32, limit: u32) -> Vec<u32> {
        let effective_limit = limit.min(50);
        let counter: u32 = env
            .storage()
            .instance()
            .get(&DataKey::RefundCounter)
            .unwrap_or(0);

        let mut result = Vec::new(&env);
        let mut matched: u32 = 0;
        for refund_id in 0..counter {
            let refund: Refund = match env.storage().persistent().get(&DataKey::Refund(refund_id)) {
                Some(r) => r,
                None => continue,
            };
            if refund.status != RefundStatus::EscalatedToSenior {
                continue;
            }
            if matched >= offset && result.len() < effective_limit {
                result.push_back(refund_id);
            }
            matched += 1;
        }
        result
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Feature: Refund Customer Abuse Score
    // ─────────────────────────────────────────────────────────────────────────

    /// Admin sets the abuse score block threshold.
    pub fn set_abuse_block_threshold(env: Env, admin: Address, threshold: u32) {
        Self::require_admin(&env, &admin);
        if threshold == 0 {
            panic!("threshold must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey2::AbuseBlockThreshold, &threshold);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin sets how long (in ledgers) a customer stays blocked after hitting the threshold.
    pub fn set_block_duration_ledgers(env: Env, admin: Address, ledgers: u64) {
        Self::require_admin(&env, &admin);
        if ledgers == 0 {
            panic!("ledgers must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey2::BlockDurationLedgers, &ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin sets the rapid-submission detection window in ledgers.
    pub fn set_rapid_submission_window(env: Env, admin: Address, ledgers: u32) {
        Self::require_admin(&env, &admin);
        if ledgers == 0 {
            panic!("ledgers must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey2::RapidSubmissionWindowLedgers, &ledgers);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin flags a specific refund denial as abusive, applying the elevated +20 increment.
    pub fn flag_refund_abuse(env: Env, admin: Address, refund_id: u32) {
        Self::require_admin(&env, &admin);

        let refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Rejected {
            panic!("OnlyRejectedRefundsCanBeFlagged");
        }

        Self::increment_abuse_score(&env, &refund.customer, 10, true); // +10 more on top of existing +10

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin clears a customer's abuse record.
    pub fn reset_customer_abuse_score(env: Env, admin: Address, customer: Address) {
        Self::require_admin(&env, &admin);

        let zero_record = RefundAbuseRecord {
            total_requests: 0,
            denied_count: 0,
            auto_denied_count: 0,
            rapid_submission_count: 0,
            abuse_score: 0,
            blocked_until_ledger: 0,
            last_increment_ledger: 0,
            last_submission_ledger: 0,
        };
        env.storage().persistent().set(
            &DataKey2::CustomerAbuseRecord(customer.clone()),
            &zero_record,
        );
        env.storage().persistent().extend_ttl(
            &DataKey2::CustomerAbuseRecord(customer.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Public read: returns the customer's abuse record with lazy decay applied.
    pub fn get_customer_abuse_score(env: Env, customer: Address) -> RefundAbuseRecord {
        Self::load_abuse_record_with_decay(&env, &customer)
    }

    // ─── #355: Configurable Abuse Score Decay ────────────────────────────────

    /// Admin configures the exponential decay parameters for the abuse score system.
    ///
    /// - `period_ledgers`: number of ledgers between decay applications (e.g. 10_000).
    ///   After each period the score is multiplied by `factor_bps / 10_000`.
    /// - `factor_bps`: decay factor in basis points (e.g. 5000 = halve every period).
    ///   Must be < 10_000 (a factor ≥ 10_000 would mean no decay or score growth).
    ///   Use 0 to disable decay entirely.
    ///
    /// Updating these parameters does NOT reset existing scores; the new values are
    /// used the next time a score is mutated or read.
    pub fn set_abuse_score_decay_params(
        env: Env,
        admin: Address,
        period_ledgers: u64,
        factor_bps: u32,
    ) {
        Self::require_admin(&env, &admin);

        if factor_bps > 10_000 {
            panic!("factor_bps must be <= 10_000 (10_000 = no decay)");
        }
        if period_ledgers == 0 && factor_bps > 0 {
            panic!("period_ledgers must be positive when factor_bps > 0");
        }

        env.storage()
            .instance()
            .set(&DataKey2::AbuseScoreDecayPeriodLedgers, &period_ledgers);
        env.storage()
            .instance()
            .set(&DataKey2::AbuseScoreDecayFactorBps, &factor_bps);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Get the configured abuse score decay period in ledgers (default: 10_000).
    pub fn get_abuse_decay_period_ledgers(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey2::AbuseScoreDecayPeriodLedgers)
            .unwrap_or(10_000u64)
    }

    /// Get the configured abuse score decay factor in basis points (default: 5_000 = halve).
    pub fn get_abuse_score_decay_factor_bps(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey2::AbuseScoreDecayFactorBps)
            .unwrap_or(5_000u32)
    }

    // --- Internal abuse-score helpers ---

    fn load_abuse_record_with_decay(env: &Env, customer: &Address) -> RefundAbuseRecord {
        let mut record: RefundAbuseRecord = env
            .storage()
            .persistent()
            .get(&DataKey2::CustomerAbuseRecord(customer.clone()))
            .unwrap_or(RefundAbuseRecord {
                total_requests: 0,
                denied_count: 0,
                auto_denied_count: 0,
                rapid_submission_count: 0,
                abuse_score: 0,
                blocked_until_ledger: 0,
                last_increment_ledger: 0,
                last_submission_ledger: 0,
            });

        // #355: Configurable exponential decay.
        // effective_score = stored_score * (factor_bps / 10_000) ^ periods_elapsed
        // Computed on read; the decayed value is NOT written back (no storage mutation on read).
        if record.abuse_score > 0 {
            let current_ledger = env.ledger().sequence() as u64;
            if current_ledger > record.last_increment_ledger {
                let decay_period: u64 = env
                    .storage()
                    .instance()
                    .get(&DataKey2::AbuseScoreDecayPeriodLedgers)
                    .unwrap_or(10_000u64);
                let decay_factor_bps: u32 = env
                    .storage()
                    .instance()
                    .get(&DataKey2::AbuseScoreDecayFactorBps)
                    .unwrap_or(5_000u32); // default: halve every period

                if decay_period > 0 {
                    let elapsed = current_ledger - record.last_increment_ledger;
                    let periods = (elapsed / decay_period) as u32;
                    if periods > 0 {
                        // Apply (factor_bps / 10_000) ^ periods multiplicatively
                        let mut score = record.abuse_score as u128;
                        for _ in 0..periods {
                            score = score.saturating_mul(decay_factor_bps as u128) / 10_000;
                            if score == 0 {
                                break;
                            }
                        }
                        record.abuse_score = score as u32;
                    }
                }
            }
        }

        record
    }

    /// Write back the decay-adjusted record and emit `AbuseScoreDecayed` when the
    /// score was reduced by decay. This is called from mutating paths so that the
    /// event is only emitted on state changes, not on read-only queries.
    fn persist_abuse_record_with_decay_event(
        env: &Env,
        customer: &Address,
        old_record: &RefundAbuseRecord,
        new_record: &RefundAbuseRecord,
    ) {
        if old_record.abuse_score > new_record.abuse_score {
            let decay_period: u64 = env
                .storage()
                .instance()
                .get(&DataKey2::AbuseScoreDecayPeriodLedgers)
                .unwrap_or(10_000u64);
            let elapsed = if new_record.last_increment_ledger > old_record.last_increment_ledger {
                0u64
            } else {
                let current = env.ledger().sequence() as u64;
                current.saturating_sub(old_record.last_increment_ledger)
            };
            let periods = if decay_period > 0 { (elapsed / decay_period) as u32 } else { 0 };
            events::emit_abuse_score_decayed(
                env,
                customer.clone(),
                old_record.abuse_score,
                new_record.abuse_score,
                periods,
            );
        }
        env.storage()
            .persistent()
            .set(&DataKey2::CustomerAbuseRecord(customer.clone()), new_record);
        env.storage().persistent().extend_ttl(
            &DataKey2::CustomerAbuseRecord(customer.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    fn check_abuse_block(env: &Env, customer: &Address) {
        let record = Self::load_abuse_record_with_decay(env, customer);
        let current_ledger = env.ledger().sequence() as u64;

        // Check if currently in a block window
        if record.blocked_until_ledger > 0 && current_ledger <= record.blocked_until_ledger {
            events::emit_customer_blocked_for_abuse(
                env,
                customer.clone(),
                record.blocked_until_ledger,
            );
            panic!("CustomerBlockedForAbuse");
        }

        // Check if score now exceeds threshold (even after decay)
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::AbuseBlockThreshold)
            .unwrap_or(u32::MAX);

        if threshold != u32::MAX && record.abuse_score >= threshold {
            let block_duration: u64 = env
                .storage()
                .instance()
                .get(&DataKey2::BlockDurationLedgers)
                .unwrap_or(DEFAULT_BLOCK_DURATION_LEDGERS);
            let blocked_until = current_ledger + block_duration;

            let mut updated = record.clone();
            updated.blocked_until_ledger = blocked_until;
            env.storage()
                .persistent()
                .set(&DataKey2::CustomerAbuseRecord(customer.clone()), &updated);
            env.storage().persistent().extend_ttl(
                &DataKey2::CustomerAbuseRecord(customer.clone()),
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );

            events::emit_customer_blocked_for_abuse(env, customer.clone(), blocked_until);
            panic!("CustomerBlockedForAbuse");
        }
    }

    fn update_abuse_on_request(env: &Env, customer: &Address, current_seq: u32, rapid_window: u32) {
        let old_record = Self::load_abuse_record_with_decay(env, customer);
        let mut record = old_record.clone();
        record.total_requests += 1;

        // Rapid submission detection (total_requests > 1 means at least one prior submission exists)
        if record.total_requests > 1
            && current_seq.saturating_sub(record.last_submission_ledger) < rapid_window
        {
            record.rapid_submission_count += 1;
            record.abuse_score = record.abuse_score.saturating_add(5);
            record.last_increment_ledger = env.ledger().sequence() as u64;

            events::emit_customer_abuse_score_updated(env, customer.clone(), record.abuse_score, 5);
        }

        record.last_submission_ledger = current_seq;

        // Emit decay event if score was reduced by decay, then persist
        Self::persist_abuse_record_with_decay_event(env, customer, &old_record, &record);
    }

    fn increment_abuse_score(env: &Env, customer: &Address, delta: u32, is_elevated: bool) {
        // Load with in-memory decay applied (does NOT write to storage)
        let old_record = Self::load_abuse_record_with_decay(env, customer);
        let mut record = old_record.clone();

        let effective_delta = if is_elevated { delta + 10 } else { delta };
        record.denied_count += 1;
        if is_elevated {
            record.auto_denied_count += 1;
        }
        record.abuse_score = record.abuse_score.saturating_add(effective_delta);
        record.last_increment_ledger = env.ledger().sequence() as u64;

        events::emit_customer_abuse_score_updated(
            env,
            customer.clone(),
            record.abuse_score,
            effective_delta,
        );

        // Check threshold and apply block
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::AbuseBlockThreshold)
            .unwrap_or(u32::MAX);

        if threshold != u32::MAX && record.abuse_score >= threshold {
            let block_duration: u64 = env
                .storage()
                .instance()
                .get(&DataKey2::BlockDurationLedgers)
                .unwrap_or(DEFAULT_BLOCK_DURATION_LEDGERS);
            let current_ledger = env.ledger().sequence() as u64;
            let blocked_until = current_ledger + block_duration;
            record.blocked_until_ledger = blocked_until;
            events::emit_customer_blocked_for_abuse(env, customer.clone(), blocked_until);
        }

        // Persist; decay event emitted if score dropped due to exponential decay on load
        Self::persist_abuse_record_with_decay_event(env, customer, &old_record, &record);
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Feature: Cross-Contract Refund Registration
    // ─────────────────────────────────────────────────────────────────────────

    /// Admin adds a contract to the cross-contract refund whitelist.
    pub fn add_refund_origin_contract(env: Env, admin: Address, contract: Address) {
        Self::require_admin(&env, &admin);

        let mut whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey2::CrossContractWhitelist)
            .unwrap_or(Vec::new(&env));

        for addr in whitelist.iter() {
            if addr == contract {
                panic!("ContractAlreadyWhitelisted");
            }
        }

        whitelist.push_back(contract.clone());
        env.storage()
            .persistent()
            .set(&DataKey2::CrossContractWhitelist, &whitelist);
        env.storage().persistent().extend_ttl(
            &DataKey2::CrossContractWhitelist,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin removes a contract from the cross-contract refund whitelist.
    pub fn remove_refund_origin_contract(env: Env, admin: Address, contract: Address) {
        Self::require_admin(&env, &admin);

        let whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey2::CrossContractWhitelist)
            .unwrap_or(Vec::new(&env));

        let mut new_list = Vec::new(&env);
        let mut found = false;
        for addr in whitelist.iter() {
            if addr == contract {
                found = true;
            } else {
                new_list.push_back(addr);
            }
        }

        if !found {
            panic!("ContractNotWhitelisted");
        }

        env.storage()
            .persistent()
            .set(&DataKey2::CrossContractWhitelist, &new_list);
        env.storage().persistent().extend_ttl(
            &DataKey2::CrossContractWhitelist,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Privileged: called by a whitelisted origin contract to register a cross-contract refund.
    /// No token transfer occurs — the origin contract handles fund movement.
    /// Returns the new refund ID.
    pub fn register_cross_contract_refund(
        env: Env,
        origin_contract: Address,
        origin_id: u32,
        customer: Address,
        merchant: Address,
        token: Address,
        amount: i128,
        reason_code: u32,
    ) -> u32 {
        Self::require_not_paused(&env);
        origin_contract.require_auth();

        if amount <= 0 {
            panic!("amount must be positive");
        }

        // Verify origin_contract is whitelisted
        let whitelist: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey2::CrossContractWhitelist)
            .unwrap_or(Vec::new(&env));
        let mut allowed = false;
        for addr in whitelist.iter() {
            if addr == origin_contract {
                allowed = true;
                break;
            }
        }
        if !allowed {
            panic!("UnauthorisedOriginContract");
        }

        let refund_id = Self::next_refund_id(&env);
        let now = env.ledger().timestamp();

        let refund = Refund {
            id: refund_id,
            payment_id: 0,
            customer: customer.clone(),
            merchant: merchant.clone(),
            amount,
            token: token.clone(),
            status: RefundStatus::CrossContractRefunded,
            reason: String::from_str(&env, "cross_contract"),
            reason_code,
            requested_at: now,
            approved_at: Some(now),
            processed_at: Some(now),
            rejected_at: None,
            auto_approved_source: Some(String::from_str(&env, "cross_contract")),
            escrow_id: Some(origin_id),
            fee_amount: None,
            priority: Priority::Medium,
            merch_response_deadline: 0,
            evidence_submitted: false,
            primary_review_deadline_ledger: 0,
            senior_review_deadline_ledger: 0,
            origin_contract: Some(origin_contract.clone()),
            auto_approval_deadline_ledger: 0,
            extension_requested: false,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::append_index(&env, &DataKey::CustomerRefunds(customer.clone()), refund_id);
        Self::append_index(&env, &DataKey::MerchantRefunds(merchant.clone()), refund_id);

        // Add to cross-contract queue for admin visibility
        let mut cc_queue: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey2::CrossContractRefundQueue)
            .unwrap_or(Vec::new(&env));
        cc_queue.push_back(refund_id);
        env.storage()
            .persistent()
            .set(&DataKey2::CrossContractRefundQueue, &cc_queue);
        env.storage().persistent().extend_ttl(
            &DataKey2::CrossContractRefundQueue,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        // Count in per-merchant metrics
        Self::update_stats_on_request(&env, &merchant, amount);
        Self::update_stats_on_process(&env, &merchant, amount);

        events::emit_cross_contract_refund_registered(
            &env,
            refund_id,
            origin_contract,
            origin_id,
            customer,
            amount,
        );

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        refund_id
    }

    /// Get the list of whitelisted origin contracts.
    pub fn get_cross_contract_whitelist(env: Env) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&DataKey2::CrossContractWhitelist)
            .unwrap_or(Vec::new(&env))
    }

    /// Get paginated cross-contract refund records for the admin queue.
    /// Returns (items, total, has_more).
    pub fn get_cross_contract_refund_queue(
        env: Env,
        page: u32,
        page_size: u32,
    ) -> (Vec<Refund>, u32, bool) {
        let effective_size = page_size.min(50);
        let queue: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey2::CrossContractRefundQueue)
            .unwrap_or(Vec::new(&env));
        let total = queue.len();
        let start = (page * effective_size).min(total);
        let end = (start + effective_size).min(total);
        let mut result: Vec<Refund> = Vec::new(&env);
        for i in start..end {
            let rid = queue.get(i).unwrap();
            if let Some(r) = env
                .storage()
                .persistent()
                .get::<DataKey, Refund>(&DataKey::Refund(rid))
            {
                result.push_back(r);
            }
        }
        (result, total, end < total)
    }

    // --- #334: Merchant Reserve Minimum Balance Requirement ---

    /// Merchant deposits into reserve balance (cross-contract callable).
    pub fn deposit_merchant_reserve(env: Env, merchant: Address, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey2::ReserveToken)
            .unwrap_or_else(|| panic!("Reserve token not configured"));

        let client = token::Client::new(&env, &token);
        client.transfer_from(
            &env.current_contract_address(),
            &merchant,
            &env.current_contract_address(),
            &amount,
        );

        let mut balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantReserveBalance(merchant.clone()))
            .unwrap_or(0);
        balance += amount;

        env.storage()
            .persistent()
            .set(&DataKey2::MerchantReserveBalance(merchant.clone()), &balance);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantReserveBalance(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_merchant_reserve_deposited(&env, merchant, amount, balance);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Check merchant reserve meets minimum requirement (cross-contract callable).
    pub fn check_merchant_reserve(env: Env, merchant: Address) -> bool {
        let min_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::MinReserveBpsOfMonthlyVolume)
            .unwrap_or(500); // 5% default

        // Check if waived
        let waiver_expiry: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::ReserveWaiverExpiryLedger(merchant.clone()))
            .unwrap_or(0);
        if waiver_expiry > env.ledger().sequence() {
            return true;
        }

        let reserve: i128 = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantReserveBalance(merchant.clone()))
            .unwrap_or(0);

        let volume: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantVolume(merchant.clone()))
            .unwrap_or(0);

        let minimum_required = (volume as u128 * min_bps as u128 / 10_000) as i128;
        let met = reserve >= minimum_required;
        let alert_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::ReserveAlertThresholdBps)
            .unwrap_or(10_000);
        let alert_at = minimum_required * alert_bps as i128 / 10_000;

        // Emit alert if low
        if reserve < alert_at {
            events::emit_merchant_reserve_low(&env, merchant, reserve, minimum_required);
        }

        met
    }

    /// Withdraw from merchant reserve (blocked if below minimum).
    pub fn withdraw_merchant_reserve(env: Env, merchant: Address, amount: i128) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount <= 0 {
            panic!("Amount must be positive");
        }

        let mut balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey2::MerchantReserveBalance(merchant.clone()))
            .unwrap_or(0);

        if balance < amount {
            panic!("Insufficient reserve balance");
        }

        // Check that withdrawal doesn't breach minimum
        let min_bps: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::MinReserveBpsOfMonthlyVolume)
            .unwrap_or(500);

        let volume: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantVolume(merchant.clone()))
            .unwrap_or(0);

        let minimum_required = (volume as u128 * min_bps as u128 / 10_000) as i128;

        if balance - amount < minimum_required {
            panic!("Withdrawal would breach minimum reserve requirement");
        }

        balance -= amount;
        env.storage()
            .persistent()
            .set(&DataKey2::MerchantReserveBalance(merchant.clone()), &balance);

        let token: Address = env
            .storage()
            .instance()
            .get(&DataKey2::ReserveToken)
            .unwrap_or_else(|| panic!("Reserve token not configured"));

        let client = token::Client::new(&env, &token);
        client.transfer(&env.current_contract_address(), &merchant, &amount);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Admin waives reserve requirement for a merchant.
    pub fn waive_reserve_requirement(
        env: Env,
        admin: Address,
        merchant: Address,
        waiver_expiry_ledger: u32,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        Self::require_admin(&env, &admin);

        env.storage().instance().set(
            &DataKey2::ReserveWaiverExpiryLedger(merchant.clone()),
            &waiver_expiry_ledger,
        );
        events::emit_reserve_requirement_waived(&env, merchant, waiver_expiry_ledger);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn set_reserve_config(
        env: Env,
        admin: Address,
        token: Address,
        min_bps: u32,
        alert_bps: u32,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&DataKey2::ReserveToken, &token);
        env.storage()
            .instance()
            .set(&DataKey2::MinReserveBpsOfMonthlyVolume, &min_bps);
        env.storage()
            .instance()
            .set(&DataKey2::ReserveAlertThresholdBps, &alert_bps);
    }

    /// #585: Admin-triggered one-time migration of a merchant's legacy (#274)
    /// reserve balance into the canonical (#334) reserve balance. Moves the
    /// full legacy balance, zeroes the legacy entry, and returns the amount
    /// migrated (0 if the merchant had no legacy balance). Total merchant
    /// reserve balance is preserved exactly — no funds are created or lost,
    /// only relocated between the two storage keys the two subsystems track.
    pub fn migrate_merchant_reserve(env: Env, admin: Address, merchant: Address) -> i128 {
        Self::require_not_paused(&env);
        Self::require_admin(&env, &admin);

        let legacy_key = DataKey::MerchantReserve(merchant.clone());
        let legacy_balance: i128 = env.storage().persistent().get(&legacy_key).unwrap_or(0);
        if legacy_balance == 0 {
            return 0;
        }

        env.storage().persistent().set(&legacy_key, &0i128);

        let canonical_key = DataKey2::MerchantReserveBalance(merchant.clone());
        let canonical_balance: i128 = env.storage().persistent().get(&canonical_key).unwrap_or(0);
        let new_canonical_balance = canonical_balance + legacy_balance;
        env.storage()
            .persistent()
            .set(&canonical_key, &new_canonical_balance);
        env.storage().persistent().extend_ttl(
            &canonical_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_merchant_reserve_migrated(
            &env,
            merchant,
            legacy_balance,
            new_canonical_balance,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        legacy_balance
    }

    pub fn set_merchant_response_deadline(
        env: Env,
        admin: Address,
        deadline_ledgers: u32,
        extension_ledgers: u32,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage()
            .instance()
            .set(&DataKey2::MerchantResponseDeadlineLedgers, &deadline_ledgers);
        env.storage()
            .instance()
            .set(&DataKey2::RefundExtensionLedgers, &extension_ledgers);
    }

    pub fn set_merchant_auto_approve_exempt(
        env: Env,
        admin: Address,
        merchant: Address,
        exempt: bool,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage()
            .persistent()
            .set(&DataKey2::MerchantAutoApproveExempt(merchant), &exempt);
    }

    // --- #335: Refund Auto-Approval on Merchant Non-Response ---

    /// Trigger auto-approval after merchant deadline has passed.
    pub fn trigger_auto_approval(env: Env, refund_id: u32) {
        Self::require_not_paused(&env);

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.status != RefundStatus::Requested {
            panic!("RefundNotInRequestedState");
        }
        if env
            .storage()
            .persistent()
            .get::<DataKey2, bool>(&DataKey2::MerchantAutoApproveExempt(refund.merchant.clone()))
            .unwrap_or(false)
        {
            panic!("MerchantAutoApprovalExempt");
        }

        let now_ledger = env.ledger().sequence();
        if now_ledger < refund.auto_approval_deadline_ledger {
            panic!("MerchantDeadlineNotPassed");
        }

        refund.status = RefundStatus::Approved;
        refund.approved_at = Some(env.ledger().timestamp());
        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        Self::process_refund_internal(&env, refund_id);
        events::emit_refund_auto_approved(&env, refund_id, refund.customer, refund.amount);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Customer requests extension on merchant response deadline (one-time only).
    pub fn request_review_extension(
        env: Env,
        merchant: Address,
        refund_id: u32,
        reason_hash: BytesN<32>,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let mut refund: Refund = env
            .storage()
            .persistent()
            .get(&DataKey::Refund(refund_id))
            .expect("Refund not found");

        if refund.merchant != merchant {
            panic!("OnlyMerchantCanRequestExtension");
        }

        if refund.extension_requested {
            panic!("ExtensionAlreadyUsed");
        }

        let extension_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey2::RefundExtensionLedgers)
            .unwrap_or(120_960); // ~7 days

        refund.auto_approval_deadline_ledger += extension_ledgers;
        refund.extension_requested = true;

        env.storage()
            .persistent()
            .set(&DataKey::Refund(refund_id), &refund);
        env.storage().persistent().extend_ttl(
            &DataKey::Refund(refund_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_review_extension_granted(
            &env,
            refund_id,
            refund.auto_approval_deadline_ledger,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // --- #320: Merchant On-Chain Refund Policy Registry ---

    /// Merchant publishes their refund policy.
    pub fn publish_refund_policy(
        env: Env,
        merchant: Address,
        eligible_window_ledgers: u32,
        max_refund_bps: u32,
        excluded_tags: Vec<Symbol>,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if max_refund_bps > 10_000 {
            panic!("MaxRefundBpsCannotExceed100Percent");
        }

        let was_update = env
            .storage()
            .persistent()
            .has(&DataKey2::MerchantRefundPolicy(merchant.clone()));
        let policy = RefundPolicy {
            eligible_window_ledgers,
            max_refund_bps,
            excluded_tags,
        };

        env.storage()
            .persistent()
            .set(&DataKey2::MerchantRefundPolicy(merchant.clone()), &policy);
        env.storage().persistent().extend_ttl(
            &DataKey2::MerchantRefundPolicy(merchant.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        if was_update {
            events::emit_refund_policy_updated(&env, merchant);
        } else {
            events::emit_refund_policy_published(
                &env,
                merchant,
                eligible_window_ledgers,
                max_refund_bps,
            );
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn set_global_refund_policy(
        env: Env,
        admin: Address,
        eligible_window_ledgers: u32,
        max_refund_bps: u32,
        excluded_tags: Vec<Symbol>,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if max_refund_bps > 10_000 {
            panic!("MaxRefundBpsCannotExceed100Percent");
        }
        env.storage().instance().set(
            &DataKey2::GlobalRefundPolicy,
            &RefundPolicy {
                eligible_window_ledgers,
                max_refund_bps,
                excluded_tags,
            },
        );
    }

    /// Validate refund against merchant's published policy.
    pub fn validate_against_policy(
        env: Env,
        merchant: Address,
        payment_created_at: u64,
        refund_amount: i128,
        payment_amount: i128,
        payment_tags: Vec<Symbol>,
    ) -> bool {
        if let Some(policy) = env
            .storage()
            .persistent()
            .get::<DataKey2, RefundPolicy>(&DataKey2::MerchantRefundPolicy(merchant.clone()))
        {
            // Check window
            let elapsed_seconds = env.ledger().timestamp().saturating_sub(payment_created_at);
            let elapsed_ledgers = (elapsed_seconds / 5) as u32; // Approximate
            if elapsed_ledgers > policy.eligible_window_ledgers {
                return false;
            }

            // Check amount
            let max_refund =
                (payment_amount as u128 * policy.max_refund_bps as u128 / 10_000) as i128;
            if refund_amount > max_refund {
                return false;
            }

            // Check tags
            for tag in &payment_tags {
                if Self::is_tag_excluded(&policy.excluded_tags, tag) {
                    return false;
                }
            }

            return true;
        }

        // No policy = no restrictions (or apply global default)
        true
    }

    fn refund_policy_for(env: &Env, merchant: &Address) -> RefundPolicy {
        env.storage()
            .persistent()
            .get(&DataKey2::MerchantRefundPolicy(merchant.clone()))
            .or_else(|| env.storage().instance().get(&DataKey2::GlobalRefundPolicy))
            .unwrap_or(RefundPolicy {
                eligible_window_ledgers: u32::MAX,
                max_refund_bps: 10_000,
                excluded_tags: Vec::new(env),
            })
    }

    /// Returns the full currently active `RefundPolicy` for a merchant (#583):
    /// the merchant's own published policy if set, else the admin global default,
    /// else the hardcoded fallback — the same resolution used to enforce refunds.
    pub fn export_refund_policy(env: Env, merchant: Address) -> RefundPolicy {
        Self::refund_policy_for(&env, &merchant)
    }

    fn enforce_refund_policy(
        env: &Env,
        policy: &RefundPolicy,
        payment_created_at: u64,
        refund_amount: i128,
        payment_amount: i128,
        payment_tags: &Option<Vec<Symbol>>,
    ) {
        let elapsed_ledgers =
            (env.ledger().timestamp().saturating_sub(payment_created_at) / 5) as u32;
        if elapsed_ledgers > policy.eligible_window_ledgers {
            panic!("RefundOutsideWindow");
        }
        if refund_amount > (payment_amount as u128 * policy.max_refund_bps as u128 / 10_000) as i128
        {
            panic!("RefundAmountExceedsPolicy");
        }
        if let Some(tags) = payment_tags {
            for tag in tags {
                if Self::is_tag_excluded(&policy.excluded_tags, tag) {
                    panic!("PaymentTagExcluded");
                }
            }
        }
    }

    fn is_tag_excluded(excluded: &Vec<Symbol>, tag: Symbol) -> bool {
        for t in excluded {
            if t == tag {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_token_whitelist;

#[cfg(test)]
mod test_counter_offer;

#[cfg(test)]
mod test_reserve_fund;

#[cfg(test)]
mod test_evidence_window;

#[cfg(test)]
mod test_escalation;

#[cfg(test)]
mod test_abuse_score;

#[cfg(test)]
mod test_cross_contract_refund;

#[cfg(test)]
mod test_deadline_boundaries;
