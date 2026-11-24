#![cfg(test)]
//! #1168: customers reclaim payments the merchant never marked fulfilled.
use super::*;
use crate::fulfilment::FulfilmentError;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

const START: u64 = 10_000;
const WINDOW: u64 = 3_600;
const DEADLINE: u64 = START + WINDOW;
const AMOUNT: i128 = 500;

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    token: Address,
    token_client: TokenClient<'a>,
    customer: Address,
    merchant: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);
    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.initialize(&admin, &Address::generate(&env), &0);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    TokenAdminClient::new(&env, &token).mint(&customer, &10_000);
    let token_client = TokenClient::new(&env, &token);
    Setup {
        env,
        client,
        token: token.clone(),
        token_client,
        customer,
        merchant,
    }
}

impl<'a> Setup<'a> {
    fn create(&self) -> u32 {
        self.client.create_payment_with_deadline(
            &self.customer,
            &self.merchant,
            &AMOUNT,
            &self.token,
            &DEADLINE,
            &None,
            &None,
        )
    }

    fn create_plain(&self) -> u32 {
        self.client.create_payment(
            &self.customer,
            &self.merchant,
            &AMOUNT,
            &self.token,
            &None,
            &None,
            &None,
        )
    }

    fn past_deadline(&self) {
        self.env.ledger().set_timestamp(DEADLINE + 1);
    }

    fn dispute(&self, id: u32) {
        self.client
            .dispute_payment(&self.customer, &id, &String::from_str(&self.env, "no show"));
    }
}

fn assert_err<T: core::fmt::Debug>(
    result: Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: FulfilmentError,
) {
    assert_eq!(result.unwrap_err().unwrap(), expected.into());
}

// --- create_payment_with_deadline ---

#[test]
fn test_create_stores_deadline() {
    let s = setup();
    let id = s.create();
    let info = s.client.get_fulfilment_info(&id).unwrap();
    assert_eq!(info.deadline, DEADLINE);
    assert!(!info.fulfilled);
    assert_eq!(info.fulfilled_at, 0);
    assert_eq!(s.client.get_payment(&id).status, PaymentStatus::Pending);
    assert_eq!(s.token_client.balance(&s.client.address), AMOUNT);
}

#[test]
fn test_plain_payment_has_no_fulfilment_info() {
    let s = setup();
    let id = s.create_plain();
    assert_eq!(s.client.get_fulfilment_info(&id), None);
}

#[test]
fn test_create_rejects_past_or_current_deadline() {
    let s = setup();
    let result = s.client.try_create_payment_with_deadline(
        &s.customer,
        &s.merchant,
        &AMOUNT,
        &s.token,
        &START,
        &None,
        &None,
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        FulfilmentError::InvalidFulfilmentDeadline.into()
    );
}

// --- happy paths ---

#[test]
fn test_reclaim_unfulfilled_refunds_customer_in_full() {
    let s = setup();
    let id = s.create();
    s.past_deadline();

    let refunded = s.client.reclaim_unfulfilled(&s.customer, &id);

    assert_eq!(refunded, AMOUNT);
    assert_eq!(s.token_client.balance(&s.customer), 10_000);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
    let payment = s.client.get_payment(&id);
    assert_eq!(payment.status, PaymentStatus::Refunded);
    assert_eq!(payment.refunded_amount, AMOUNT);
}

#[test]
fn test_mark_fulfilled_then_complete() {
    let s = setup();
    let id = s.create();
    s.env.ledger().set_timestamp(DEADLINE);
    s.client.mark_fulfilled(&s.merchant, &id);

    let info = s.client.get_fulfilment_info(&id).unwrap();
    assert!(info.fulfilled);
    assert_eq!(info.fulfilled_at, DEADLINE);

    s.client.complete_payment(&id);
    assert_eq!(s.token_client.balance(&s.merchant), AMOUNT);
}

// --- mark_fulfilled errors ---

#[test]
fn test_mark_fulfilled_wrong_merchant() {
    let s = setup();
    let id = s.create();
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_mark_fulfilled(&other, &id),
        FulfilmentError::NotFulfilmentMerchant,
    );
}

#[test]
fn test_mark_fulfilled_without_deadline() {
    let s = setup();
    let id = s.create_plain();
    assert_err(
        s.client.try_mark_fulfilled(&s.merchant, &id),
        FulfilmentError::NoFulfilmentDeadline,
    );
}

#[test]
fn test_mark_fulfilled_twice() {
    let s = setup();
    let id = s.create();
    s.client.mark_fulfilled(&s.merchant, &id);
    assert_err(
        s.client.try_mark_fulfilled(&s.merchant, &id),
        FulfilmentError::AlreadyFulfilled,
    );
}

#[test]
fn test_mark_fulfilled_after_deadline() {
    let s = setup();
    let id = s.create();
    s.past_deadline();
    assert_err(
        s.client.try_mark_fulfilled(&s.merchant, &id),
        FulfilmentError::FulfilmentDeadlinePassed,
    );
}

#[test]
fn test_mark_fulfilled_when_disputed() {
    let s = setup();
    let id = s.create();
    s.dispute(id);
    assert_err(
        s.client.try_mark_fulfilled(&s.merchant, &id),
        FulfilmentError::PaymentDisputed,
    );
}

#[test]
fn test_mark_fulfilled_when_settled() {
    let s = setup();
    let id = s.create();
    s.client.complete_payment(&id);
    assert_err(
        s.client.try_mark_fulfilled(&s.merchant, &id),
        FulfilmentError::PaymentNotReclaimable,
    );
}

// --- reclaim_unfulfilled errors ---

#[test]
fn test_reclaim_wrong_customer() {
    let s = setup();
    let id = s.create();
    s.past_deadline();
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_reclaim_unfulfilled(&other, &id),
        FulfilmentError::NotFulfilmentCustomer,
    );
}

#[test]
fn test_reclaim_without_deadline() {
    let s = setup();
    let id = s.create_plain();
    s.past_deadline();
    assert_err(
        s.client.try_reclaim_unfulfilled(&s.customer, &id),
        FulfilmentError::NoFulfilmentDeadline,
    );
}

#[test]
fn test_reclaim_after_fulfilled() {
    let s = setup();
    let id = s.create();
    s.client.mark_fulfilled(&s.merchant, &id);
    s.past_deadline();
    assert_err(
        s.client.try_reclaim_unfulfilled(&s.customer, &id),
        FulfilmentError::AlreadyFulfilled,
    );
}

#[test]
fn test_reclaim_before_deadline() {
    let s = setup();
    let id = s.create();
    s.env.ledger().set_timestamp(DEADLINE);
    assert_err(
        s.client.try_reclaim_unfulfilled(&s.customer, &id),
        FulfilmentError::FulfilmentDeadlineNotReached,
    );
}

#[test]
fn test_reclaim_when_disputed() {
    let s = setup();
    let id = s.create();
    s.dispute(id);
    s.past_deadline();
    assert_err(
        s.client.try_reclaim_unfulfilled(&s.customer, &id),
        FulfilmentError::PaymentDisputed,
    );
}

#[test]
fn test_reclaim_when_settled() {
    let s = setup();
    let id = s.create();
    s.past_deadline();
    s.client.complete_payment(&id);
    assert_err(
        s.client.try_reclaim_unfulfilled(&s.customer, &id),
        FulfilmentError::PaymentNotReclaimable,
    );
}

#[test]
fn test_reclaim_twice() {
    let s = setup();
    let id = s.create();
    s.past_deadline();
    s.client.reclaim_unfulfilled(&s.customer, &id);
    assert_err(
        s.client.try_reclaim_unfulfilled(&s.customer, &id),
        FulfilmentError::PaymentNotReclaimable,
    );
}
