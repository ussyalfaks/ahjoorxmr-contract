#![cfg(test)]
//! Merchant reserve lock while a refund appeal is pending: `appeal_refund`
//! locks the disputed amount, `withdraw_merchant_reserve` can't touch it, and
//! `resolve_appeal` either pays it out or releases it.
use super::*;
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use soroban_sdk::{Address, Env, String};

const APPEAL_WINDOW: u64 = 3_600;
const REFUND_AMOUNT: i128 = 250;

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
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
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

    Setup {
        env: env.clone(),
        refund_client,
        payment_client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
        customer: Address::generate(&env),
        merchant: Address::generate(&env),
    }
}

/// Configures `token` as the reserve token (seeded directly: `set_reserve_config`
/// double-authorises the admin and trips `mock_all_auths`).
fn set_reserve_token(s: &Setup, token: &Address) {
    s.env.as_contract(&s.refund_client.address, || {
        s.env
            .storage()
            .instance()
            .set(&crate::DataKey2::ReserveToken, token);
    });
}

/// Seeds the merchant's canonical reserve with `amount` of the payment token.
fn fund_reserve(s: &Setup, amount: i128) {
    set_reserve_token(s, &s.token_addr);
    s.token_admin_client.mint(&s.merchant, &amount);
    s.token_client.approve(
        &s.merchant,
        &s.refund_client.address,
        &amount,
        &(s.env.ledger().sequence() + 1_000),
    );
    s.refund_client.deposit_merchant_reserve(&s.merchant, &amount);
}

fn requested_refund(s: &Setup) -> u32 {
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
    let rid = requested_refund(s);
    s.refund_client
        .reject_refund(&s.admin, &rid, &String::from_str(&s.env, "No evidence"));
    rid
}

fn appealed_refund(s: &Setup) -> u32 {
    let rid = rejected_refund(s);
    s.refund_client.appeal_refund(&s.customer, &rid);
    rid
}

// ===========================================================================
//  Locking
// ===========================================================================

#[test]
fn test_appeal_locks_disputed_amount() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = appealed_refund(&s);

    assert_eq!(s.refund_client.get_refund(&rid).status, RefundStatus::UnderAppeal);
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), REFUND_AMOUNT);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), REFUND_AMOUNT);
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 750);
    // The funds stay in the reserve while locked.
    assert_eq!(
        s.refund_client.get_reserve_balance_summary(&s.merchant).merchant_reserve_balance,
        1_000
    );
}

#[test]
fn test_appeal_locks_only_available_reserve_when_short() {
    let s = setup();
    fund_reserve(&s, 100);
    let rid = appealed_refund(&s);

    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), 100);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 100);
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 0);
}

#[test]
fn test_concurrent_appeals_cannot_lock_the_same_funds_twice() {
    let s = setup();
    fund_reserve(&s, 400);
    let first = appealed_refund(&s);
    let second = appealed_refund(&s);

    assert_eq!(s.refund_client.get_appeal_reserve_lock(&first), 250);
    // Only 150 of the reserve was still free for the second appeal.
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&second), 150);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 400);
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 0);
}

#[test]
fn test_appeal_without_reserve_config_locks_nothing() {
    let s = setup();
    let rid = appealed_refund(&s);

    assert_eq!(s.refund_client.get_refund(&rid).status, RefundStatus::UnderAppeal);
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), 0);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

#[test]
fn test_appeal_with_empty_reserve_locks_nothing() {
    let s = setup();
    set_reserve_token(&s, &s.token_addr);
    let rid = appealed_refund(&s);

    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), 0);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

#[test]
fn test_appeal_locks_nothing_when_reserve_token_differs_from_refund_token() {
    let s = setup();
    let other = s.env.register_stellar_asset_contract_v2(s.admin.clone()).address();
    let other_client = TokenClient::new(&s.env, &other);
    TokenAdminClient::new(&s.env, &other).mint(&s.merchant, &1_000);
    set_reserve_token(&s, &other);
    other_client.approve(
        &s.merchant,
        &s.refund_client.address,
        &1_000,
        &(s.env.ledger().sequence() + 1_000),
    );
    s.refund_client.deposit_merchant_reserve(&s.merchant, &1_000);

    let rid = appealed_refund(&s);
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), 0);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

// ===========================================================================
//  Withdrawals
// ===========================================================================

#[test]
fn test_locked_funds_cannot_be_withdrawn() {
    let s = setup();
    fund_reserve(&s, 1_000);
    appealed_refund(&s);

    // 751 would dip into the 250 locked.
    assert!(s.refund_client.try_withdraw_merchant_reserve(&s.merchant, &751).is_err());
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 750);
}

#[test]
#[should_panic(expected = "ReserveFundsLockedForAppeal")]
fn test_withdraw_locked_funds_panics_with_lock_error() {
    let s = setup();
    fund_reserve(&s, 1_000);
    appealed_refund(&s);
    s.refund_client.withdraw_merchant_reserve(&s.merchant, &751);
}

#[test]
fn test_unlocked_portion_can_still_be_withdrawn() {
    let s = setup();
    fund_reserve(&s, 1_000);
    appealed_refund(&s);

    let before = s.token_client.balance(&s.merchant);
    s.refund_client.withdraw_merchant_reserve(&s.merchant, &750);
    assert_eq!(s.token_client.balance(&s.merchant), before + 750);
    // What remains is exactly the locked amount, and can't be touched.
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 0);
    assert!(s.refund_client.try_withdraw_merchant_reserve(&s.merchant, &1).is_err());
}

#[test]
#[should_panic(expected = "Insufficient reserve balance")]
fn test_withdraw_more_than_balance_still_reports_insufficient_balance() {
    let s = setup();
    fund_reserve(&s, 1_000);
    appealed_refund(&s);
    s.refund_client.withdraw_merchant_reserve(&s.merchant, &1_001);
}

// ===========================================================================
//  Resolution
// ===========================================================================

#[test]
fn test_appeal_approved_pays_locked_funds_to_customer() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = appealed_refund(&s);

    let customer_before = s.token_client.balance(&s.customer);
    s.refund_client.resolve_appeal(&s.admin, &rid, &true);

    assert_eq!(s.refund_client.get_refund(&rid).status, RefundStatus::Processed);
    // Escrowed refund amount returned plus the locked reserve payout.
    assert_eq!(
        s.token_client.balance(&s.customer),
        customer_before + REFUND_AMOUNT + REFUND_AMOUNT
    );
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), 0);
    assert_eq!(
        s.refund_client.get_reserve_balance_summary(&s.merchant).merchant_reserve_balance,
        750
    );
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 750);
}

#[test]
fn test_appeal_approved_with_partial_lock_pays_only_locked_amount() {
    let s = setup();
    fund_reserve(&s, 100);
    let rid = appealed_refund(&s);

    let customer_before = s.token_client.balance(&s.customer);
    s.refund_client.resolve_appeal(&s.admin, &rid, &true);

    assert_eq!(
        s.token_client.balance(&s.customer),
        customer_before + REFUND_AMOUNT + 100
    );
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
    assert_eq!(
        s.refund_client.get_reserve_balance_summary(&s.merchant).merchant_reserve_balance,
        0
    );
}

#[test]
fn test_appeal_rejected_releases_lock() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = appealed_refund(&s);

    s.refund_client.resolve_appeal(&s.admin, &rid, &false);

    assert_eq!(s.refund_client.get_refund(&rid).status, RefundStatus::Rejected);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), 0);
    // Nothing left the reserve, and all of it is withdrawable again.
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 1_000);
    let before = s.token_client.balance(&s.merchant);
    s.refund_client.withdraw_merchant_reserve(&s.merchant, &1_000);
    assert_eq!(s.token_client.balance(&s.merchant), before + 1_000);
}

#[test]
fn test_resolving_one_appeal_keeps_other_locks() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let first = appealed_refund(&s);
    let second = appealed_refund(&s);
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 500);

    s.refund_client.resolve_appeal(&s.admin, &first, &false);

    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 250);
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&second), 250);
    assert_eq!(s.refund_client.get_available_reserve(&s.merchant), 750);
}

#[test]
fn test_resolve_appeal_without_lock_is_unchanged() {
    let s = setup();
    let rid = appealed_refund(&s);

    let before = s.token_client.balance(&s.customer);
    s.refund_client.resolve_appeal(&s.admin, &rid, &true);
    // No reserve involved: only the escrowed amount comes back.
    assert_eq!(s.token_client.balance(&s.customer), before + REFUND_AMOUNT);
}

// ===========================================================================
//  Error branches of appeal_refund / resolve_appeal leave no lock behind
// ===========================================================================

#[test]
fn test_appeal_on_requested_refund_locks_nothing() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = requested_refund(&s);

    assert!(s.refund_client.try_appeal_refund(&s.customer, &rid).is_err());
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

#[test]
fn test_appeal_by_non_customer_locks_nothing() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = rejected_refund(&s);
    let stranger = Address::generate(&s.env);

    assert!(s.refund_client.try_appeal_refund(&stranger, &rid).is_err());
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

#[test]
fn test_appeal_after_window_locks_nothing() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = rejected_refund(&s);
    s.env.ledger().with_mut(|l| l.timestamp += APPEAL_WINDOW + 1);

    assert!(s.refund_client.try_appeal_refund(&s.customer, &rid).is_err());
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

#[test]
fn test_second_appeal_does_not_lock_twice() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = appealed_refund(&s);

    assert!(s.refund_client.try_appeal_refund(&s.customer, &rid).is_err());
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), REFUND_AMOUNT);
}

#[test]
fn test_appeal_while_paused_locks_nothing() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = rejected_refund(&s);
    s.refund_client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));

    assert!(s.refund_client.try_appeal_refund(&s.customer, &rid).is_err());
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

#[test]
fn test_resolve_without_appeal_keeps_lock_state() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = rejected_refund(&s);

    assert!(s.refund_client.try_resolve_appeal(&s.admin, &rid, &true).is_err());
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), 0);
}

#[test]
fn test_resolve_appeal_by_non_admin_keeps_lock() {
    let s = setup();
    fund_reserve(&s, 1_000);
    let rid = appealed_refund(&s);
    let stranger = Address::generate(&s.env);

    assert!(s.refund_client.try_resolve_appeal(&stranger, &rid, &true).is_err());
    assert_eq!(s.refund_client.get_locked_reserve(&s.merchant), REFUND_AMOUNT);
    assert_eq!(s.refund_client.get_appeal_reserve_lock(&rid), REFUND_AMOUNT);
}
