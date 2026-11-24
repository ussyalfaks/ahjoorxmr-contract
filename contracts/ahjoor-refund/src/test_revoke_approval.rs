#![cfg(test)]
//! #1176: Merchant revokes a mistaken approval before processing.
use super::test_delegate_caps::{request_refund, setup, APPEAL_WINDOW};
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN};

fn reason(s: &super::test_delegate_caps::Setup) -> BytesN<32> {
    BytesN::from_array(&s.env, &[7u8; 32])
}

#[test]
fn test_revoke_approval_happy_path_and_appeal() {
    let s = setup();
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.admin, &id);

    s.refund_client
        .revoke_refund_approval(&s.merchant, &id, &reason(&s));

    let r = s.refund_client.get_refund(&id);
    assert_eq!(r.status, RefundStatus::ApprovalRevoked);
    assert!(r.approved_at.is_none());
    assert_eq!(
        s.refund_client.get_approval_revocation(&id),
        Some(reason(&s))
    );

    // Revoked refund can no longer be processed...
    assert!(s.refund_client.try_process_refund(&s.admin, &id).is_err());

    // ...but the customer can appeal, and the admin can uphold the refund.
    s.refund_client.appeal_refund(&s.customer, &id);
    assert_eq!(
        s.refund_client.get_refund(&id).status,
        RefundStatus::UnderAppeal
    );
    let before = s.token_client.balance(&s.customer);
    s.refund_client.resolve_appeal(&s.admin, &id, &true);
    assert_eq!(s.refund_client.get_refund(&id).status, RefundStatus::Processed);
    assert_eq!(s.token_client.balance(&s.customer), before + 300);
}

#[test]
fn test_revoke_at_window_edge_succeeds() {
    let s = setup();
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.admin, &id);
    let window = s.refund_client.get_revoke_approval_window();
    s.env.ledger().with_mut(|l| l.timestamp += window);
    s.refund_client
        .revoke_refund_approval(&s.merchant, &id, &reason(&s));
    assert_eq!(
        s.refund_client.get_refund(&id).status,
        RefundStatus::ApprovalRevoked
    );
}

#[test]
#[should_panic(expected = "Revoke window has expired")]
fn test_revoke_after_window_panics() {
    let s = setup();
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.admin, &id);
    let window = s.refund_client.get_revoke_approval_window();
    s.env.ledger().with_mut(|l| l.timestamp += window + 1);
    s.refund_client
        .revoke_refund_approval(&s.merchant, &id, &reason(&s));
}

#[test]
#[should_panic(expected = "Only the refund's merchant can revoke an approval")]
fn test_revoke_by_non_merchant_panics() {
    let s = setup();
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.admin, &id);
    let stranger = Address::generate(&s.env);
    s.refund_client
        .revoke_refund_approval(&stranger, &id, &reason(&s));
}

#[test]
#[should_panic(expected = "Refund is not in approved status")]
fn test_revoke_unapproved_refund_panics() {
    let s = setup();
    let id = request_refund(&s, 300);
    s.refund_client
        .revoke_refund_approval(&s.merchant, &id, &reason(&s));
}

#[test]
#[should_panic(expected = "Refund is not in approved status")]
fn test_revoke_processed_refund_panics() {
    let s = setup();
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.admin, &id);
    s.refund_client.process_refund(&s.admin, &id);
    s.refund_client
        .revoke_refund_approval(&s.merchant, &id, &reason(&s));
}

#[test]
#[should_panic(expected = "Refund not found")]
fn test_revoke_unknown_refund_panics() {
    let s = setup();
    s.refund_client
        .revoke_refund_approval(&s.merchant, &999u32, &reason(&s));
}

#[test]
#[should_panic(expected = "Approval already disbursed from reserve")]
fn test_revoke_after_reserve_draw_panics() {
    let s = setup();
    s.token_admin_client.mint(&s.merchant, &500);
    s.refund_client
        .deposit_reserve(&s.merchant, &s.token_addr, &500);
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.admin, &id);
    s.refund_client
        .revoke_refund_approval(&s.merchant, &id, &reason(&s));
}

#[test]
fn test_set_revoke_window() {
    let s = setup();
    assert_eq!(s.refund_client.get_revoke_approval_window(), 3_600);
    s.refund_client.set_revoke_approval_window(&s.admin, &60);
    assert_eq!(s.refund_client.get_revoke_approval_window(), 60);
}

#[test]
#[should_panic(expected = "Revoke window must be positive")]
fn test_set_revoke_window_zero_panics() {
    let s = setup();
    s.refund_client.set_revoke_approval_window(&s.admin, &0);
}

#[test]
#[should_panic(expected = "Appeal window has expired")]
fn test_appeal_after_revocation_window_expired_panics() {
    let s = setup();
    let id = request_refund(&s, 300);
    s.refund_client.approve_refund(&s.admin, &id);
    s.refund_client
        .revoke_refund_approval(&s.merchant, &id, &reason(&s));
    s.env
        .ledger()
        .with_mut(|l| l.timestamp += APPEAL_WINDOW + 1);
    s.refund_client.appeal_refund(&s.customer, &id);
}
