#![cfg(test)]
//! Coverage for fraud scoring and auto-approval thresholds
//! (`get_fraud_score`, `reset_fraud_score`, `set_fraud_score_block_threshold`,
//! `set_auto_approve_threshold`, `set_auto_approve_fraud_score_cap`,
//! `trigger_auto_approval`).
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

const PAYMENT_AMOUNT: i128 = 1_000;
const REFUND_AMOUNT: i128 = 100;

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

/// Creates and completes a payment from the customer to the merchant.
fn make_payment(s: &Setup) -> u32 {
    s.token_admin.mint(&s.customer, &(PAYMENT_AMOUNT * 2));
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

fn request(s: &Setup, amount: i128) -> u32 {
    let pid = make_payment(s);
    s.refund_client.request_refund(
        &s.customer,
        &pid,
        &amount,
        &String::from_str(&s.env, "reason"),
        &0,
    )
}

/// Raises the customer's fraud score by `n` via admin rejections.
fn raise_fraud_score(s: &Setup, n: u32) {
    for _ in 0..n {
        let rid = request(s, REFUND_AMOUNT);
        s.refund_client
            .reject_refund(&s.admin, &rid, &String::from_str(&s.env, "no"));
    }
}

// ===========================================================================
//  get_fraud_score / reset_fraud_score
// ===========================================================================

#[test]
fn test_fraud_score_defaults_to_zero_and_increments_on_rejection() {
    let s = setup();
    assert_eq!(s.refund_client.get_fraud_score(&s.customer), 0);

    raise_fraud_score(&s, 2);
    assert_eq!(s.refund_client.get_fraud_score(&s.customer), 2);
}

#[test]
fn test_reset_fraud_score_clears_score() {
    let s = setup();
    raise_fraud_score(&s, 3);
    assert_eq!(s.refund_client.get_fraud_score(&s.customer), 3);

    s.refund_client.reset_fraud_score(&s.admin, &s.customer);
    assert_eq!(s.refund_client.get_fraud_score(&s.customer), 0);

    // Resetting an already-zero score is a no-op.
    s.refund_client.reset_fraud_score(&s.admin, &s.customer);
    assert_eq!(s.refund_client.get_fraud_score(&s.customer), 0);
}

#[test]
#[should_panic(expected = "Only admin can manage pause state")]
fn test_reset_fraud_score_rejects_non_admin() {
    let s = setup();
    raise_fraud_score(&s, 1);
    s.refund_client.reset_fraud_score(&s.merchant, &s.customer);
}

// ===========================================================================
//  set_fraud_score_block_threshold
// ===========================================================================

#[test]
fn test_set_fraud_score_block_threshold() {
    let s = setup();
    assert_eq!(s.refund_client.get_fraud_score_block_threshold(), 10);

    s.refund_client.set_fraud_score_block_threshold(&s.admin, &2);
    assert_eq!(s.refund_client.get_fraud_score_block_threshold(), 2);
}

#[test]
#[should_panic(expected = "Threshold must be positive")]
fn test_set_fraud_score_block_threshold_rejects_zero() {
    let s = setup();
    s.refund_client.set_fraud_score_block_threshold(&s.admin, &0);
}

#[test]
#[should_panic(expected = "Only admin can manage pause state")]
fn test_set_fraud_score_block_threshold_rejects_non_admin() {
    let s = setup();
    s.refund_client
        .set_fraud_score_block_threshold(&s.customer, &2);
}

#[test]
#[should_panic(expected = "BuyerBlockedForFraud")]
fn test_request_at_block_threshold_is_rejected() {
    let s = setup();
    s.refund_client.set_fraud_score_block_threshold(&s.admin, &2);
    raise_fraud_score(&s, 2);

    request(&s, REFUND_AMOUNT);
}

#[test]
fn test_request_below_block_threshold_is_accepted() {
    let s = setup();
    s.refund_client.set_fraud_score_block_threshold(&s.admin, &2);
    raise_fraud_score(&s, 1);

    let rid = request(&s, REFUND_AMOUNT);
    assert_eq!(
        s.refund_client.get_refund(&rid).status,
        RefundStatus::Requested
    );
}

#[test]
fn test_reset_unblocks_buyer() {
    let s = setup();
    s.refund_client.set_fraud_score_block_threshold(&s.admin, &2);
    raise_fraud_score(&s, 2);

    let pid = make_payment(&s);
    let blocked = s.refund_client.try_request_refund(
        &s.customer,
        &pid,
        &REFUND_AMOUNT,
        &String::from_str(&s.env, "reason"),
        &0,
    );
    assert!(blocked.is_err());

    s.refund_client.reset_fraud_score(&s.admin, &s.customer);

    let rid = s.refund_client.request_refund(
        &s.customer,
        &pid,
        &REFUND_AMOUNT,
        &String::from_str(&s.env, "reason"),
        &0,
    );
    assert_eq!(
        s.refund_client.get_refund(&rid).status,
        RefundStatus::Requested
    );
}

// ===========================================================================
//  set_auto_approve_threshold / set_auto_approve_fraud_score_cap
// ===========================================================================

#[test]
fn test_set_auto_approve_threshold_requires_merchant_auth() {
    let s = setup();
    assert_eq!(s.refund_client.get_auto_approve_threshold(&s.merchant), 0);

    s.refund_client
        .set_auto_approve_threshold(&s.merchant, &REFUND_AMOUNT);
    assert_eq!(s.env.auths()[0].0, s.merchant);
    assert_eq!(
        s.refund_client.get_auto_approve_threshold(&s.merchant),
        REFUND_AMOUNT
    );
}

#[test]
#[should_panic(expected = "Threshold amount cannot be negative")]
fn test_set_auto_approve_threshold_rejects_negative() {
    let s = setup();
    s.refund_client.set_auto_approve_threshold(&s.merchant, &-1);
}

#[test]
fn test_set_auto_approve_fraud_score_cap() {
    let s = setup();
    assert_eq!(s.refund_client.get_auto_approve_fraud_score_cap(), u32::MAX);

    s.refund_client.set_auto_approve_fraud_score_cap(&s.admin, &1);
    assert_eq!(s.refund_client.get_auto_approve_fraud_score_cap(), 1);
}

#[test]
#[should_panic(expected = "Only admin can manage pause state")]
fn test_set_auto_approve_fraud_score_cap_rejects_non_admin() {
    let s = setup();
    s.refund_client
        .set_auto_approve_fraud_score_cap(&s.merchant, &1);
}

#[test]
fn test_request_under_threshold_is_auto_approved() {
    let s = setup();
    s.refund_client
        .set_auto_approve_threshold(&s.merchant, &REFUND_AMOUNT);

    let rid = request(&s, REFUND_AMOUNT);
    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Approved);
    assert_eq!(
        refund.auto_approved_source,
        Some(String::from_str(&s.env, "threshold"))
    );
}

#[test]
fn test_request_over_threshold_is_not_auto_approved() {
    let s = setup();
    s.refund_client
        .set_auto_approve_threshold(&s.merchant, &REFUND_AMOUNT);

    let rid = request(&s, REFUND_AMOUNT + 1);
    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Requested);
    assert_eq!(refund.auto_approved_source, None);
}

#[test]
fn test_customer_at_fraud_score_cap_is_not_auto_approved() {
    let s = setup();
    // Raise the score first: with auto-approval on, the refunds it requests
    // would be auto-approved and could no longer be rejected.
    raise_fraud_score(&s, 1);
    s.refund_client
        .set_auto_approve_threshold(&s.merchant, &REFUND_AMOUNT);
    s.refund_client.set_auto_approve_fraud_score_cap(&s.admin, &1);

    let rid = request(&s, REFUND_AMOUNT);
    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Requested);
    assert_eq!(refund.auto_approved_source, None);

    // Once the score is reset the customer qualifies again.
    s.refund_client.reset_fraud_score(&s.admin, &s.customer);
    let rid = request(&s, REFUND_AMOUNT);
    assert_eq!(
        s.refund_client.get_refund(&rid).status,
        RefundStatus::Approved
    );
}

// ===========================================================================
//  trigger_auto_approval
// ===========================================================================

#[test]
fn test_trigger_auto_approval_after_merchant_deadline() {
    let s = setup();
    s.refund_client
        .set_merchant_response_deadline(&s.admin, &100, &0);

    let rid = request(&s, REFUND_AMOUNT);
    assert_eq!(
        s.refund_client.get_refund(&rid).status,
        RefundStatus::Requested
    );

    let balance_before = s.token.balance(&s.customer);
    let deadline = s.refund_client.get_refund(&rid).auto_approval_deadline_ledger;
    s.env.ledger().set_sequence_number(deadline);
    s.refund_client.trigger_auto_approval(&rid);

    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert!(refund.approved_at.is_some());
    assert_eq!(s.token.balance(&s.customer), balance_before + REFUND_AMOUNT);
}

#[test]
#[should_panic(expected = "MerchantDeadlineNotPassed")]
fn test_trigger_auto_approval_before_deadline_panics() {
    let s = setup();
    s.refund_client
        .set_merchant_response_deadline(&s.admin, &100, &0);

    let rid = request(&s, REFUND_AMOUNT);
    s.refund_client.trigger_auto_approval(&rid);
}

#[test]
#[should_panic(expected = "RefundNotInRequestedState")]
fn test_trigger_auto_approval_rejects_already_approved() {
    let s = setup();
    s.refund_client
        .set_auto_approve_threshold(&s.merchant, &REFUND_AMOUNT);

    let rid = request(&s, REFUND_AMOUNT);
    s.refund_client.trigger_auto_approval(&rid);
}
