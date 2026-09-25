#![allow(dead_code)]
use soroban_sdk::{
    contracterror, contracttype, Address,
    BytesN, Env, Map, String, Vec,
};

/// Pre-approved spending allowance with on-chain consent record
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpendingAllowance {
    /// Allowance ID
    pub allowance_id: u32,
    /// Customer address
    pub customer: Address,
    /// Merchant address
    pub merchant: Address,
    /// Token address
    pub token: Address,
    /// Total allowance amount
    pub total_amount: i128,
    /// Amount already spent
    pub amount_spent: i128,
    /// Allowance creation timestamp
    pub created_at: u64,
    /// Allowance expiration timestamp
    pub expires_at: u64,
    /// Allowance status
    pub status: AllowanceStatus,
    /// Consent record hash
    pub consent_hash: BytesN<32>,
    /// Consent timestamp
    pub consent_timestamp: u64,
    /// Consent metadata (e.g., IP, device, location)
    pub consent_metadata: Map<String, String>,
    /// Spending limit per transaction
    pub per_transaction_limit: i128,
    /// Spending limit per day
    pub daily_limit: i128,
    /// Current day's spending
    pub daily_spent: i128,
    /// Last reset timestamp for daily limit
    pub daily_reset_timestamp: u64,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllowanceStatus {
    Active = 0,
    Paused = 1,
    Revoked = 2,
    Expired = 3,
    Exhausted = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsentRecord {
    /// Consent record ID
    pub consent_id: u32,
    /// Customer address
    pub customer: Address,
    /// Merchant address
    pub merchant: Address,
    /// Consent type
    pub consent_type: ConsentType,
    /// Consent hash (hash of terms/conditions)
    pub consent_hash: BytesN<32>,
    /// Consent timestamp
    pub timestamp: u64,
    /// Consent expiration
    pub expires_at: u64,
    /// IP address (hashed)
    pub ip_hash: BytesN<32>,
    /// Device fingerprint (hashed)
    pub device_hash: BytesN<32>,
    /// Geographic location (hashed)
    pub location_hash: BytesN<32>,
    /// Consent status
    pub status: ConsentStatus,
    /// Additional metadata
    pub metadata: Map<String, String>,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsentType {
    SpendingAllowance = 0,
    RecurringPayment = 1,
    DataSharing = 2,
    Marketing = 3,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsentStatus {
    Active = 0,
    Revoked = 1,
    Expired = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceTransaction {
    /// Transaction ID
    pub tx_id: u32,
    /// Allowance ID
    pub allowance_id: u32,
    /// Amount spent
    pub amount: i128,
    /// Transaction timestamp
    pub timestamp: u64,
    /// Transaction status
    pub status: TransactionStatus,
    /// Reference/description
    pub reference: String,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransactionStatus {
    Pending = 0,
    Approved = 1,
    Declined = 2,
    Completed = 3,
    Reversed = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceAuditLog {
    /// Log entry ID
    pub log_id: u32,
    /// Allowance ID
    pub allowance_id: u32,
    /// Action type
    pub action: AuditAction,
    /// Actor address
    pub actor: Address,
    /// Timestamp
    pub timestamp: u64,
    /// Details
    pub details: String,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditAction {
    Created = 0,
    Modified = 1,
    Paused = 2,
    Resumed = 3,
    Revoked = 4,
    TransactionApproved = 5,
    TransactionDeclined = 6,
    ConsentRecorded = 7,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpendingAllowanceError {
    AllowanceNotFound = 1,
    AllowanceExpired = 2,
    AllowanceExhausted = 3,
    AllowanceRevoked = 4,
    TransactionExceedsLimit = 5,
    DailyLimitExceeded = 6,
    PerTransactionLimitExceeded = 7,
    UnauthorizedAccess = 8,
    ConsentNotFound = 9,
    ConsentExpired = 10,
    ConsentRevoked = 11,
    InvalidConsentHash = 12,
    AllowancePaused = 13,
    InvalidAllowanceAmount = 14,
}

pub trait PreApprovedSpendingInterface {
    /// Create a new spending allowance with consent
    fn create_allowance(
        env: Env,
        customer: Address,
        merchant: Address,
        token: Address,
        total_amount: i128,
        per_transaction_limit: i128,
        daily_limit: i128,
        expires_at: u64,
        consent_hash: BytesN<32>,
        consent_metadata: Map<String, String>,
    ) -> u32;

    /// Record consent for an allowance
    fn record_consent(
        env: Env,
        customer: Address,
        merchant: Address,
        consent_type: ConsentType,
        consent_hash: BytesN<32>,
        ip_hash: BytesN<32>,
        device_hash: BytesN<32>,
        location_hash: BytesN<32>,
        expires_at: u64,
        metadata: Map<String, String>,
    ) -> u32;

    /// Spend from an allowance
    fn spend_from_allowance(
        env: Env,
        allowance_id: u32,
        amount: i128,
        reference: String,
    ) -> AllowanceTransaction;

    /// Get allowance details
    fn get_allowance(env: Env, allowance_id: u32) -> Option<SpendingAllowance>;

    /// Get consent record
    fn get_consent(env: Env, consent_id: u32) -> Option<ConsentRecord>;

    /// Pause an allowance
    fn pause_allowance(env: Env, allowance_id: u32);

    /// Resume a paused allowance
    fn resume_allowance(env: Env, allowance_id: u32);

    /// Revoke an allowance
    fn revoke_allowance(env: Env, allowance_id: u32);

    /// Revoke consent
    fn revoke_consent(env: Env, consent_id: u32);

    /// Get remaining balance
    fn get_remaining_balance(env: Env, allowance_id: u32) -> i128;

    /// Get daily remaining balance
    fn get_daily_remaining(env: Env, allowance_id: u32) -> i128;

    /// Get allowance transaction history
    fn get_allowance_transactions(env: Env, allowance_id: u32) -> Vec<AllowanceTransaction>;

    /// Get audit log for an allowance
    fn get_audit_log(env: Env, allowance_id: u32) -> Vec<AllowanceAuditLog>;

    /// Get a page of a customer's allowances (at most `limit` entries starting at `offset`)
    fn get_customer_allowances(
        env: Env,
        customer: Address,
        offset: u32,
        limit: u32,
    ) -> Vec<SpendingAllowance>;

    /// Get all allowances for a merchant
    fn get_merchant_allowances(env: Env, merchant: Address) -> Vec<SpendingAllowance>;

    /// Verify consent is valid
    fn verify_consent(env: Env, consent_id: u32) -> bool;

    /// Update allowance limits
    fn update_allowance_limits(
        env: Env,
        allowance_id: u32,
        per_transaction_limit: i128,
        daily_limit: i128,
    );
}
