#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    token_addr: Address,
    token_client: TokenClient<'a>,
    tac: TokenAdminClient<'a>,
    admin: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let tac = TokenAdminClient::new(&env, &token_addr);
    client.initialize(&admin, &admin, &0u32);
    client.set_min_collateral(&0i128);
    Setup {
        env,
        client,
        token_addr,
        token_client,
        tac,
        admin,
    }
}

fn approve(s: &Setup<'_>, customer: &Address, amount: i128) {
    s.token_client.approve(
        customer,
        &s.client.address,
        &amount,
        &(s.env.ledger().sequence() + 1000),
    );
}

#[test]
fn test_customer_cancel_authorized_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.client.cancel_payment(&customer, &pid);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::CustomerCancelled);
    assert_eq!(s.token_client.balance(&customer), 1_000);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_cancel_rejected_after_capture() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    s.client.capture_payment(&merchant, &pid);

    let result = s.client.try_cancel_payment(&customer, &pid);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ExtError::NotAuthorizedForCancel.into()
    );
}

#[test]
fn test_cancel_rejected_by_non_owner() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let other = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    let result = s.client.try_cancel_payment(&other, &pid);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ExtError::NotPaymentCustomer.into()
    );
}

#[test]
fn test_cancel_rejected_past_capture_deadline() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.env.ledger().set_sequence_number(200);
    let result = s.client.try_cancel_payment(&customer, &pid);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ExtError::CancelPastCaptureDeadline.into()
    );
}

#[test]
fn test_capture_rejected_after_customer_cancel() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    s.client.cancel_payment(&customer, &pid);

    let result = s.client.try_capture_payment(&merchant, &pid);
    assert!(result.is_err());
}

#[test]
fn test_get_customer_refunds_pagination_and_cap() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &10_000);
    approve(&s, &customer, 10_000);

    s.env.ledger().set_sequence_number(100);
    let mut ids = soroban_sdk::Vec::new(&s.env);
    for _ in 0..3 {
        let pid = s
            .client
            .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
        s.client.cancel_payment(&customer, &pid);
        ids.push_back(pid);
    }

    // Index includes withdrawn (customer-cancelled) requests.
    let page = s.client.get_customer_refunds(&customer, &0u32, &10u32);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap(), ids.get(0).unwrap());
    assert_eq!(page.get(2).unwrap(), ids.get(2).unwrap());

    // Pagination window.
    let page = s.client.get_customer_refunds(&customer, &1u32, &1u32);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), ids.get(1).unwrap());

    // Page size is capped.
    let capped = s.client.get_customer_refunds(&customer, &0u32, &u32::MAX);
    assert!(capped.len() <= 50);
}

#[test]
#[should_panic(expected = "Payment not found")]
fn test_cancel_rejected_for_nonexistent_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);

    s.client.cancel_payment(&customer, &999u32);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_cancel_rejected_when_contract_paused() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));

    s.client.cancel_payment(&customer, &pid);
}

#[test]
fn test_cancel_succeeds_one_ledger_before_deadline() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.env.ledger().set_sequence_number(199);
    s.client.cancel_payment(&customer, &pid);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::CustomerCancelled);
    assert_eq!(s.token_client.balance(&customer), 1_000);
}

#[test]
fn test_cancel_rejected_at_exact_deadline_boundary() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    // Sequence equal to capture_deadline is already past the allowed window.
    s.env.ledger().set_sequence_number(200);
    let result = s.client.try_cancel_payment(&customer, &pid);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ExtError::CancelPastCaptureDeadline.into()
    );
}

#[test]
fn test_cancel_twice_rejected() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    s.client.cancel_payment(&customer, &pid);

    let result = s.client.try_cancel_payment(&customer, &pid);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ExtError::NotAuthorizedForCancel.into()
    );
}

#[test]
fn test_cancel_one_payment_leaves_other_authorized_payment_untouched() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &2_000);
    approve(&s, &customer, 2_000);

    s.env.ledger().set_sequence_number(100);
    let pid_a = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    let pid_b = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &500, &200u64);

    s.client.cancel_payment(&customer, &pid_a);

    let payment_a = s.client.get_payment(&pid_a);
    let payment_b = s.client.get_payment(&pid_b);
    assert_eq!(payment_a.status, PaymentStatus::CustomerCancelled);
    assert_eq!(payment_b.status, PaymentStatus::Authorized);
    // Only pid_a's escrow (300) was refunded; pid_b's escrow (500) stays locked.
    assert_eq!(s.token_client.balance(&customer), 2_000 - 500);
    assert_eq!(s.token_client.balance(&s.client.address), 500);
}

#[test]
fn test_get_customer_refunds_excludes_still_authorized_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &2_000);
    approve(&s, &customer, 2_000);

    s.env.ledger().set_sequence_number(100);
    let pid_cancelled = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    let pid_authorized = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &500, &200u64);

    s.client.cancel_payment(&customer, &pid_cancelled);

    let refunds = s.client.get_customer_refunds(&customer, &0u32, &10u32);
    assert_eq!(refunds.len(), 1);
    assert_eq!(refunds.get(0).unwrap(), pid_cancelled);
    assert!(!refunds.iter().any(|id| id == pid_authorized));
}

#[test]
fn test_cancel_sets_refunded_amount_to_full_amount() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    s.client.cancel_payment(&customer, &pid);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.refunded_amount, payment.amount);
    assert_eq!(payment.refunded_amount, 300);
}

#[test]
fn test_cancel_preserves_payment_fields_other_than_status() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    let before = s.client.get_payment(&pid);

    s.client.cancel_payment(&customer, &pid);
    let after = s.client.get_payment(&pid);

    assert_eq!(after.id, before.id);
    assert_eq!(after.customer, before.customer);
    assert_eq!(after.merchant, before.merchant);
    assert_eq!(after.token, before.token);
    assert_eq!(after.amount, before.amount);
    assert_eq!(after.capture_deadline, before.capture_deadline);
    assert_ne!(after.status, before.status);
}

#[test]
fn test_cancel_rejected_after_void_authorization() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.tac.mint(&customer, &1_000);
    approve(&s, &customer, 1_000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    s.client.void_authorization(&customer, &pid);

    let result = s.client.try_cancel_payment(&customer, &pid);
    assert_eq!(
        result.unwrap_err().unwrap(),
        ExtError::NotAuthorizedForCancel.into()
    );
    // Funds were already returned by void_authorization; cancel must not double-refund.
    assert_eq!(s.token_client.balance(&customer), 1_000);
}
