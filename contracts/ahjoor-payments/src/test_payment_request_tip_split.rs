#![cfg(test)]
//! Tests for reusable payment request links and tip split configuration.
//!
//! Covers `create_payment_request`, `pay_request`, `deactivate_payment_request`,
//! `get_payment_request`, `set_tip_split_config` and `remove_tip_split_config`.

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, testutils::Ledger, Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    contract_id: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin: TokenAdminClient<'a>,
    merchant: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &fee_recipient, &0);

    let merchant = Address::generate(&env);

    Setup {
        env,
        client,
        contract_id,
        token_addr,
        token_client,
        token_admin,
        merchant,
    }
}

fn funded_customer(s: &Setup, amount: i128) -> Address {
    let customer = Address::generate(&s.env);
    s.token_admin.mint(&customer, &amount);
    customer
}

// ---------------------------------------------------------------------------
// create_payment_request / get_payment_request
// ---------------------------------------------------------------------------

#[test]
fn test_create_payment_request_stores_active_request() {
    let s = setup();

    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &500, &Some(3), &None);

    let request = s.client.get_payment_request(&request_id).unwrap();
    assert_eq!(request.request_id, request_id);
    assert_eq!(request.merchant, s.merchant);
    assert_eq!(request.token, s.token_addr);
    assert_eq!(request.amount, 500);
    assert_eq!(request.max_uses, Some(3));
    assert_eq!(request.uses, 0);
    assert_eq!(request.expires_at, None);
    assert!(request.active);
}

#[test]
fn test_get_payment_request_unknown_returns_none() {
    let s = setup();
    assert_eq!(s.client.get_payment_request(&42), None);
}

#[test]
fn test_payment_request_ids_increment() {
    let s = setup();
    let first = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &100, &None, &None);
    let second = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &200, &None, &None);
    assert_eq!(second, first + 1);
}

#[test]
#[should_panic(expected = "Payment amount must be positive")]
fn test_create_payment_request_rejects_zero_amount() {
    let s = setup();
    s.client
        .create_payment_request(&s.merchant, &s.token_addr, &0, &None, &None);
}

// ---------------------------------------------------------------------------
// pay_request — active request
// ---------------------------------------------------------------------------

#[test]
fn test_pay_active_request_creates_linked_payment() {
    let s = setup();
    let customer = funded_customer(&s, 1_000);

    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &400, &None, &None);

    let payment_id = s.client.pay_request(&customer, &request_id);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.customer, customer);
    assert_eq!(payment.merchant, s.merchant);
    assert_eq!(payment.amount, 400);
    assert_eq!(payment.token, s.token_addr);
    assert_eq!(payment.status, PaymentStatus::Pending);

    // Funds are escrowed in the contract.
    assert_eq!(s.token_client.balance(&customer), 600);
    assert_eq!(s.token_client.balance(&s.contract_id), 400);

    // Payment is linked back to its request and the use counter advanced.
    assert_eq!(
        s.client.get_payment_request_for_payment(&payment_id),
        Some(request_id)
    );
    assert_eq!(s.client.get_payment_request(&request_id).unwrap().uses, 1);
}

// ---------------------------------------------------------------------------
// pay_request — reused request
// ---------------------------------------------------------------------------

#[test]
fn test_reuse_unlimited_request_creates_distinct_payments() {
    let s = setup();
    let alice = funded_customer(&s, 1_000);
    let bob = funded_customer(&s, 1_000);

    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &250, &None, &None);

    let p1 = s.client.pay_request(&alice, &request_id);
    let p2 = s.client.pay_request(&bob, &request_id);
    let p3 = s.client.pay_request(&alice, &request_id);

    assert_ne!(p1, p2);
    assert_ne!(p2, p3);
    assert_eq!(s.client.get_payment(&p1).customer, alice);
    assert_eq!(s.client.get_payment(&p2).customer, bob);
    assert_eq!(s.client.get_payment(&p3).customer, alice);

    for pid in [p1, p2, p3] {
        assert_eq!(
            s.client.get_payment_request_for_payment(&pid),
            Some(request_id)
        );
    }

    let request = s.client.get_payment_request(&request_id).unwrap();
    assert_eq!(request.uses, 3);
    assert!(request.active);
    assert_eq!(s.token_client.balance(&alice), 500);
    assert_eq!(s.token_client.balance(&bob), 750);
}

#[test]
fn test_reuse_request_up_to_max_uses() {
    let s = setup();
    let customer = funded_customer(&s, 1_000);

    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &100, &Some(2), &None);

    s.client.pay_request(&customer, &request_id);
    s.client.pay_request(&customer, &request_id);

    assert_eq!(s.client.get_payment_request(&request_id).unwrap().uses, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #86)")]
fn test_reuse_request_beyond_max_uses_rejected() {
    let s = setup();
    let customer = funded_customer(&s, 1_000);

    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &100, &Some(2), &None);

    s.client.pay_request(&customer, &request_id);
    s.client.pay_request(&customer, &request_id);
    // Third use exceeds max_uses → PaymentRequestExhausted
    s.client.pay_request(&customer, &request_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #84)")]
fn test_pay_expired_request_rejected() {
    let s = setup();
    let customer = funded_customer(&s, 1_000);

    let now = s.env.ledger().timestamp();
    let request_id = s.client.create_payment_request(
        &s.merchant,
        &s.token_addr,
        &100,
        &None,
        &Some(now + 100),
    );

    s.env.ledger().with_mut(|l| l.timestamp = now + 100);
    s.client.pay_request(&customer, &request_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #83)")]
fn test_pay_unknown_request_rejected() {
    let s = setup();
    let customer = funded_customer(&s, 1_000);
    s.client.pay_request(&customer, &999);
}

// ---------------------------------------------------------------------------
// pay_request — deactivated request
// ---------------------------------------------------------------------------

#[test]
fn test_deactivate_payment_request_marks_inactive() {
    let s = setup();
    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &100, &None, &None);

    s.client.deactivate_payment_request(&s.merchant, &request_id);

    let request = s.client.get_payment_request(&request_id).unwrap();
    assert!(!request.active);
}

#[test]
#[should_panic(expected = "Error(Contract, #85)")]
fn test_pay_deactivated_request_rejected() {
    let s = setup();
    let customer = funded_customer(&s, 1_000);

    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &100, &None, &None);

    s.client.deactivate_payment_request(&s.merchant, &request_id);
    // PaymentRequestInactive
    s.client.pay_request(&customer, &request_id);
}

#[test]
fn test_pay_deactivated_request_after_prior_use_leaves_state_unchanged() {
    let s = setup();
    let customer = funded_customer(&s, 1_000);

    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &100, &None, &None);

    s.client.pay_request(&customer, &request_id);
    s.client.deactivate_payment_request(&s.merchant, &request_id);

    let result = s.client.try_pay_request(&customer, &request_id);
    assert!(result.is_err());

    let request = s.client.get_payment_request(&request_id).unwrap();
    assert_eq!(request.uses, 1);
    assert!(!request.active);
    assert_eq!(s.token_client.balance(&customer), 900);
}

#[test]
#[should_panic(expected = "Only the payment request merchant can deactivate it")]
fn test_deactivate_by_other_merchant_rejected() {
    let s = setup();
    let request_id = s
        .client
        .create_payment_request(&s.merchant, &s.token_addr, &100, &None, &None);

    let other = Address::generate(&s.env);
    s.client.deactivate_payment_request(&other, &request_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #83)")]
fn test_deactivate_unknown_request_rejected() {
    let s = setup();
    s.client.deactivate_payment_request(&s.merchant, &7);
}

// ---------------------------------------------------------------------------
// set_tip_split_config
// ---------------------------------------------------------------------------

#[test]
fn test_set_tip_split_config_summing_to_100_percent() {
    let s = setup();
    let staff = Address::generate(&s.env);
    let kitchen = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((s.merchant.clone(), 5_000u32));
    split.push_back((staff.clone(), 3_000u32));
    split.push_back((kitchen.clone(), 2_000u32));

    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);

    let stored = s.client.get_tip_split_config(&s.merchant).unwrap();
    assert_eq!(stored.len(), 3);
    assert_eq!(stored.get(0).unwrap(), (s.merchant.clone(), 5_000));
    assert_eq!(stored.get(1).unwrap(), (staff, 3_000));
    assert_eq!(stored.get(2).unwrap(), (kitchen, 2_000));
}

#[test]
fn test_tip_split_config_distributes_tip_to_beneficiaries() {
    let s = setup();
    let staff = Address::generate(&s.env);
    let kitchen = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((staff.clone(), 6_000u32));
    split.push_back((kitchen.clone(), 4_000u32));
    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);

    let customer = funded_customer(&s, 1_100);
    let pid = s.client.create_payment_with_tipping(
        &customer,
        &s.merchant,
        &1_000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment_with_tip(&pid, &customer, &100);

    assert_eq!(s.token_client.balance(&staff), 60);
    assert_eq!(s.token_client.balance(&kitchen), 40);
    // Base amount (fee 0) still settles to the merchant; the tip does not.
    assert_eq!(s.token_client.balance(&s.merchant), 1_000);
}

#[test]
#[should_panic(expected = "Tip split basis points must sum to exactly 10,000")]
fn test_set_tip_split_config_under_100_percent_rejected() {
    let s = setup();
    let staff = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((s.merchant.clone(), 5_000u32));
    split.push_back((staff, 4_999u32));

    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);
}

#[test]
#[should_panic(expected = "Tip split basis points must sum to exactly 10,000")]
fn test_set_tip_split_config_over_100_percent_rejected() {
    let s = setup();
    let staff = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((s.merchant.clone(), 5_000u32));
    split.push_back((staff, 5_001u32));

    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);
}

#[test]
#[should_panic(expected = "Tip split configuration cannot be empty")]
fn test_set_tip_split_config_empty_rejected() {
    let s = setup();
    let split: Vec<(Address, u32)> = Vec::new(&s.env);
    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);
}

#[test]
#[should_panic(expected = "Only merchant can configure tip splits")]
fn test_set_tip_split_config_non_merchant_rejected() {
    let s = setup();
    let other = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((other.clone(), 10_000u32));

    s.client.set_tip_split_config(&s.merchant, &other, &split);
}

// ---------------------------------------------------------------------------
// remove_tip_split_config
// ---------------------------------------------------------------------------

#[test]
fn test_remove_tip_split_config_clears_config() {
    let s = setup();
    let staff = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((staff, 10_000u32));
    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);
    assert!(s.client.get_tip_split_config(&s.merchant).is_some());

    s.client.remove_tip_split_config(&s.merchant, &s.merchant);

    assert_eq!(s.client.get_tip_split_config(&s.merchant), None);
}

#[test]
fn test_remove_tip_split_config_reverts_tip_to_merchant() {
    let s = setup();
    let staff = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((staff.clone(), 10_000u32));
    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);
    s.client.remove_tip_split_config(&s.merchant, &s.merchant);

    let customer = funded_customer(&s, 1_100);
    let pid = s.client.create_payment_with_tipping(
        &customer,
        &s.merchant,
        &1_000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment_with_tip(&pid, &customer, &100);

    assert_eq!(s.token_client.balance(&staff), 0);
    assert_eq!(s.token_client.balance(&s.merchant), 1_100);
}

#[test]
#[should_panic(expected = "Only merchant can remove tip split configuration")]
fn test_remove_tip_split_config_non_merchant_rejected() {
    let s = setup();
    let staff = Address::generate(&s.env);

    let mut split = Vec::new(&s.env);
    split.push_back((staff, 10_000u32));
    s.client
        .set_tip_split_config(&s.merchant, &s.merchant, &split);

    let other = Address::generate(&s.env);
    s.client.remove_tip_split_config(&s.merchant, &other);
}
