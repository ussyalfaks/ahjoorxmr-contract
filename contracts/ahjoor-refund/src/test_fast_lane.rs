#![cfg(test)]
//! Trusted-customer refund fast lane (`set_fast_lane_criteria`,
//! `opt_in_fast_lane`, `opt_out_fast_lane`, `is_fast_lane_customer`).
use super::*;
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

const PAYMENT_AMOUNT: i128 = 1_000;
const DEFECTIVE: u32 = 0;

struct Setup<'a> {
    env: Env,
    refund_client: AhjoorRefundContractClient<'a>,
    payment_client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token: TokenClient<'a>,
    token_admin: TokenAdminClient<'a>,
    customer: Address,
    merchant: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token = TokenClient::new(&env, &token_addr);
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    // Escrow for refund requests.
    token_admin.mint(&customer, &PAYMENT_AMOUNT);

    Setup {
        env,
        refund_client,
        payment_client,
        admin,
        token,
        token_admin,
        customer,
        merchant,
    }
}

/// Creates and completes one payment from the setup customer; returns its id.
fn complete_payment(s: &Setup) -> u32 {
    s.token_admin.mint(&s.customer, &PAYMENT_AMOUNT);
    let pid = s.payment_client.create_payment(
        &s.customer,
        &s.merchant,
        &PAYMENT_AMOUNT,
        &s.token.address,
        &None,
        &None,
        &None,
    );
    s.payment_client.complete_payment(&pid);
    pid
}

fn complete_payments(s: &Setup, n: u32) -> u32 {
    let mut last = 0;
    for _ in 0..n {
        last = complete_payment(s);
    }
    last
}

fn request(s: &Setup, pid: u32, amount: i128) -> u32 {
    s.refund_client.request_refund(
        &s.customer,
        &pid,
        &amount,
        &String::from_str(&s.env, "reason"),
        &DEFECTIVE,
    )
}

fn status(s: &Setup, refund_id: u32) -> RefundStatus {
    s.refund_client.get_refund(&refund_id).status
}

/// Merchant opted in, criteria = 3 completed payments and fraud score 0.
fn enable(s: &Setup, max_amount: i128) {
    s.refund_client.set_fast_lane_criteria(&s.admin, &3u32, &0u32);
    s.refund_client.opt_in_fast_lane(&s.merchant, &max_amount);
}

/// Rejects one refund of the customer's, raising both fraud and abuse scores.
fn get_rejected(s: &Setup, pid: u32) {
    let rid = request(s, pid, 10);
    s.refund_client
        .reject_refund(&s.admin, &rid, &String::from_str(&s.env, "no"));
}

// ===========================================================================
//  Happy path
// ===========================================================================

#[test]
fn test_fast_lane_customer_is_auto_approved() {
    let s = setup();
    enable(&s, 500);
    let pid = complete_payments(&s, 3);
    assert!(s.refund_client.is_fast_lane_customer(&s.customer));

    let rid = request(&s, pid, 200);
    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Approved);
    assert_eq!(
        refund.auto_approved_source,
        Some(String::from_str(&s.env, "fast_lane"))
    );
}

#[test]
fn test_fast_lane_amount_equal_to_max_is_approved() {
    let s = setup();
    enable(&s, 200);
    let pid = complete_payments(&s, 3);
    let rid = request(&s, pid, 200);
    assert_eq!(status(&s, rid), RefundStatus::Approved);
}

#[test]
fn test_fast_lane_config_getters() {
    let s = setup();
    assert_eq!(s.refund_client.get_fast_lane_criteria(), None);
    assert_eq!(s.refund_client.get_fast_lane_max_amount(&s.merchant), None);

    enable(&s, 500);
    assert_eq!(
        s.refund_client.get_fast_lane_criteria(),
        Some(FastLaneCriteria {
            min_completed_payments: 3,
            max_fraud_score: 0
        })
    );
    assert_eq!(
        s.refund_client.get_fast_lane_max_amount(&s.merchant),
        Some(500)
    );
}

// ===========================================================================
//  Eligibility view: every way to fail
// ===========================================================================

#[test]
fn test_not_fast_lane_when_criteria_not_configured() {
    let s = setup();
    complete_payments(&s, 5);
    assert!(!s.refund_client.is_fast_lane_customer(&s.customer));
}

#[test]
fn test_not_fast_lane_with_too_few_payments() {
    let s = setup();
    enable(&s, 500);
    complete_payments(&s, 2);
    assert!(!s.refund_client.is_fast_lane_customer(&s.customer));
}

#[test]
fn test_not_fast_lane_when_customer_has_no_payments() {
    let s = setup();
    enable(&s, 500);
    assert!(!s.refund_client.is_fast_lane_customer(&s.customer));
}

#[test]
fn test_pending_payments_do_not_count() {
    let s = setup();
    enable(&s, 500);
    complete_payments(&s, 2);
    // Third payment is created but never completed.
    s.token_admin.mint(&s.customer, &PAYMENT_AMOUNT);
    s.payment_client.create_payment(
        &s.customer,
        &s.merchant,
        &PAYMENT_AMOUNT,
        &s.token.address,
        &None,
        &None,
        &None,
    );
    assert!(!s.refund_client.is_fast_lane_customer(&s.customer));
}

#[test]
fn test_not_fast_lane_with_abuse_score() {
    let s = setup();
    s.refund_client.set_fast_lane_criteria(&s.admin, &3u32, &100u32);
    let pid = complete_payments(&s, 3);
    assert!(s.refund_client.is_fast_lane_customer(&s.customer));

    get_rejected(&s, pid);
    // Fraud score is within the (generous) limit, but the abuse record is not clean.
    assert!(s.refund_client.get_customer_abuse_score(&s.customer).abuse_score > 0);
    assert!(!s.refund_client.is_fast_lane_customer(&s.customer));

    s.refund_client.reset_customer_abuse_score(&s.admin, &s.customer);
    assert!(s.refund_client.is_fast_lane_customer(&s.customer));
}

#[test]
fn test_not_fast_lane_with_high_fraud_score() {
    let s = setup();
    enable(&s, 500); // max fraud score 0
    let pid = complete_payments(&s, 3);

    get_rejected(&s, pid);
    // Clear the abuse record so only the fraud score can disqualify.
    s.refund_client.reset_customer_abuse_score(&s.admin, &s.customer);
    assert!(s.refund_client.get_fraud_score(&s.customer) > 0);
    assert!(!s.refund_client.is_fast_lane_customer(&s.customer));

    // A more lenient fraud limit lets the customer back in.
    s.refund_client.set_fast_lane_criteria(&s.admin, &3u32, &50u32);
    assert!(s.refund_client.is_fast_lane_customer(&s.customer));
}

#[test]
fn test_other_customers_payments_do_not_count() {
    let s = setup();
    enable(&s, 500);
    complete_payments(&s, 3);
    let stranger = Address::generate(&s.env);
    assert!(!s.refund_client.is_fast_lane_customer(&stranger));
}

// ===========================================================================
//  Request flow: when the fast lane does not apply
// ===========================================================================

#[test]
fn test_merchant_not_opted_in_goes_to_queue() {
    let s = setup();
    s.refund_client.set_fast_lane_criteria(&s.admin, &3u32, &0u32);
    let pid = complete_payments(&s, 3);
    let rid = request(&s, pid, 100);
    assert_eq!(status(&s, rid), RefundStatus::Requested);
}

#[test]
fn test_amount_above_fast_lane_max_goes_to_queue() {
    let s = setup();
    enable(&s, 150);
    let pid = complete_payments(&s, 3);
    let rid = request(&s, pid, 151);
    assert_eq!(status(&s, rid), RefundStatus::Requested);
}

#[test]
fn test_non_qualifying_customer_goes_to_queue() {
    let s = setup();
    enable(&s, 500);
    let pid = complete_payments(&s, 2);
    let rid = request(&s, pid, 100);
    assert_eq!(status(&s, rid), RefundStatus::Requested);
}

#[test]
fn test_opt_out_returns_requests_to_queue() {
    let s = setup();
    enable(&s, 500);
    let pid = complete_payments(&s, 3);
    s.refund_client.opt_out_fast_lane(&s.merchant);
    assert_eq!(s.refund_client.get_fast_lane_max_amount(&s.merchant), None);

    let rid = request(&s, pid, 100);
    assert_eq!(status(&s, rid), RefundStatus::Requested);
}

#[test]
fn test_reason_rule_takes_precedence_over_fast_lane() {
    let s = setup();
    enable(&s, 500);
    // 0 = "always review this reason".
    s.refund_client
        .set_reason_auto_approve(&s.merchant, &DEFECTIVE, &0i128);
    let pid = complete_payments(&s, 3);
    let rid = request(&s, pid, 100);
    assert_eq!(status(&s, rid), RefundStatus::Requested);
}

// ===========================================================================
//  Error branches of the configuration functions
// ===========================================================================

#[test]
#[should_panic(expected = "Only admin")]
fn test_set_criteria_non_admin_panics() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.refund_client.set_fast_lane_criteria(&stranger, &3u32, &0u32);
}

#[test]
#[should_panic(expected = "min_completed_payments must be positive")]
fn test_set_criteria_zero_payments_panics() {
    let s = setup();
    s.refund_client.set_fast_lane_criteria(&s.admin, &0u32, &0u32);
}

#[test]
#[should_panic(expected = "min_completed_payments exceeds maximum")]
fn test_set_criteria_above_maximum_panics() {
    let s = setup();
    s.refund_client.set_fast_lane_criteria(&s.admin, &101u32, &0u32);
}

#[test]
#[should_panic(expected = "Fast-lane max amount must be positive")]
fn test_opt_in_zero_amount_panics() {
    let s = setup();
    s.refund_client.opt_in_fast_lane(&s.merchant, &0i128);
}

#[test]
#[should_panic(expected = "Fast-lane max amount must be positive")]
fn test_opt_in_negative_amount_panics() {
    let s = setup();
    s.refund_client.opt_in_fast_lane(&s.merchant, &-5i128);
}

#[test]
#[should_panic(expected = "Merchant not opted in to fast lane")]
fn test_opt_out_when_not_opted_in_panics() {
    let s = setup();
    s.refund_client.opt_out_fast_lane(&s.merchant);
}

#[test]
fn test_opt_in_and_out_rejected_while_paused() {
    let s = setup();
    s.refund_client.opt_in_fast_lane(&s.merchant, &100i128);
    s.refund_client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));
    assert!(s.refund_client.try_opt_in_fast_lane(&s.merchant, &200i128).is_err());
    assert!(s.refund_client.try_opt_out_fast_lane(&s.merchant).is_err());
}
