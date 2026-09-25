#![cfg(test)]
//! #941: Refund appeal process — `appeal_refund`, `resolve_appeal` and
//! `request_review_extension`.
use super::*;
use soroban_sdk::testutils::{Address as _, BytesN as _, Ledger};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use soroban_sdk::{Address, BytesN, Env, String};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

const APPEAL_WINDOW: u64 = 3_600;
const REFUND_AMOUNT: i128 = 250;
/// Extension applied by `request_review_extension` when none is configured (~7 days).
const DEFAULT_REVIEW_EXTENSION_LEDGERS: u32 = 120_960;

struct Setup<'a> {
    env: Env,
    refund_client: AhjoorRefundContractClient<'a>,
    payment_client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
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
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(
        &admin,
        &payment_id,
        &86_400u64,
        &Some(RefundInitConfig {
            escrow_contract: None,
            refund_fee_bps: 0,
            fee_recipient: None,
            auto_reject_window_seconds: 0,
            appeal_window_seconds: APPEAL_WINDOW,
            refund_tiers: None,
            refund_cooldown_seconds: 0,
            customer_cancel_window_seconds: 0,
        }),
    );

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    Setup {
        env,
        refund_client,
        payment_client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
        customer,
        merchant,
    }
}

fn request_refund(s: &Setup) -> u32 {
    s.token_admin_client.mint(&s.customer, &1_000);
    let pid = s.payment_client.create_payment(
        &s.customer,
        &s.merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.payment_client.complete_payment(&pid);
    s.refund_client.request_refund(
        &s.customer,
        &pid,
        &REFUND_AMOUNT,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    )
}

fn rejected_refund(s: &Setup) -> u32 {
    let refund_id = request_refund(s);
    s.refund_client.reject_refund(
        &s.admin,
        &refund_id,
        &String::from_str(&s.env, "Insufficient evidence"),
    );
    refund_id
}

// ---------------------------------------------------------------------------
// appeal_refund
// ---------------------------------------------------------------------------

#[test]
fn test_customer_can_appeal_rejected_refund() {
    let s = setup();
    let refund_id = rejected_refund(&s);

    s.refund_client.appeal_refund(&s.customer, &refund_id);

    assert_eq!(
        s.refund_client.get_refund(&refund_id).status,
        RefundStatus::UnderAppeal
    );
}

#[test]
#[should_panic(expected = "Appeal only allowed from Rejected status")]
fn test_appeal_on_requested_refund_panics() {
    let s = setup();
    let refund_id = request_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);
}

#[test]
#[should_panic(expected = "Appeal only allowed from Rejected status")]
fn test_appeal_on_approved_refund_panics() {
    let s = setup();
    let refund_id = request_refund(&s);
    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.appeal_refund(&s.customer, &refund_id);
}

#[test]
#[should_panic(expected = "Appeal only allowed from Rejected status")]
fn test_appeal_twice_panics() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);
    s.refund_client.appeal_refund(&s.customer, &refund_id);
}

#[test]
#[should_panic(expected = "Only the original customer can appeal")]
fn test_non_customer_cannot_appeal() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    let stranger = Address::generate(&s.env);
    s.refund_client.appeal_refund(&stranger, &refund_id);
}

#[test]
#[should_panic(expected = "Appeal window has expired")]
fn test_appeal_after_window_panics() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    let rejected_at = s.refund_client.get_refund(&refund_id).rejected_at.unwrap();
    s.env
        .ledger()
        .with_mut(|l| l.timestamp = rejected_at + APPEAL_WINDOW + 1);
    s.refund_client.appeal_refund(&s.customer, &refund_id);
}

// ---------------------------------------------------------------------------
// resolve_appeal
// ---------------------------------------------------------------------------

#[test]
fn test_resolve_appeal_overturn_processes_refund() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);

    let before = s.token_client.balance(&s.customer);
    s.refund_client.resolve_appeal(&s.admin, &refund_id, &true);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert!(refund.processed_at.is_some());
    assert_eq!(s.token_client.balance(&s.customer), before + REFUND_AMOUNT);
}

#[test]
fn test_resolve_appeal_uphold_keeps_rejection() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);

    s.refund_client.resolve_appeal(&s.admin, &refund_id, &false);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Rejected);
    assert!(refund.processed_at.is_none());
}

#[test]
#[should_panic(expected = "Refund is not under appeal")]
fn test_resolve_appeal_without_appeal_panics() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.resolve_appeal(&s.admin, &refund_id, &true);
}

#[test]
fn test_resolve_appeal_by_non_admin_fails() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);

    let stranger = Address::generate(&s.env);
    assert!(s
        .refund_client
        .try_resolve_appeal(&stranger, &refund_id, &true)
        .is_err());
    assert_eq!(
        s.refund_client.get_refund(&refund_id).status,
        RefundStatus::UnderAppeal
    );
}

// ---------------------------------------------------------------------------
// request_review_extension
// ---------------------------------------------------------------------------

#[test]
fn test_review_extension_extends_deadline_for_appeal_in_progress() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);

    let before = s.refund_client.get_refund(&refund_id);
    assert!(!before.extension_requested);

    s.refund_client.request_review_extension(
        &s.merchant,
        &refund_id,
        &BytesN::<32>::random(&s.env),
    );

    let after = s.refund_client.get_refund(&refund_id);
    assert_eq!(
        after.auto_approval_deadline_ledger,
        before.auto_approval_deadline_ledger + DEFAULT_REVIEW_EXTENSION_LEDGERS
    );
    assert!(after.extension_requested);
    assert_eq!(after.status, RefundStatus::UnderAppeal);

    // The appeal can still be resolved after the extension
    s.refund_client.resolve_appeal(&s.admin, &refund_id, &true);
    assert_eq!(
        s.refund_client.get_refund(&refund_id).status,
        RefundStatus::Processed
    );
}

#[test]
#[should_panic(expected = "ExtensionAlreadyUsed")]
fn test_review_extension_only_once() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);

    let reason = BytesN::<32>::random(&s.env);
    s.refund_client
        .request_review_extension(&s.merchant, &refund_id, &reason);
    s.refund_client
        .request_review_extension(&s.merchant, &refund_id, &reason);
}

#[test]
#[should_panic(expected = "OnlyMerchantCanRequestExtension")]
fn test_review_extension_by_non_merchant_panics() {
    let s = setup();
    let refund_id = rejected_refund(&s);
    s.refund_client.appeal_refund(&s.customer, &refund_id);

    let stranger = Address::generate(&s.env);
    s.refund_client.request_review_extension(
        &stranger,
        &refund_id,
        &BytesN::<32>::random(&s.env),
    );
}
