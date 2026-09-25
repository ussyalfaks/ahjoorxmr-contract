#![allow(dead_code)]
use soroban_sdk::{
    panic_with_error, Address,
    BytesN, Env, Map, String, Symbol, Vec,
};
use crate::pre_approved_spending::*;

// Storage key symbols (used as tuple key prefixes to avoid format!)
const ALLOWANCE_COUNTER_KEY: &str = "allowance_counter";
const CONSENT_COUNTER_KEY: &str = "consent_counter";
const TRANSACTION_COUNTER_KEY: &str = "transaction_counter";
const AUDIT_LOG_COUNTER_KEY: &str = "audit_log_counter";

fn allowance_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "allowance"), id)
}
fn consent_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "consent"), id)
}
fn transaction_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "transaction"), id)
}
fn audit_log_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "audit_log"), id)
}
fn customer_allowances_key(env: &Env, customer: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "cust_allows"), customer.clone())
}
fn merchant_allowances_key(env: &Env, merchant: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "merch_allows"), merchant.clone())
}
fn allowance_transactions_key(env: &Env, allowance_id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "allow_txs"), allowance_id)
}
fn allowance_audit_logs_key(env: &Env, allowance_id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "allow_audit"), allowance_id)
}

/// Implementation of pre-approved spending functionality
pub struct PreApprovedSpendingImpl;

impl PreApprovedSpendingImpl {
    /// Create a new spending allowance with consent
    pub fn create_allowance(
        env: &Env,
        customer: Address,
        merchant: Address,
        token: Address,
        total_amount: i128,
        per_transaction_limit: i128,
        daily_limit: i128,
        expires_at: u64,
        consent_hash: BytesN<32>,
        consent_metadata: Map<String, String>,
    ) -> u32 {
        customer.require_auth();

        if total_amount <= 0 || per_transaction_limit <= 0 || daily_limit <= 0 {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        if per_transaction_limit > total_amount || daily_limit > total_amount {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        let now = env.ledger().timestamp();

        if expires_at <= now {
            panic_with_error!(env, SpendingAllowanceError::AllowanceExpired);
        }

        // Get next allowance ID
        let allowance_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, ALLOWANCE_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = allowance_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        });

        let allowance = SpendingAllowance {
            allowance_id: next_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            token,
            total_amount,
            amount_spent: 0,
            created_at: now,
            expires_at,
            status: AllowanceStatus::Active,
            consent_hash,
            consent_timestamp: now,
            consent_metadata,
            per_transaction_limit,
            daily_limit,
            daily_spent: 0,
            daily_reset_timestamp: now,
        };

        // Store allowance
        env.storage().persistent().set(&allowance_key(env, next_id), &allowance);
        env.storage()
            .instance()
            .set(&Symbol::new(env, ALLOWANCE_COUNTER_KEY), &next_id);

        // Add to customer allowances list
        let ckey = customer_allowances_key(env, &customer);
        let mut customer_allowances: Vec<u32> = env
            .storage()
            .persistent()
            .get(&ckey)
            .unwrap_or_else(|| Vec::new(env));
        customer_allowances.push_back(next_id);
        env.storage().persistent().set(&ckey, &customer_allowances);

        // Add to merchant allowances list
        let mkey = merchant_allowances_key(env, &merchant);
        let mut merchant_allowances: Vec<u32> = env
            .storage()
            .persistent()
            .get(&mkey)
            .unwrap_or_else(|| Vec::new(env));
        merchant_allowances.push_back(next_id);
        env.storage().persistent().set(&mkey, &merchant_allowances);

        // Log audit entry
        Self::log_audit_entry(
            env,
            next_id,
            AuditAction::Created,
            customer.clone(),
            "Allowance created",
        );

        next_id
    }

    /// Record consent for an allowance
    pub fn record_consent(
        env: &Env,
        customer: Address,
        merchant: Address,
        consent_type: ConsentType,
        consent_hash: BytesN<32>,
        ip_hash: BytesN<32>,
        device_hash: BytesN<32>,
        location_hash: BytesN<32>,
        expires_at: u64,
        metadata: Map<String, String>,
    ) -> u32 {
        customer.require_auth();

        let now = env.ledger().timestamp();

        if expires_at <= now {
            panic_with_error!(env, SpendingAllowanceError::ConsentExpired);
        }

        // Get next consent ID
        let consent_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, CONSENT_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = consent_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        });

        let consent = ConsentRecord {
            consent_id: next_id,
            customer,
            merchant,
            consent_type,
            consent_hash,
            timestamp: now,
            expires_at,
            ip_hash,
            device_hash,
            location_hash,
            status: ConsentStatus::Active,
            metadata,
        };

        // Store consent
        env.storage().persistent().set(&consent_key(env, next_id), &consent);
        env.storage()
            .instance()
            .set(&Symbol::new(env, CONSENT_COUNTER_KEY), &next_id);

        next_id
    }

    /// Spend from an allowance
    pub fn spend_from_allowance(
        env: &Env,
        allowance_id: u32,
        amount: i128,
        reference: String,
    ) -> AllowanceTransaction {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        let now = env.ledger().timestamp();

        // Check allowance status
        match allowance.status {
            AllowanceStatus::Revoked => {
                panic_with_error!(env, SpendingAllowanceError::AllowanceRevoked);
            }
            AllowanceStatus::Paused => {
                panic_with_error!(env, SpendingAllowanceError::AllowancePaused);
            }
            AllowanceStatus::Expired => {
                panic_with_error!(env, SpendingAllowanceError::AllowanceExpired);
            }
            AllowanceStatus::Exhausted => {
                panic_with_error!(env, SpendingAllowanceError::AllowanceExhausted);
            }
            _ => {}
        }

        // Check expiration
        if now > allowance.expires_at {
            allowance.status = AllowanceStatus::Expired;
            env.storage().persistent().set(&key, &allowance);
            panic_with_error!(env, SpendingAllowanceError::AllowanceExpired);
        }

        // Check per-transaction limit
        if amount > allowance.per_transaction_limit {
            panic_with_error!(env, SpendingAllowanceError::PerTransactionLimitExceeded);
        }

        // Reset daily limit if needed
        let day_in_seconds: u64 = 24 * 60 * 60;
        if now > allowance.daily_reset_timestamp + day_in_seconds {
            allowance.daily_spent = 0;
            allowance.daily_reset_timestamp = now;
        }

        // Check daily limit
        let new_daily_spent = allowance
            .daily_spent
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount));

        if new_daily_spent > allowance.daily_limit {
            panic_with_error!(env, SpendingAllowanceError::DailyLimitExceeded);
        }

        // Check total limit
        let new_total_spent = allowance
            .amount_spent
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount));

        if new_total_spent > allowance.total_amount {
            panic_with_error!(env, SpendingAllowanceError::AllowanceExhausted);
        }

        // Update allowance
        allowance.amount_spent = new_total_spent;
        allowance.daily_spent = new_daily_spent;

        if new_total_spent >= allowance.total_amount {
            allowance.status = AllowanceStatus::Exhausted;
        }

        // Get transaction ID
        let tx_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, TRANSACTION_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_tx_id = tx_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        });

        let transaction = AllowanceTransaction {
            tx_id: next_tx_id,
            allowance_id,
            amount,
            timestamp: now,
            status: TransactionStatus::Completed,
            reference,
        };

        // Store transaction
        env.storage().persistent().set(&transaction_key(env, next_tx_id), &transaction);

        let tx_key = allowance_transactions_key(env, allowance_id);
        let mut allowance_transactions: Vec<AllowanceTransaction> = env
            .storage()
            .persistent()
            .get(&tx_key)
            .unwrap_or_else(|| Vec::new(env));
        allowance_transactions.push_back(transaction.clone());
        env.storage().persistent().set(&tx_key, &allowance_transactions);

        // Store updated allowance
        env.storage().persistent().set(&key, &allowance);
        env.storage()
            .instance()
            .set(&Symbol::new(env, TRANSACTION_COUNTER_KEY), &next_tx_id);

        // Log audit entry
        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::TransactionApproved,
            allowance.customer.clone(),
            "Spent from allowance",
        );

        transaction
    }

    /// Get allowance details
    pub fn get_allowance(env: &Env, allowance_id: u32) -> Option<SpendingAllowance> {
        env.storage().persistent().get(&allowance_key(env, allowance_id))
    }

    /// Get consent record
    pub fn get_consent(env: &Env, consent_id: u32) -> Option<ConsentRecord> {
        env.storage().persistent().get(&consent_key(env, consent_id))
    }

    /// Pause an allowance
    pub fn pause_allowance(env: &Env, allowance_id: u32) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        allowance.status = AllowanceStatus::Paused;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Paused,
            allowance.customer.clone(),
            "Allowance paused",
        );
    }

    /// Resume a paused allowance
    pub fn resume_allowance(env: &Env, allowance_id: u32) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        if allowance.status != AllowanceStatus::Paused {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        allowance.status = AllowanceStatus::Active;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Resumed,
            allowance.customer.clone(),
            "Allowance resumed",
        );
    }

    /// Revoke an allowance
    pub fn revoke_allowance(env: &Env, allowance_id: u32) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        allowance.status = AllowanceStatus::Revoked;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Revoked,
            allowance.customer.clone(),
            "Allowance revoked",
        );
    }

    /// Revoke consent
    pub fn revoke_consent(env: &Env, consent_id: u32) {
        let key = consent_key(env, consent_id);
        let mut consent: ConsentRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::ConsentNotFound));

        consent.customer.require_auth();

        consent.status = ConsentStatus::Revoked;
        env.storage().persistent().set(&key, &consent);
    }

    /// Get remaining balance
    pub fn get_remaining_balance(env: &Env, allowance_id: u32) -> i128 {
        let allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&allowance_key(env, allowance_id))
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance
            .total_amount
            .checked_sub(allowance.amount_spent)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount))
    }

    /// Get daily remaining balance
    pub fn get_daily_remaining(env: &Env, allowance_id: u32) -> i128 {
        let allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&allowance_key(env, allowance_id))
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance
            .daily_limit
            .checked_sub(allowance.daily_spent)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount))
    }

    /// Get allowance transaction history
    pub fn get_allowance_transactions(env: &Env, allowance_id: u32) -> Vec<AllowanceTransaction> {
        env.storage()
            .persistent()
            .get(&allowance_transactions_key(env, allowance_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get audit log for an allowance
    pub fn get_audit_log(env: &Env, allowance_id: u32) -> Vec<AllowanceAuditLog> {
        env.storage()
            .persistent()
            .get(&allowance_audit_logs_key(env, allowance_id))
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get a page of a customer's allowances: at most `limit` entries starting
    /// at `offset` in creation order. A page past the end returns an empty Vec.
    pub fn get_customer_allowances(
        env: &Env,
        customer: Address,
        offset: u32,
        limit: u32,
    ) -> Vec<SpendingAllowance> {
        let mut allowances = Vec::new(env);
        let ckey = customer_allowances_key(env, &customer);

        if let Some(allowance_ids) = env.storage().persistent().get::<_, Vec<u32>>(&ckey) {
            let end = offset.saturating_add(limit).min(allowance_ids.len());
            for i in offset..end {
                let id = allowance_ids.get_unchecked(i);
                if let Some(allowance) = env.storage().persistent().get::<_, SpendingAllowance>(&allowance_key(env, id)) {
                    allowances.push_back(allowance);
                }
            }
        }

        allowances
    }

    /// Get all allowances for a merchant
    pub fn get_merchant_allowances(env: &Env, merchant: Address) -> Vec<SpendingAllowance> {
        let mut allowances = Vec::new(env);
        let mkey = merchant_allowances_key(env, &merchant);

        if let Some(allowance_ids) = env.storage().persistent().get::<_, Vec<u32>>(&mkey) {
            for id in allowance_ids.iter() {
                if let Some(allowance) = env.storage().persistent().get::<_, SpendingAllowance>(&allowance_key(env, id)) {
                    allowances.push_back(allowance);
                }
            }
        }

        allowances
    }

    /// Verify consent is valid
    pub fn verify_consent(env: &Env, consent_id: u32) -> bool {
        if let Some(consent) = env.storage().persistent().get::<_, ConsentRecord>(&consent_key(env, consent_id)) {
            let now = env.ledger().timestamp();
            consent.status == ConsentStatus::Active && now <= consent.expires_at
        } else {
            false
        }
    }

    /// Update allowance limits
    pub fn update_allowance_limits(
        env: &Env,
        allowance_id: u32,
        per_transaction_limit: i128,
        daily_limit: i128,
    ) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        if per_transaction_limit <= 0 || daily_limit <= 0 {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        if per_transaction_limit > allowance.total_amount || daily_limit > allowance.total_amount {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        allowance.per_transaction_limit = per_transaction_limit;
        allowance.daily_limit = daily_limit;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Modified,
            allowance.customer.clone(),
            "Allowance limits updated",
        );
    }

    /// Helper function to log audit entries
    fn log_audit_entry(
        env: &Env,
        allowance_id: u32,
        action: AuditAction,
        actor: Address,
        details: &str,
    ) {
        let log_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, AUDIT_LOG_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_log_id = log_id.checked_add(1).unwrap_or(log_id);

        let log = AllowanceAuditLog {
            log_id: next_log_id,
            allowance_id,
            action,
            actor,
            timestamp: env.ledger().timestamp(),
            details: String::from_str(env, details),
        };

        env.storage().persistent().set(&audit_log_key(env, next_log_id), &log);

        let audit_key = allowance_audit_logs_key(env, allowance_id);
        let mut allowance_logs: Vec<AllowanceAuditLog> = env
            .storage()
            .persistent()
            .get(&audit_key)
            .unwrap_or_else(|| Vec::new(env));
        allowance_logs.push_back(log.clone());
        env.storage().persistent().set(&audit_key, &allowance_logs);

        env.storage()
            .instance()
            .set(&Symbol::new(env, AUDIT_LOG_COUNTER_KEY), &next_log_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::AhjoorPaymentsContract;
    use soroban_sdk::testutils::Address as _;

    fn make_bytes32(env: &Env, seed: u8) -> BytesN<32> {
        let mut bytes = [0u8; 32];
        bytes[0] = seed;
        BytesN::from_array(env, &bytes)
    }

    #[test]
    fn test_allowance_history_and_audit_log_are_retrievable() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AhjoorPaymentsContract, ());

        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        let token = Address::generate(&env);
        let metadata = Map::new(&env);

        env.as_contract(&contract_id, || {
            let allowance_id = PreApprovedSpendingImpl::create_allowance(
                &env,
                customer.clone(),
                merchant.clone(),
                token.clone(),
                1000,
                200,
                500,
                1_000_000,
                make_bytes32(&env, 1),
                metadata,
            );

            let tx = PreApprovedSpendingImpl::spend_from_allowance(
                &env,
                allowance_id,
                125,
                String::from_str(&env, "invoice-1"),
            );

            let history = PreApprovedSpendingImpl::get_allowance_transactions(&env, allowance_id);
            let audit = PreApprovedSpendingImpl::get_audit_log(&env, allowance_id);

            assert_eq!(history.len(), 1);
            let first_tx = history.get(0).unwrap();
            assert_eq!(first_tx.tx_id, tx.tx_id);
            assert_eq!(first_tx.allowance_id, allowance_id);
            assert_eq!(audit.len(), 2);
            let first_log = audit.get(0).unwrap();
            assert_eq!(first_log.allowance_id, allowance_id);
        });
    }

    #[test]
    #[should_panic]
    fn test_update_allowance_limits_rejects_per_transaction_limit_above_total() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AhjoorPaymentsContract, ());

        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        let token = Address::generate(&env);
        let metadata = Map::new(&env);

        env.as_contract(&contract_id, || {
            let allowance_id = PreApprovedSpendingImpl::create_allowance(
                &env,
                customer,
                merchant,
                token,
                1000,
                200,
                500,
                1_000_000,
                make_bytes32(&env, 1),
                metadata,
            );

            PreApprovedSpendingImpl::update_allowance_limits(&env, allowance_id, 1500, 500);
        });
    }

    #[test]
    #[should_panic]
    fn test_update_allowance_limits_rejects_daily_limit_above_total() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AhjoorPaymentsContract, ());

        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        let token = Address::generate(&env);
        let metadata = Map::new(&env);

        env.as_contract(&contract_id, || {
            let allowance_id = PreApprovedSpendingImpl::create_allowance(
                &env,
                customer,
                merchant,
                token,
                1000,
                200,
                500,
                1_000_000,
                make_bytes32(&env, 1),
                metadata,
            );

            PreApprovedSpendingImpl::update_allowance_limits(&env, allowance_id, 200, 1500);
        });
    }

    #[test]
    fn test_update_allowance_limits_accepts_values_at_total_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AhjoorPaymentsContract, ());

        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);
        let token = Address::generate(&env);
        let metadata = Map::new(&env);

        env.as_contract(&contract_id, || {
            let allowance_id = PreApprovedSpendingImpl::create_allowance(
                &env,
                customer,
                merchant,
                token,
                1000,
                200,
                500,
                1_000_000,
                make_bytes32(&env, 1),
                metadata,
            );

            PreApprovedSpendingImpl::update_allowance_limits(&env, allowance_id, 1000, 1000);

            let allowance = PreApprovedSpendingImpl::get_allowance(&env, allowance_id).unwrap();
            assert_eq!(allowance.per_transaction_limit, 1000);
            assert_eq!(allowance.daily_limit, 1000);
        });
    }

    /// Creates `n` allowances for `customer`, each in its own contract frame
    /// (a frame may only consume the customer's auth once).
    fn create_n_allowances(env: &Env, contract_id: &Address, customer: &Address, n: u32) -> Vec<u32> {
        let merchant = Address::generate(env);
        let token = Address::generate(env);
        let mut ids = Vec::new(env);
        for i in 0..n {
            let id = env.as_contract(contract_id, || {
                PreApprovedSpendingImpl::create_allowance(
                    env,
                    customer.clone(),
                    merchant.clone(),
                    token.clone(),
                    1000,
                    200,
                    500,
                    1_000_000,
                    make_bytes32(env, i as u8),
                    Map::new(env),
                )
            });
            ids.push_back(id);
        }
        ids
    }

    #[test]
    fn test_get_customer_allowances_middle_page() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AhjoorPaymentsContract, ());
        let customer = Address::generate(&env);

        let ids = create_n_allowances(&env, &contract_id, &customer, 5);

        env.as_contract(&contract_id, || {

            let page = PreApprovedSpendingImpl::get_customer_allowances(&env, customer.clone(), 1, 2);
            assert_eq!(page.len(), 2);
            assert_eq!(page.get(0).unwrap().allowance_id, ids.get(1).unwrap());
            assert_eq!(page.get(1).unwrap().allowance_id, ids.get(2).unwrap());

            let all = PreApprovedSpendingImpl::get_customer_allowances(&env, customer.clone(), 0, 5);
            assert_eq!(all.len(), 5);
        });
    }

    #[test]
    fn test_get_customer_allowances_page_past_end() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(AhjoorPaymentsContract, ());
        let customer = Address::generate(&env);

        let ids = create_n_allowances(&env, &contract_id, &customer, 3);

        env.as_contract(&contract_id, || {

            // Partial page: runs past the end, returns only the remaining entry
            let partial = PreApprovedSpendingImpl::get_customer_allowances(&env, customer.clone(), 2, 10);
            assert_eq!(partial.len(), 1);
            assert_eq!(partial.get(0).unwrap().allowance_id, ids.get(2).unwrap());

            // Offset at and beyond the end returns an empty page
            assert_eq!(PreApprovedSpendingImpl::get_customer_allowances(&env, customer.clone(), 3, 10).len(), 0);
            assert_eq!(PreApprovedSpendingImpl::get_customer_allowances(&env, customer.clone(), 50, 10).len(), 0);

            // Zero limit returns nothing; huge limit does not overflow
            assert_eq!(PreApprovedSpendingImpl::get_customer_allowances(&env, customer.clone(), 0, 0).len(), 0);
            assert_eq!(PreApprovedSpendingImpl::get_customer_allowances(&env, customer.clone(), 1, u32::MAX).len(), 2);

            // Customer with no allowances
            let other = Address::generate(&env);
            assert_eq!(PreApprovedSpendingImpl::get_customer_allowances(&env, other, 0, 10).len(), 0);
        });
    }
}
