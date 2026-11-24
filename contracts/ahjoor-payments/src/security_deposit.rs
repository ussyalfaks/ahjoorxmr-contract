//! #1167: Refundable security deposits with itemized deductions.
//!
//! Lifecycle:
//! 1. `create_security_deposit` — the customer funds a deposit held on the
//!    contract until `return_by` (ledger timestamp). The merchant co-signs to
//!    accept the terms.
//! 2. Before `return_by` the merchant either returns the full deposit
//!    (`release_deposit`) or proposes itemized `claim_deductions`.
//! 3. The customer `accept_deductions` (settles immediately) or
//!    `dispute_deductions` within [`DEPOSIT_RESPONSE_WINDOW_SECONDS`]; a
//!    dispute is settled by the admin via `resolve_deposit_dispute`. If the
//!    customer does not respond in time, the merchant may
//!    `finalize_deductions`.
//! 4. If the merchant does nothing by `return_by`, the customer can
//!    `reclaim_deposit` for the full amount.

use crate::{
    AhjoorPaymentsContract, AhjoorPaymentsContractArgs, AhjoorPaymentsContractClient, DataKey,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{
    contracterror, contractevent, contractimpl, contracttype, panic_with_error, token, Address,
    BytesN, Env, Vec,
};

/// Maximum number of itemized deductions per claim.
pub const MAX_DEPOSIT_DEDUCTIONS: u32 = 20;

/// Time the customer has to accept or dispute proposed deductions before the
/// merchant may finalize them.
pub const DEPOSIT_RESPONSE_WINDOW_SECONDS: u64 = 7 * 24 * 60 * 60;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositError {
    /// Deposit amount must be positive.
    InvalidDepositAmount = 100,
    /// `return_by` must be in the future.
    InvalidReturnBy = 101,
    /// Merchant and customer must be different addresses.
    InvalidDepositParties = 102,
    /// No deposit with this ID.
    DepositNotFound = 103,
    /// Caller is not the deposit's merchant.
    NotDepositMerchant = 104,
    /// Caller is not the deposit's customer.
    NotDepositCustomer = 105,
    /// Deposit is not in the `Held` state.
    DepositNotHeld = 106,
    /// `return_by` has passed; the merchant can no longer claim or release.
    ReturnDeadlinePassed = 107,
    /// `return_by` has not passed; the customer cannot reclaim yet.
    ReturnDeadlineNotReached = 108,
    /// Deductions list is empty or contains a non-positive amount.
    InvalidDeductions = 109,
    /// Deductions total more than the deposit.
    DeductionsExceedDeposit = 110,
    /// More than `MAX_DEPOSIT_DEDUCTIONS` items.
    TooManyDeductions = 111,
    /// Deposit has no pending deduction proposal.
    NoDeductionsProposed = 112,
    /// The customer's response window is still open.
    ResponseWindowOpen = 113,
    /// The customer's response window has closed.
    ResponseWindowClosed = 114,
    /// Deposit is not disputed.
    DepositNotDisputed = 115,
    /// Resolution amount is negative or above the claimed deductions.
    InvalidDepositResolution = 116,
    /// Caller is not the contract admin.
    NotAdmin = 117,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DepositStatus {
    /// Funds held; merchant may claim deductions or release before `return_by`.
    Held = 0,
    /// Merchant proposed deductions; awaiting the customer.
    DeductionsProposed = 1,
    /// Customer disputed the deductions; awaiting admin resolution.
    Disputed = 2,
    /// Paid out (deductions to merchant, remainder to customer).
    Settled = 3,
    /// Customer reclaimed the full deposit after `return_by`.
    Reclaimed = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Deduction {
    /// Hash of the off-chain reason/evidence for this item.
    pub reason_hash: BytesN<32>,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityDeposit {
    pub id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub token: Address,
    pub amount: i128,
    /// Ledger timestamp by which the merchant must act.
    pub return_by: u64,
    pub status: DepositStatus,
    pub deductions: Vec<Deduction>,
    pub deductions_total: i128,
    /// Ledger timestamp of `claim_deductions` (0 if none).
    pub proposed_at: u64,
    pub created_at: u64,
    /// Amount paid to the merchant at settlement.
    pub merchant_paid: i128,
    /// Amount returned to the customer at settlement/reclaim.
    pub customer_refunded: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DepositKey {
    Counter,
    Deposit(u32),
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct SecurityDepositCreated {
    pub deposit_id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub token: Address,
    pub amount: i128,
    pub return_by: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct DepositDeductionsClaimed {
    pub deposit_id: u32,
    pub total: i128,
    pub items: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct DepositDeductionsDisputed {
    pub deposit_id: u32,
    pub customer: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct SecurityDepositSettled {
    pub deposit_id: u32,
    pub merchant_paid: i128,
    pub customer_refunded: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct SecurityDepositReclaimed {
    pub deposit_id: u32,
    pub customer: Address,
    pub amount: i128,
}

fn load(env: &Env, deposit_id: u32) -> SecurityDeposit {
    match env
        .storage()
        .persistent()
        .get(&DepositKey::Deposit(deposit_id))
    {
        Some(d) => d,
        None => panic_with_error!(env, DepositError::DepositNotFound),
    }
}

fn save(env: &Env, deposit: &SecurityDeposit) {
    let key = DepositKey::Deposit(deposit.id);
    env.storage().persistent().set(&key, deposit);
    env.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn require_merchant(env: &Env, deposit: &SecurityDeposit, merchant: &Address) {
    merchant.require_auth();
    if deposit.merchant != *merchant {
        panic_with_error!(env, DepositError::NotDepositMerchant);
    }
}

fn require_customer(env: &Env, deposit: &SecurityDeposit, customer: &Address) {
    customer.require_auth();
    if deposit.customer != *customer {
        panic_with_error!(env, DepositError::NotDepositCustomer);
    }
}

fn response_deadline(deposit: &SecurityDeposit) -> u64 {
    deposit
        .proposed_at
        .saturating_add(DEPOSIT_RESPONSE_WINDOW_SECONDS)
}

#[contractimpl]
impl AhjoorPaymentsContract {
    /// Pays `merchant_amount` to the merchant's payout address and the rest
    /// of the deposit back to the customer, then marks it `Settled`.
    fn settle_deposit(env: &Env, deposit: &mut SecurityDeposit, merchant_amount: i128) {
        let client = token::Client::new(env, &deposit.token);
        let to_customer = deposit.amount - merchant_amount;
        if merchant_amount > 0 {
            client.transfer(
                &env.current_contract_address(),
                Self::resolve_payout_address(env, &deposit.merchant),
                &merchant_amount,
            );
        }
        if to_customer > 0 {
            client.transfer(
                &env.current_contract_address(),
                &deposit.customer,
                &to_customer,
            );
        }
        deposit.status = DepositStatus::Settled;
        deposit.merchant_paid = merchant_amount;
        deposit.customer_refunded = to_customer;
        save(env, deposit);
        SecurityDepositSettled {
            deposit_id: deposit.id,
            merchant_paid: merchant_amount,
            customer_refunded: to_customer,
        }
        .publish(env);
    }

    /// #1167: Customer funds a refundable security deposit for `merchant`,
    /// held until `return_by` (ledger timestamp). Both parties authorize.
    /// Returns the deposit ID.
    pub fn create_security_deposit(
        env: Env,
        merchant: Address,
        customer: Address,
        token: Address,
        amount: i128,
        return_by: u64,
    ) -> u32 {
        Self::require_not_paused(&env);
        // Checked before auth: requiring auth twice for one address fails.
        if merchant == customer {
            panic_with_error!(&env, DepositError::InvalidDepositParties);
        }
        merchant.require_auth();
        customer.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, DepositError::InvalidDepositAmount);
        }
        let now = env.ledger().timestamp();
        if return_by <= now {
            panic_with_error!(&env, DepositError::InvalidReturnBy);
        }
        Self::require_token_accepted(&env, &merchant, &token);
        Self::require_merchant_approved(&env, &merchant);

        token::Client::new(&env, &token).transfer(
            &customer,
            env.current_contract_address(),
            &amount,
        );

        let id: u32 = env
            .storage()
            .instance()
            .get(&DepositKey::Counter)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DepositKey::Counter, &(id + 1));

        let deposit = SecurityDeposit {
            id,
            merchant: merchant.clone(),
            customer: customer.clone(),
            token: token.clone(),
            amount,
            return_by,
            status: DepositStatus::Held,
            deductions: Vec::new(&env),
            deductions_total: 0,
            proposed_at: 0,
            created_at: now,
            merchant_paid: 0,
            customer_refunded: 0,
        };
        save(&env, &deposit);
        SecurityDepositCreated {
            deposit_id: id,
            merchant,
            customer,
            token,
            amount,
            return_by,
        }
        .publish(&env);
        id
    }

    /// #1167: Merchant proposes itemized deductions `(reason_hash, amount)`
    /// on a held deposit, on or before `return_by`.
    pub fn claim_deductions(
        env: Env,
        merchant: Address,
        deposit_id: u32,
        deductions: Vec<(BytesN<32>, i128)>,
    ) {
        Self::require_not_paused(&env);
        let mut deposit = load(&env, deposit_id);
        require_merchant(&env, &deposit, &merchant);

        if deposit.status != DepositStatus::Held {
            panic_with_error!(&env, DepositError::DepositNotHeld);
        }
        let now = env.ledger().timestamp();
        if now > deposit.return_by {
            panic_with_error!(&env, DepositError::ReturnDeadlinePassed);
        }
        if deductions.is_empty() {
            panic_with_error!(&env, DepositError::InvalidDeductions);
        }
        if deductions.len() > MAX_DEPOSIT_DEDUCTIONS {
            panic_with_error!(&env, DepositError::TooManyDeductions);
        }

        let mut items: Vec<Deduction> = Vec::new(&env);
        let mut total: i128 = 0;
        for (reason_hash, amount) in deductions.iter() {
            if amount <= 0 {
                panic_with_error!(&env, DepositError::InvalidDeductions);
            }
            total = match total.checked_add(amount) {
                Some(t) => t,
                None => panic_with_error!(&env, DepositError::DeductionsExceedDeposit),
            };
            items.push_back(Deduction {
                reason_hash,
                amount,
            });
        }
        if total > deposit.amount {
            panic_with_error!(&env, DepositError::DeductionsExceedDeposit);
        }

        let count = items.len();
        deposit.deductions = items;
        deposit.deductions_total = total;
        deposit.proposed_at = now;
        deposit.status = DepositStatus::DeductionsProposed;
        save(&env, &deposit);
        DepositDeductionsClaimed {
            deposit_id,
            total,
            items: count,
        }
        .publish(&env);
    }

    /// #1167: Merchant returns the full deposit with no deductions.
    pub fn release_deposit(env: Env, merchant: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        let mut deposit = load(&env, deposit_id);
        require_merchant(&env, &deposit, &merchant);
        if deposit.status != DepositStatus::Held {
            panic_with_error!(&env, DepositError::DepositNotHeld);
        }
        Self::settle_deposit(&env, &mut deposit, 0);
    }

    /// #1167: Customer accepts the proposed deductions; the deposit settles.
    pub fn accept_deductions(env: Env, customer: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        let mut deposit = load(&env, deposit_id);
        require_customer(&env, &deposit, &customer);
        if deposit.status != DepositStatus::DeductionsProposed {
            panic_with_error!(&env, DepositError::NoDeductionsProposed);
        }
        let total = deposit.deductions_total;
        Self::settle_deposit(&env, &mut deposit, total);
    }

    /// #1167: Customer disputes the proposed deductions within the response
    /// window; the admin then settles via `resolve_deposit_dispute`.
    pub fn dispute_deductions(env: Env, customer: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        let mut deposit = load(&env, deposit_id);
        require_customer(&env, &deposit, &customer);
        if deposit.status != DepositStatus::DeductionsProposed {
            panic_with_error!(&env, DepositError::NoDeductionsProposed);
        }
        if env.ledger().timestamp() > response_deadline(&deposit) {
            panic_with_error!(&env, DepositError::ResponseWindowClosed);
        }
        deposit.status = DepositStatus::Disputed;
        save(&env, &deposit);
        DepositDeductionsDisputed {
            deposit_id,
            customer,
        }
        .publish(&env);
    }

    /// #1167: Merchant settles proposed deductions the customer neither
    /// accepted nor disputed within the response window.
    pub fn finalize_deductions(env: Env, merchant: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        let mut deposit = load(&env, deposit_id);
        require_merchant(&env, &deposit, &merchant);
        if deposit.status != DepositStatus::DeductionsProposed {
            panic_with_error!(&env, DepositError::NoDeductionsProposed);
        }
        if env.ledger().timestamp() <= response_deadline(&deposit) {
            panic_with_error!(&env, DepositError::ResponseWindowOpen);
        }
        let total = deposit.deductions_total;
        Self::settle_deposit(&env, &mut deposit, total);
    }

    /// #1167: Admin settles a disputed deposit, awarding `merchant_amount`
    /// (at most the claimed deductions) to the merchant and the rest to the
    /// customer.
    pub fn resolve_deposit_dispute(
        env: Env,
        admin: Address,
        deposit_id: u32,
        merchant_amount: i128,
    ) {
        Self::require_not_paused(&env);
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("Not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, DepositError::NotAdmin);
        }
        let mut deposit = load(&env, deposit_id);
        if deposit.status != DepositStatus::Disputed {
            panic_with_error!(&env, DepositError::DepositNotDisputed);
        }
        if merchant_amount < 0 || merchant_amount > deposit.deductions_total {
            panic_with_error!(&env, DepositError::InvalidDepositResolution);
        }
        Self::settle_deposit(&env, &mut deposit, merchant_amount);
    }

    /// #1167: Customer reclaims the full deposit when the merchant did
    /// nothing by `return_by`.
    pub fn reclaim_deposit(env: Env, customer: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        let mut deposit = load(&env, deposit_id);
        require_customer(&env, &deposit, &customer);
        if deposit.status != DepositStatus::Held {
            panic_with_error!(&env, DepositError::DepositNotHeld);
        }
        if env.ledger().timestamp() <= deposit.return_by {
            panic_with_error!(&env, DepositError::ReturnDeadlineNotReached);
        }
        token::Client::new(&env, &deposit.token).transfer(
            &env.current_contract_address(),
            &deposit.customer,
            &deposit.amount,
        );
        deposit.status = DepositStatus::Reclaimed;
        deposit.customer_refunded = deposit.amount;
        save(&env, &deposit);
        SecurityDepositReclaimed {
            deposit_id,
            customer,
            amount: deposit.amount,
        }
        .publish(&env);
    }

    /// #1167: Look up a security deposit.
    pub fn get_security_deposit(env: Env, deposit_id: u32) -> SecurityDeposit {
        load(&env, deposit_id)
    }
}
