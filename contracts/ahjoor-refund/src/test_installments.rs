#![cfg(test)]
//! #1177: Large refunds paid in installments.
use super::test_delegate_caps::{request_refund, setup, Setup};
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::Address;

const DAY: u64 = 86_400;

fn active_plan(s: &Setup, amount: i128, count: u32) -> u32 {
    let id = request_refund(s, amount);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &count, &DAY);
    s.refund_client.accept_installment_plan(&s.customer, &id);
    s.token_admin_client.mint(&s.merchant, &amount);
    id
}

#[test]
fn test_installments_happy_path_tracks_paid_and_remaining() {
    let s = setup();
    let id = active_plan(&s, 1_000, 3);
    assert_eq!(
        s.refund_client.get_refund(&id).status,
        RefundStatus::InInstallments
    );

    let before = s.token_client.balance(&s.customer);
    s.refund_client.pay_installment(&s.merchant, &id);
    let plan = s.refund_client.get_installment_plan(&id).unwrap();
    assert_eq!(plan.paid_count, 1);
    assert_eq!(plan.paid_amount, 333);
    assert_eq!(plan.remaining_amount, 667);

    s.env.ledger().with_mut(|l| l.timestamp += DAY);
    s.refund_client.pay_installment(&s.merchant, &id);
    s.env.ledger().with_mut(|l| l.timestamp += DAY);
    s.refund_client.pay_installment(&s.merchant, &id);

    let plan = s.refund_client.get_installment_plan(&id).unwrap();
    assert_eq!(plan.paid_count, 3);
    assert_eq!(plan.paid_amount, 1_000); // last installment covers remainder
    assert_eq!(plan.remaining_amount, 0);
    assert_eq!(s.token_client.balance(&s.customer), before + 1_000);
    assert_eq!(s.refund_client.get_refund(&id).status, RefundStatus::Processed);

    // Nothing left to pay.
    assert!(s.refund_client.try_pay_installment(&s.merchant, &id).is_err());
}

#[test]
fn test_proposal_does_not_change_status_until_accepted() {
    let s = setup();
    let id = request_refund(&s, 600);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &2, &DAY);
    let plan = s.refund_client.get_installment_plan(&id).unwrap();
    assert!(!plan.accepted);
    assert_eq!(plan.installment_amount, 300);
    assert_eq!(s.refund_client.get_refund(&id).status, RefundStatus::Requested);
    // Merchant may replace an unaccepted proposal.
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &3, &DAY);
    assert_eq!(
        s.refund_client.get_installment_plan(&id).unwrap().installment_count,
        3
    );
}

#[test]
fn test_missed_installment_escalates_to_senior() {
    let s = setup();
    let arbiter = Address::generate(&s.env);
    s.refund_client.set_senior_arbiter(&s.admin, &arbiter);
    let id = active_plan(&s, 1_000, 4);
    s.refund_client.pay_installment(&s.merchant, &id);

    s.env.ledger().with_mut(|l| l.timestamp += 2 * DAY + 1);
    assert!(s.refund_client.try_pay_installment(&s.merchant, &id).is_err());
    s.refund_client.report_missed_installment(&id);

    let r = s.refund_client.get_refund(&id);
    assert_eq!(r.status, RefundStatus::EscalatedToSenior);
    assert_eq!(r.amount, 750); // only the unpaid balance goes to senior review
    assert!(s.refund_client.get_installment_plan(&id).unwrap().escalated);
    assert_eq!(s.refund_client.list_pending_senior_escalations(&0, &10).len(), 1);
}

#[test]
#[should_panic(expected = "Installment overdue")]
fn test_pay_overdue_installment_panics() {
    let s = setup();
    let id = active_plan(&s, 1_000, 2);
    s.env.ledger().with_mut(|l| l.timestamp += DAY + 1);
    s.refund_client.pay_installment(&s.merchant, &id);
}

#[test]
#[should_panic(expected = "Installment not yet overdue")]
fn test_report_before_due_panics() {
    let s = setup();
    let id = active_plan(&s, 1_000, 2);
    s.refund_client.report_missed_installment(&id);
}

#[test]
#[should_panic(expected = "SeniorArbiterNotConfigured")]
fn test_report_without_arbiter_panics() {
    let s = setup();
    let id = active_plan(&s, 1_000, 2);
    s.env.ledger().with_mut(|l| l.timestamp += DAY + 1);
    s.refund_client.report_missed_installment(&id);
}

#[test]
#[should_panic(expected = "Refund is not being paid in installments")]
fn test_report_without_active_plan_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client.report_missed_installment(&id);
}

#[test]
#[should_panic(expected = "Only the refund's merchant can propose an installment plan")]
fn test_propose_by_non_merchant_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    let stranger = Address::generate(&s.env);
    s.refund_client
        .propose_installment_plan(&stranger, &id, &2, &DAY);
}

#[test]
#[should_panic(expected = "Refund is not in requested status")]
fn test_propose_on_approved_refund_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client.approve_refund(&s.admin, &id);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &2, &DAY);
}

#[test]
#[should_panic(expected = "Invalid installment count")]
fn test_propose_too_few_installments_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &1, &DAY);
}

#[test]
#[should_panic(expected = "Invalid installment count")]
fn test_propose_too_many_installments_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &13, &DAY);
}

#[test]
#[should_panic(expected = "Installment interval must be positive")]
fn test_propose_zero_interval_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &2, &0);
}

#[test]
#[should_panic(expected = "Refund too small for installments")]
fn test_propose_refund_too_small_panics() {
    let s = setup();
    let id = request_refund(&s, 5);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &12, &DAY);
}

#[test]
#[should_panic(expected = "Installment plan already active")]
fn test_propose_after_acceptance_panics() {
    let s = setup();
    let id = active_plan(&s, 1_000, 2);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &3, &DAY);
}

#[test]
#[should_panic(expected = "Only the refund's customer can accept the plan")]
fn test_accept_by_non_customer_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &2, &DAY);
    s.refund_client.accept_installment_plan(&s.merchant, &id);
}

#[test]
#[should_panic(expected = "No installment plan proposed")]
fn test_accept_without_proposal_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client.accept_installment_plan(&s.customer, &id);
}

#[test]
#[should_panic(expected = "Installment plan already active")]
fn test_accept_twice_panics() {
    let s = setup();
    let id = active_plan(&s, 1_000, 2);
    s.refund_client.accept_installment_plan(&s.customer, &id);
}

#[test]
#[should_panic(expected = "Refund is not in requested status")]
fn test_accept_after_refund_approved_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &2, &DAY);
    s.refund_client.approve_refund(&s.admin, &id);
    s.refund_client.accept_installment_plan(&s.customer, &id);
}

#[test]
#[should_panic(expected = "Only the refund's merchant can pay installments")]
fn test_pay_by_non_merchant_panics() {
    let s = setup();
    let id = active_plan(&s, 1_000, 2);
    s.refund_client.pay_installment(&s.customer, &id);
}

#[test]
#[should_panic(expected = "Refund is not being paid in installments")]
fn test_pay_without_accepted_plan_panics() {
    let s = setup();
    let id = request_refund(&s, 500);
    s.refund_client
        .propose_installment_plan(&s.merchant, &id, &2, &DAY);
    s.refund_client.pay_installment(&s.merchant, &id);
}
