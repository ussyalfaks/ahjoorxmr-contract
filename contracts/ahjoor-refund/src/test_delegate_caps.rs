#![cfg(test)]
//! #1174: Per-delegate approval caps.
use super::*;
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{StellarAssetClient as TokenAdminClient, TokenClient};
use soroban_sdk::{Address, Env, String};

pub(crate) const APPEAL_WINDOW: u64 = 86_400;

pub(crate) struct Setup<'a> {
    pub env: Env,
    pub admin: Address,
    pub refund_client: AhjoorRefundContractClient<'a>,
    pub payment_client: AhjoorPaymentsContractClient<'a>,
    pub token_addr: Address,
    pub token_client: TokenClient<'a>,
    pub token_admin_client: TokenAdminClient<'a>,
    pub customer: Address,
    pub merchant: Address,
}

pub(crate) fn setup<'a>() -> Setup<'a> {
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
        admin,
        refund_client,
        payment_client,
        token_addr,
        token_client,
        token_admin_client,
        customer,
        merchant,
    }
}

/// Creates a completed 1_000 payment and requests a refund of `amount`.
pub(crate) fn request_refund(s: &Setup, amount: i128) -> u32 {
    s.token_admin_client.mint(&s.customer, &(1_000 + amount));
    let pid = s.payment_client.create_payment(
        &s.customer,
        &s.merchant,
        &1_000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.payment_client.complete_payment(&pid);
    s.refund_client.request_refund(
        &s.customer,
        &pid,
        &amount,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    )
}

#[test]
fn test_capped_delegate_approves_within_cap() {
    let s = setup();
    let d = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &d, &Some(300));
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&d, &id);
    assert_eq!(s.refund_client.get_refund(&id).status, RefundStatus::Approved);
}

#[test]
#[should_panic(expected = "Refund amount exceeds delegate approval cap")]
fn test_capped_delegate_cannot_approve_above_cap() {
    let s = setup();
    let d = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &d, &Some(100));
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&d, &id);
}

#[test]
fn test_higher_cap_delegate_can_approve_large_refund() {
    let s = setup();
    let low = Address::generate(&s.env);
    let high = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &low, &Some(100));
    s.refund_client.add_delegate(&s.merchant, &high, &Some(500));
    let id = request_refund(&s, 300);
    assert!(s.refund_client.try_approve_refund(&low, &id).is_err());
    s.refund_client.approve_refund(&high, &id);
    assert_eq!(s.refund_client.get_refund(&id).status, RefundStatus::Approved);
}

#[test]
fn test_merchant_can_approve_above_delegate_cap() {
    let s = setup();
    let d = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &d, &Some(100));
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.merchant, &id);
    assert_eq!(s.refund_client.get_refund(&id).status, RefundStatus::Approved);
}

#[test]
fn test_uncapped_delegate_approves_any_amount() {
    let s = setup();
    let d = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &d, &None);
    let id = request_refund(&s, 900);
    s.refund_client.approve_refund(&d, &id);
    assert_eq!(s.refund_client.get_refund(&id).status, RefundStatus::Approved);
}

#[test]
#[should_panic(expected = "Delegate approval cap must be positive")]
fn test_add_delegate_zero_cap_panics() {
    let s = setup();
    let d = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &d, &Some(0));
}

#[test]
#[should_panic(expected = "Delegate approval cap must be positive")]
fn test_add_delegate_negative_cap_panics() {
    let s = setup();
    let d = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &d, &Some(-5));
}

#[test]
#[should_panic(expected = "Refund amount exceeds delegate approval cap")]
fn test_voucher_approval_respects_cap() {
    let s = setup();
    let d = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &d, &Some(100));
    let id = request_refund(&s, 300);
    s.refund_client
        .approve_refund_as_voucher(&d, &id, &300i128, &0u64);
}

#[test]
fn test_get_delegate_limits() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    s.refund_client.add_delegate(&s.merchant, &a, &Some(250));
    s.refund_client.add_delegate(&s.merchant, &b, &None);

    let limits = s.refund_client.get_delegate_limits(&s.merchant);
    assert_eq!(limits.len(), 2);
    assert_eq!(
        limits.get(0).unwrap(),
        DelegateLimit {
            delegate: a.clone(),
            max_approve_amount: Some(250)
        }
    );
    assert_eq!(
        limits.get(1).unwrap(),
        DelegateLimit {
            delegate: b,
            max_approve_amount: None
        }
    );

    // Removing a delegate clears its cap; re-adding uncapped stays uncapped.
    s.refund_client.remove_delegate(&s.merchant, &a);
    s.refund_client.add_delegate(&s.merchant, &a, &None);
    let limits = s.refund_client.get_delegate_limits(&s.merchant);
    assert_eq!(limits.get(1).unwrap().max_approve_amount, None);
}

#[test]
fn test_get_delegate_limits_empty() {
    let s = setup();
    assert_eq!(s.refund_client.get_delegate_limits(&s.merchant).len(), 0);
}
