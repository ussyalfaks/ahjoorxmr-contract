#![cfg(test)]
//! Boundary-ledger tests for #553: permissionless-after-timeout functions must
//! treat the exact-boundary ledger (now == deadline) consistently. This
//! contract's convention (matching escrow's auto_release_expired /
//! expire_cancellation / expire_seller_transfer_veto and rosca's
//! close_round / finalize_round) is an *exclusive* boundary: the window has
//! elapsed only once `now` is strictly greater than the deadline.
//!
//! ## #553 audit table
//!
//! Every permissionless-after-timeout deadline/expiry comparison named in
//! #553, and the operator each used *before* this fix. "Elapsed at
//! boundary?" answers: is `now == deadline` already considered
//! expired/elapsed? All functions are now aligned on "No" (exclusive
//! boundary — `now` must be *strictly greater than* the deadline).
//!
//! | Contract | Function | Field compared | Operator (before) | Elapsed at boundary? (before) | Elapsed at boundary? (after) |
//! | -------- | -------- | --------------- | ------------------ | ------------------------------ | ------------------------------ |
//! | escrow | `auto_release_expired` | `escrow.deadline` | `now <= deadline` blocks | No | No (unchanged) |
//! | escrow | `expire_cancellation` | `request.expires_at` | `now <= expires_at` blocks | No | No (unchanged) |
//! | escrow | `expire_seller_transfer_veto` | `proposal.veto_deadline` (ledger seq) | `seq <= veto_deadline` blocks | No | No (unchanged) |
//! | rosca | `close_round` | round deadline | `now <= deadline` blocks | No | No (unchanged) |
//! | rosca | `finalize_round` | round deadline | `now <= deadline` blocks | No | No (unchanged) |
//! | refund | `auto_approve_refund` | `requested_at + dispute_window` | `now < threshold` blocks | **Yes** | No (fixed: now `now <= threshold` blocks) |
//! | refund | `auto_cancel_expired_request` | `requested_at + cancel_window` | `now < threshold` blocks | **Yes** | No (fixed: now `now <= threshold` blocks) |
//! | refund | `auto_reject_stale_refund` | `requested_at + auto_reject_window + extension` | `now < deadline` blocks | **Yes** | No (fixed: now `now <= deadline` blocks) |
//! | refund | `settle_expired_counter_offer` | `offer.expiry` | `now <= expiry` blocks | No | No (unchanged) |
//!
//! Three of the four named refund functions treated the exact-boundary
//! ledger as already elapsed (inclusive boundary), while every escrow and
//! rosca function — and refund's own `settle_expired_counter_offer` —
//! treated it as not yet elapsed (exclusive boundary). No code comment
//! documented this as an intentional difference, so `auto_approve_refund`,
//! `auto_cancel_expired_request`, and `auto_reject_stale_refund` were
//! aligned to the exclusive-boundary convention used everywhere else.
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

struct Setup<'a> {
    env: Env,
    refund_client: AhjoorRefundContractClient<'a>,
    payment_client: AhjoorPaymentsContractClient<'a>,
    customer: Address,
    merchant: Address,
    token_addr: Address,
}

fn setup_with_windows<'a>(
    customer_cancel_window_seconds: u64,
    auto_reject_window_seconds: u64,
) -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(
        &admin,
        &payment_id,
        &86_400u64,
        &Some(RefundInitConfig {
            escrow_contract: None,
            refund_fee_bps: 0,
            fee_recipient: None,
            auto_reject_window_seconds,
            appeal_window_seconds: 0,
            refund_tiers: None,
            refund_cooldown_seconds: 0,
            customer_cancel_window_seconds,
        }),
    );

    token_admin_client.mint(&customer, &2000);
    token_admin_client.mint(&refund_id, &2000);

    Setup {
        env,
        refund_client,
        payment_client,
        customer,
        merchant,
        token_addr,
    }
}

fn request_refund_for(s: &Setup, amount: i128) -> u32 {
    let pid = s.payment_client.create_payment(
        &s.customer,
        &s.merchant,
        &amount,
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
        &String::from_str(&s.env, "not as described"),
        &0u32,
    )
}

// ---------------------------------------------------------------------------
// auto_cancel_expired_request
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Customer cancel window has not elapsed")]
fn test_auto_cancel_exactly_at_window_boundary_panics() {
    let s = setup_with_windows(1_000, 0);
    s.env.ledger().with_mut(|l| l.timestamp = 0);
    let refund_id = request_refund_for(&s, 100);

    // requested_at(0) + cancel_window(1000) == 1000: not yet elapsed.
    s.env.ledger().with_mut(|l| l.timestamp = 1_000);
    s.refund_client.auto_cancel_expired_request(&refund_id);
}

#[test]
fn test_auto_cancel_one_ledger_past_boundary_succeeds() {
    let s = setup_with_windows(1_000, 0);
    s.env.ledger().with_mut(|l| l.timestamp = 0);
    let refund_id = request_refund_for(&s, 100);

    s.env.ledger().with_mut(|l| l.timestamp = 1_001);
    s.refund_client.auto_cancel_expired_request(&refund_id);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Cancelled);
}

// ---------------------------------------------------------------------------
// auto_reject_stale_refund
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Auto-reject window has not elapsed")]
fn test_auto_reject_exactly_at_window_boundary_panics() {
    let s = setup_with_windows(0, 2_000);
    s.env.ledger().with_mut(|l| l.timestamp = 0);
    let refund_id = request_refund_for(&s, 100);

    // requested_at(0) + auto_reject_window(2000) == 2000: not yet elapsed.
    s.env.ledger().with_mut(|l| l.timestamp = 2_000);
    s.refund_client.auto_reject_stale_refund(&refund_id);
}

#[test]
fn test_auto_reject_one_ledger_past_boundary_succeeds() {
    let s = setup_with_windows(0, 2_000);
    s.env.ledger().with_mut(|l| l.timestamp = 0);
    let refund_id = request_refund_for(&s, 100);

    s.env.ledger().with_mut(|l| l.timestamp = 2_001);
    s.refund_client.auto_reject_stale_refund(&refund_id);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Rejected);
}
