#![cfg(test)]
//! #1172: per-reason-code auto-approve rules
//! (`set_reason_auto_approve`, `remove_reason_auto_approve`,
//! `get_reason_auto_approve`) and their interaction with the global
//! `AutoApproveThreshold` and fraud score cap.
use super::*;
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

const PAYMENT_AMOUNT: i128 = 1_000;

/// Reason codes (see `request_refund`).
const DEFECTIVE: u32 = 0;
const NOT_DELIVERED: u32 = 1;

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

fn request(s: &Setup, amount: i128, reason_code: u32) -> u32 {
    let pid = make_payment(s);
    s.refund_client.request_refund(
        &s.customer,
        &pid,
        &amount,
        &String::from_str(&s.env, "reason"),
        &reason_code,
    )
}

fn status(s: &Setup, refund_id: u32) -> RefundStatus {
    s.refund_client.get_refund(&refund_id).status
}

// ===========================================================================
//  set / get / remove
// ===========================================================================

#[test]
fn test_set_and_get_reason_auto_approve() {
    let s = setup();
    assert_eq!(s.refund_client.get_reason_auto_approve(&s.merchant, &NOT_DELIVERED), None);

    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);
    assert_eq!(
        s.refund_client.get_reason_auto_approve(&s.merchant, &NOT_DELIVERED),
        Some(20)
    );
    // Rules are per reason code and per merchant.
    assert_eq!(s.refund_client.get_reason_auto_approve(&s.merchant, &DEFECTIVE), None);
    let other = Address::generate(&s.env);
    assert_eq!(s.refund_client.get_reason_auto_approve(&other, &NOT_DELIVERED), None);

    // Overwriting updates the ceiling.
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &50);
    assert_eq!(
        s.refund_client.get_reason_auto_approve(&s.merchant, &NOT_DELIVERED),
        Some(50)
    );
}

#[test]
fn test_set_reason_auto_approve_requires_merchant_auth() {
    let s = setup();
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, s.merchant);
}

#[test]
#[should_panic(expected = "Invalid reason code: must be 0-4")]
fn test_set_reason_auto_approve_rejects_invalid_reason_code() {
    let s = setup();
    s.refund_client.set_reason_auto_approve(&s.merchant, &5, &20);
}

#[test]
#[should_panic(expected = "Reason auto-approve amount cannot be negative")]
fn test_set_reason_auto_approve_rejects_negative_amount() {
    let s = setup();
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &-1);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_set_reason_auto_approve_rejects_when_paused() {
    let s = setup();
    s.refund_client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);
}

#[test]
fn test_remove_reason_auto_approve() {
    let s = setup();
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);
    s.refund_client.remove_reason_auto_approve(&s.merchant, &NOT_DELIVERED);
    assert_eq!(s.refund_client.get_reason_auto_approve(&s.merchant, &NOT_DELIVERED), None);
}

#[test]
#[should_panic(expected = "Reason auto-approve rule not found")]
fn test_remove_missing_reason_auto_approve_panics() {
    let s = setup();
    s.refund_client.remove_reason_auto_approve(&s.merchant, &NOT_DELIVERED);
}

// ===========================================================================
//  Evaluation during request_refund
// ===========================================================================

#[test]
fn test_reason_rule_auto_approves_under_max_amount() {
    let s = setup();
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);

    let rid = request(&s, 20, NOT_DELIVERED);
    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Approved);
    assert_eq!(
        refund.auto_approved_source,
        Some(String::from_str(&s.env, "reason_rule"))
    );
    assert!(refund.approved_at.is_some());
}

#[test]
fn test_reason_rule_over_max_amount_requires_review() {
    let s = setup();
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);

    let rid = request(&s, 21, NOT_DELIVERED);
    assert_eq!(status(&s, rid), RefundStatus::Requested);
    assert_eq!(s.refund_client.get_refund(&rid).auto_approved_source, None);
}

#[test]
fn test_reason_rule_is_checked_before_global_threshold() {
    let s = setup();
    // Global threshold would approve anything up to 500...
    s.refund_client.set_auto_approve_threshold(&s.merchant, &500);
    // ...but NotDelivered is capped at 20 and Defective is always reviewed.
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);
    s.refund_client.set_reason_auto_approve(&s.merchant, &DEFECTIVE, &0);

    let nd_small = request(&s, 15, NOT_DELIVERED);
    assert_eq!(status(&s, nd_small), RefundStatus::Approved);
    assert_eq!(
        s.refund_client.get_refund(&nd_small).auto_approved_source,
        Some(String::from_str(&s.env, "reason_rule"))
    );

    // Over the rule's ceiling: the global threshold must not rescue it.
    let nd_large = request(&s, 100, NOT_DELIVERED);
    assert_eq!(status(&s, nd_large), RefundStatus::Requested);

    // max_amount == 0 means "always review", regardless of the global threshold.
    let defective = request(&s, 10, DEFECTIVE);
    assert_eq!(status(&s, defective), RefundStatus::Requested);
}

#[test]
fn test_reason_without_rule_falls_back_to_global_threshold() {
    let s = setup();
    s.refund_client.set_auto_approve_threshold(&s.merchant, &500);
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);

    // Reason code 2 (DuplicateCharge) has no rule → global threshold applies.
    let rid = request(&s, 100, 2);
    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Approved);
    assert_eq!(
        refund.auto_approved_source,
        Some(String::from_str(&s.env, "threshold"))
    );
}

#[test]
fn test_reason_rule_can_approve_above_global_threshold() {
    let s = setup();
    s.refund_client.set_auto_approve_threshold(&s.merchant, &10);
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &200);

    let rid = request(&s, 150, NOT_DELIVERED);
    assert_eq!(status(&s, rid), RefundStatus::Approved);
}

#[test]
fn test_removed_reason_rule_falls_back_to_global_threshold() {
    let s = setup();
    s.refund_client.set_auto_approve_threshold(&s.merchant, &500);
    s.refund_client.set_reason_auto_approve(&s.merchant, &DEFECTIVE, &0);
    s.refund_client.remove_reason_auto_approve(&s.merchant, &DEFECTIVE);

    let rid = request(&s, 100, DEFECTIVE);
    assert_eq!(
        s.refund_client.get_refund(&rid).auto_approved_source,
        Some(String::from_str(&s.env, "threshold"))
    );
}

#[test]
fn test_reason_rule_respects_fraud_score_cap() {
    let s = setup();
    // Raise the customer's fraud score to 1 via an admin rejection
    // (no rules configured yet, so this refund goes to review).
    let rid = request(&s, 10, 4);
    s.refund_client
        .reject_refund(&s.admin, &rid, &String::from_str(&s.env, "no"));
    assert_eq!(s.refund_client.get_fraud_score(&s.customer), 1);

    s.refund_client.set_auto_approve_fraud_score_cap(&s.admin, &1);
    s.refund_client.set_reason_auto_approve(&s.merchant, &NOT_DELIVERED, &20);

    // Within the rule's ceiling but the buyer is at the cap → review.
    let rid = request(&s, 10, NOT_DELIVERED);
    assert_eq!(status(&s, rid), RefundStatus::Requested);
    assert_eq!(s.refund_client.get_refund(&rid).auto_approved_source, None);
}

#[test]
fn test_whitelisted_merchant_bypasses_reason_rule() {
    let s = setup();
    s.refund_client.add_to_auto_approve(&s.admin, &s.merchant);
    s.refund_client.set_reason_auto_approve(&s.merchant, &DEFECTIVE, &0);

    let rid = request(&s, 100, DEFECTIVE);
    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Approved);
    assert_eq!(
        refund.auto_approved_source,
        Some(String::from_str(&s.env, "whitelist"))
    );
}
