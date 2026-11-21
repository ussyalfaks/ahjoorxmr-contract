//! #1168: Fulfilment deadlines.
//!
//! A payment created with `create_payment_with_deadline` carries a
//! `fulfilment_deadline` (ledger timestamp). The merchant confirms delivery
//! with `mark_fulfilled` on or before the deadline. If the deadline passes
//! without fulfilment, the customer can call `reclaim_unfulfilled` for a full
//! refund instead of opening a dispute. Reclaiming is not possible once the
//! payment has been disputed or settled.

use crate::{
    events, AhjoorPaymentsContract, AhjoorPaymentsContractArgs, AhjoorPaymentsContractClient,
    DataKey, Payment, PaymentStatus, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
    PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{
    contracterror, contractevent, contractimpl, contracttype, panic_with_error, token, Address,
    Env, Map, String,
};

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FulfilmentError {
    /// `fulfilment_deadline` must be in the future.
    InvalidFulfilmentDeadline = 140,
    /// The payment has no fulfilment deadline.
    NoFulfilmentDeadline = 141,
    /// Caller is not the payment's merchant.
    NotFulfilmentMerchant = 142,
    /// Caller is not the payment's customer.
    NotFulfilmentCustomer = 143,
    /// The payment is already marked fulfilled.
    AlreadyFulfilled = 144,
    /// The fulfilment deadline has passed; it can no longer be marked fulfilled.
    FulfilmentDeadlinePassed = 145,
    /// The fulfilment deadline has not passed yet.
    FulfilmentDeadlineNotReached = 146,
    /// The payment is (or was) disputed.
    PaymentDisputed = 147,
    /// The payment is no longer pending (settled, refunded, expired, ...).
    PaymentNotReclaimable = 148,
}

/// Fulfilment tracking for one payment.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FulfilmentInfo {
    /// Ledger timestamp by which the merchant must call `mark_fulfilled`.
    pub deadline: u64,
    pub fulfilled: bool,
    /// Ledger timestamp of `mark_fulfilled` (0 while unfulfilled).
    pub fulfilled_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FulfilmentKey {
    Info(u32),
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct FulfilmentDeadlineSet {
    pub payment_id: u32,
    pub deadline: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct PaymentFulfilled {
    pub payment_id: u32,
    pub merchant: Address,
    pub fulfilled_at: u64,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct UnfulfilledPaymentReclaimed {
    pub payment_id: u32,
    pub customer: Address,
    pub amount: i128,
}

fn load_payment(env: &Env, payment_id: u32) -> Payment {
    env.storage()
        .persistent()
        .get(&DataKey::Payment(payment_id))
        .expect("Payment not found")
}

fn load_info(env: &Env, payment_id: u32) -> FulfilmentInfo {
    match env
        .storage()
        .persistent()
        .get(&FulfilmentKey::Info(payment_id))
    {
        Some(info) => info,
        None => panic_with_error!(env, FulfilmentError::NoFulfilmentDeadline),
    }
}

fn save_info(env: &Env, payment_id: u32, info: &FulfilmentInfo) {
    let key = FulfilmentKey::Info(payment_id);
    env.storage().persistent().set(&key, info);
    env.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

#[contractimpl]
impl AhjoorPaymentsContract {
    /// #1168: Create a payment the merchant must fulfil by
    /// `fulfilment_deadline` (ledger timestamp). Otherwise identical to
    /// `create_payment`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_payment_with_deadline(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        fulfilment_deadline: u64,
        reference: Option<String>,
        metadata: Option<Map<String, String>>,
    ) -> u32 {
        if fulfilment_deadline <= env.ledger().timestamp() {
            panic_with_error!(&env, FulfilmentError::InvalidFulfilmentDeadline);
        }

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
            None,
            None,
        );

        save_info(
            &env,
            payment_id,
            &FulfilmentInfo {
                deadline: fulfilment_deadline,
                fulfilled: false,
                fulfilled_at: 0,
            },
        );
        FulfilmentDeadlineSet {
            payment_id,
            deadline: fulfilment_deadline,
        }
        .publish(&env);

        payment_id
    }

    /// #1168: Merchant confirms fulfilment of a pending payment on or before
    /// its fulfilment deadline. Once fulfilled the customer can no longer
    /// reclaim the payment via `reclaim_unfulfilled`.
    pub fn mark_fulfilled(env: Env, merchant: Address, payment_id: u32) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let payment = load_payment(&env, payment_id);
        if payment.merchant != merchant {
            panic_with_error!(&env, FulfilmentError::NotFulfilmentMerchant);
        }
        let mut info = load_info(&env, payment_id);
        if info.fulfilled {
            panic_with_error!(&env, FulfilmentError::AlreadyFulfilled);
        }
        if payment.status == PaymentStatus::Disputed
            || payment.status == PaymentStatus::EscalatedDispute
        {
            panic_with_error!(&env, FulfilmentError::PaymentDisputed);
        }
        if payment.status != PaymentStatus::Pending {
            panic_with_error!(&env, FulfilmentError::PaymentNotReclaimable);
        }
        let now = env.ledger().timestamp();
        if now > info.deadline {
            panic_with_error!(&env, FulfilmentError::FulfilmentDeadlinePassed);
        }

        info.fulfilled = true;
        info.fulfilled_at = now;
        save_info(&env, payment_id, &info);

        PaymentFulfilled {
            payment_id,
            merchant,
            fulfilled_at: now,
        }
        .publish(&env);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// #1168: After the fulfilment deadline, a customer whose pending payment
    /// was never marked fulfilled reclaims the full remaining amount.
    ///
    /// Not available once the payment is disputed or settled (any status
    /// other than `Pending`).
    pub fn reclaim_unfulfilled(env: Env, customer: Address, payment_id: u32) -> i128 {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut payment = load_payment(&env, payment_id);
        if payment.customer != customer {
            panic_with_error!(&env, FulfilmentError::NotFulfilmentCustomer);
        }
        let info = load_info(&env, payment_id);
        if info.fulfilled {
            panic_with_error!(&env, FulfilmentError::AlreadyFulfilled);
        }
        if payment.status == PaymentStatus::Disputed
            || payment.status == PaymentStatus::EscalatedDispute
            || env.storage().temporary().has(&DataKey::Dispute(payment_id))
        {
            panic_with_error!(&env, FulfilmentError::PaymentDisputed);
        }
        let settled: bool = env
            .storage()
            .persistent()
            .get(&DataKey::Settled(payment_id))
            .unwrap_or(false);
        if payment.status != PaymentStatus::Pending || settled {
            panic_with_error!(&env, FulfilmentError::PaymentNotReclaimable);
        }
        if env.ledger().timestamp() <= info.deadline {
            panic_with_error!(&env, FulfilmentError::FulfilmentDeadlineNotReached);
        }

        let refund = payment.amount - payment.refunded_amount;
        if refund > 0 {
            token::Client::new(&env, &payment.token).transfer(
                &env.current_contract_address(),
                &payment.customer,
                &refund,
            );
        }

        let old_status = payment.status;
        payment.refunded_amount = payment.amount;
        payment.status = PaymentStatus::Refunded;
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Self::inc_global_refunded(&env, &payment.token, refund);
        Self::inc_merchant_refunded(&env, &payment.merchant, &payment.token, refund);

        UnfulfilledPaymentReclaimed {
            payment_id,
            customer,
            amount: refund,
        }
        .publish(&env);
        events::emit_payment_status_changed(&env, payment_id, old_status, PaymentStatus::Refunded);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        refund
    }

    /// #1168: Fulfilment state for a payment, or `None` if it has no deadline.
    pub fn get_fulfilment_info(env: Env, payment_id: u32) -> Option<FulfilmentInfo> {
        env.storage()
            .persistent()
            .get(&FulfilmentKey::Info(payment_id))
    }
}
