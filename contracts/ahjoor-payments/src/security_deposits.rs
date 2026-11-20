//! #1167: refundable security deposits with itemized deductions.
//!
//! Lifecycle (`return_by` is a ledger sequence):
//!
//! ```text
//!            create_security_deposit
//!                      │
//!                      ▼
//!                    Held ──release_security_deposit──▶ Released   (merchant, any time)
//!                      │  └──reclaim_security_deposit──▶ Reclaimed (customer, after return_by)
//!   claim_deductions   │  (merchant, on or before return_by)
//!                      ▼
//!             DeductionsProposed ──accept_deductions──▶ Settled
//!                      │
//!   dispute_deductions │
//!                      ▼
//!                  Disputed ──resolve_deposit_dispute──▶ Settled  (admin)
//! ```
//!
//! The admin can also resolve a `DeductionsProposed` deposit, so funds are
//! not stranded if the customer never responds.

use crate::{
    events, AhjoorPaymentsContract, AhjoorPaymentsContractArgs, AhjoorPaymentsContractClient,
    DataKey, DataKey4, ExtError2, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
    PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{contractimpl, contracttype, panic_with_error, token, Address, BytesN, Env, Vec};

/// Maximum number of itemized deductions per claim.
pub const MAX_DEPOSIT_DEDUCTIONS: u32 = 20;

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecurityDepositStatus {
    /// Funds escrowed; awaiting return of the rental / equipment.
    Held = 0,
    /// Merchant proposed deductions; awaiting the customer's response.
    DeductionsProposed = 1,
    /// Customer disputed the deductions; awaiting admin resolution.
    Disputed = 2,
    /// Deductions paid to the merchant and the remainder returned.
    Settled = 3,
    /// Customer reclaimed the full deposit after `return_by`.
    Reclaimed = 4,
    /// Merchant returned the full deposit without deductions.
    Released = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SecurityDeposit {
    pub id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub token: Address,
    pub amount: i128,
    /// Last ledger (inclusive) on which the merchant may claim deductions;
    /// after it the customer may reclaim the full deposit.
    pub return_by: u32,
    pub created_ledger: u32,
    pub status: SecurityDepositStatus,
    /// Itemized deductions proposed by the merchant: (reason_hash, amount).
    pub deductions: Vec<(BytesN<32>, i128)>,
    /// Sum of the proposed deductions.
    pub proposed_total: i128,
    /// Amount finally paid to the merchant (set when the deposit closes).
    pub deducted_amount: i128,
    /// Amount finally returned to the customer (set when the deposit closes).
    pub returned_amount: i128,
}

impl AhjoorPaymentsContract {
    fn load_security_deposit(env: &Env, id: u32) -> SecurityDeposit {
        env.storage()
            .persistent()
            .get(&DataKey4::SecurityDeposit(id))
            .unwrap_or_else(|| panic_with_error!(env, ExtError2::SecurityDepositNotFound))
    }

    fn save_security_deposit(env: &Env, deposit: &SecurityDeposit) {
        let key = DataKey4::SecurityDeposit(deposit.id);
        env.storage().persistent().set(&key, deposit);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    /// Pays `to_merchant` to the merchant's payout address and the rest of
    /// the deposit back to the customer, then closes the deposit.
    fn close_security_deposit(
        env: &Env,
        deposit: &mut SecurityDeposit,
        to_merchant: i128,
        status: SecurityDepositStatus,
    ) {
        let to_customer = deposit.amount - to_merchant;
        let client = token::Client::new(env, &deposit.token);
        if to_merchant > 0 {
            client.transfer(
                &env.current_contract_address(),
                &Self::resolve_payout_address(env, &deposit.merchant),
                &to_merchant,
            );
        }
        if to_customer > 0 {
            client.transfer(
                &env.current_contract_address(),
                &deposit.customer,
                &to_customer,
            );
        }
        deposit.deducted_amount = to_merchant;
        deposit.returned_amount = to_customer;
        deposit.status = status;
        Self::save_security_deposit(env, deposit);

        events::emit_security_deposit_closed(env, deposit.id, status, to_merchant, to_customer);
    }
}

#[contractimpl]
impl AhjoorPaymentsContract {
    /// Opens a security deposit: `amount` of `token` is escrowed from the
    /// customer until `return_by` (ledger sequence). Requires both the
    /// merchant's and the customer's authorization. Returns the deposit ID.
    pub fn create_security_deposit(
        env: Env,
        merchant: Address,
        customer: Address,
        token: Address,
        amount: i128,
        return_by: u32,
    ) -> u32 {
        Self::require_not_paused(&env);
        // Checked before auth: requiring auth twice from the same address in
        // one invocation is a host error, which would mask this domain error.
        if merchant == customer {
            panic_with_error!(&env, ExtError2::InvalidDepositParties);
        }
        merchant.require_auth();
        customer.require_auth();

        if amount <= 0 {
            panic_with_error!(&env, ExtError2::InvalidDepositAmount);
        }
        let now_ledger = env.ledger().sequence();
        if return_by <= now_ledger {
            panic_with_error!(&env, ExtError2::InvalidReturnBy);
        }
        Self::require_merchant_approved(&env, &merchant);
        Self::require_token_accepted(&env, &merchant, &token);

        token::Client::new(&env, &token).transfer(
            &customer,
            &env.current_contract_address(),
            &amount,
        );

        let id: u32 = env
            .storage()
            .instance()
            .get(&DataKey4::SecurityDepositCounter)
            .unwrap_or(0u32)
            + 1;
        env.storage()
            .instance()
            .set(&DataKey4::SecurityDepositCounter, &id);

        let deposit = SecurityDeposit {
            id,
            merchant: merchant.clone(),
            customer: customer.clone(),
            token: token.clone(),
            amount,
            return_by,
            created_ledger: now_ledger,
            status: SecurityDepositStatus::Held,
            deductions: Vec::new(&env),
            proposed_total: 0,
            deducted_amount: 0,
            returned_amount: 0,
        };
        Self::save_security_deposit(&env, &deposit);

        events::emit_security_deposit_created(&env, id, merchant, customer, token, amount, return_by);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        id
    }

    /// Merchant proposes itemized deductions `(reason_hash, amount)` against
    /// a held deposit, on or before `return_by`. The customer then accepts or
    /// disputes them.
    pub fn claim_deductions(
        env: Env,
        merchant: Address,
        deposit_id: u32,
        deductions: Vec<(BytesN<32>, i128)>,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let mut deposit = Self::load_security_deposit(&env, deposit_id);
        if deposit.merchant != merchant {
            panic_with_error!(&env, ExtError2::NotDepositMerchant);
        }
        if deposit.status != SecurityDepositStatus::Held {
            panic_with_error!(&env, ExtError2::InvalidDepositStatus);
        }
        if env.ledger().sequence() > deposit.return_by {
            panic_with_error!(&env, ExtError2::ReturnDeadlinePassed);
        }
        if deductions.is_empty() || deductions.len() > MAX_DEPOSIT_DEDUCTIONS {
            panic_with_error!(&env, ExtError2::InvalidDeduction);
        }

        let mut total: i128 = 0;
        for (_, amount) in deductions.iter() {
            if amount <= 0 {
                panic_with_error!(&env, ExtError2::InvalidDeduction);
            }
            total = total
                .checked_add(amount)
                .unwrap_or_else(|| panic_with_error!(&env, ExtError2::DeductionsExceedDeposit));
        }
        if total > deposit.amount {
            panic_with_error!(&env, ExtError2::DeductionsExceedDeposit);
        }

        deposit.deductions = deductions;
        deposit.proposed_total = total;
        deposit.status = SecurityDepositStatus::DeductionsProposed;
        Self::save_security_deposit(&env, &deposit);

        events::emit_deposit_deductions_claimed(
            &env,
            deposit_id,
            deposit.deductions.len(),
            total,
        );
    }

    /// Customer accepts the proposed deductions: they are paid to the
    /// merchant and the remainder is returned to the customer.
    pub fn accept_deductions(env: Env, customer: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut deposit = Self::load_security_deposit(&env, deposit_id);
        if deposit.customer != customer {
            panic_with_error!(&env, ExtError2::NotDepositCustomer);
        }
        if deposit.status != SecurityDepositStatus::DeductionsProposed {
            panic_with_error!(&env, ExtError2::InvalidDepositStatus);
        }
        let to_merchant = deposit.proposed_total;
        Self::close_security_deposit(&env, &mut deposit, to_merchant, SecurityDepositStatus::Settled);
    }

    /// Customer disputes the proposed deductions; the admin decides the
    /// outcome with `resolve_deposit_dispute`.
    pub fn dispute_deductions(env: Env, customer: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut deposit = Self::load_security_deposit(&env, deposit_id);
        if deposit.customer != customer {
            panic_with_error!(&env, ExtError2::NotDepositCustomer);
        }
        if deposit.status != SecurityDepositStatus::DeductionsProposed {
            panic_with_error!(&env, ExtError2::InvalidDepositStatus);
        }
        deposit.status = SecurityDepositStatus::Disputed;
        Self::save_security_deposit(&env, &deposit);

        events::emit_deposit_deductions_disputed(&env, deposit_id, customer);
    }

    /// Admin settles a disputed (or unanswered) deduction claim, awarding
    /// `merchant_amount` (0 ..= proposed total) to the merchant and returning
    /// the rest of the deposit to the customer.
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
            panic_with_error!(&env, ExtError2::NotAdmin);
        }

        let mut deposit = Self::load_security_deposit(&env, deposit_id);
        if deposit.status != SecurityDepositStatus::Disputed
            && deposit.status != SecurityDepositStatus::DeductionsProposed
        {
            panic_with_error!(&env, ExtError2::InvalidDepositStatus);
        }
        if merchant_amount < 0 || merchant_amount > deposit.proposed_total {
            panic_with_error!(&env, ExtError2::InvalidDeduction);
        }
        Self::close_security_deposit(
            &env,
            &mut deposit,
            merchant_amount,
            SecurityDepositStatus::Settled,
        );
    }

    /// Customer reclaims the full deposit once `return_by` has passed without
    /// the merchant claiming any deductions.
    pub fn reclaim_security_deposit(env: Env, customer: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut deposit = Self::load_security_deposit(&env, deposit_id);
        if deposit.customer != customer {
            panic_with_error!(&env, ExtError2::NotDepositCustomer);
        }
        if deposit.status != SecurityDepositStatus::Held {
            panic_with_error!(&env, ExtError2::InvalidDepositStatus);
        }
        if env.ledger().sequence() <= deposit.return_by {
            panic_with_error!(&env, ExtError2::ReturnDeadlineNotReached);
        }
        Self::close_security_deposit(&env, &mut deposit, 0, SecurityDepositStatus::Reclaimed);
    }

    /// Merchant returns the full deposit without deductions.
    pub fn release_security_deposit(env: Env, merchant: Address, deposit_id: u32) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let mut deposit = Self::load_security_deposit(&env, deposit_id);
        if deposit.merchant != merchant {
            panic_with_error!(&env, ExtError2::NotDepositMerchant);
        }
        if deposit.status != SecurityDepositStatus::Held {
            panic_with_error!(&env, ExtError2::InvalidDepositStatus);
        }
        Self::close_security_deposit(&env, &mut deposit, 0, SecurityDepositStatus::Released);
    }

    /// Returns a security deposit by ID.
    pub fn get_security_deposit(env: Env, deposit_id: u32) -> SecurityDeposit {
        Self::load_security_deposit(&env, deposit_id)
    }
}
