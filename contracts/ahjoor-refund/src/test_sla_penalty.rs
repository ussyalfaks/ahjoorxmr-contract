#![cfg(test)]
//! #1171: Compensate customers when merchants miss the refund response deadline.
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use soroban_sdk::{Address, Env, String};

use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

const AMOUNT: i128 = 1_000;
const DISPUTE_WINDOW: u64 = 86_400;

struct S<'a> {
    env: Env,
    refund: AhjoorRefundContractClient<'a>,
    payments: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    customer: Address,
    merchant: Address,
    token_addr: Address,
    token: TokenClient<'a>,
    token_admin: TokenAdminClient<'a>,
}

fn setup<'a>() -> S<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let payments = AhjoorPaymentsContractClient::new(&env, &env.register(AhjoorPaymentsContract, ()));
    let refund = AhjoorRefundContractClient::new(&env, &env.register(AhjoorRefundContract, ()));
    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    payments.initialize(&admin, &admin, &0u32);
    refund.initialize(&admin, &payments.address, &DISPUTE_WINDOW, &None);
    refund.set_merchant_response_deadline(&admin, &100, &0);
    S {
        token: TokenClient::new(&env, &token_addr),
        token_admin: TokenAdminClient::new(&env, &token_addr),
        customer: Address::generate(&env),
        merchant: Address::generate(&env),
        env,
        refund,
        payments,
        admin,
        token_addr,
    }
}

fn fund_reserve(s: &S, amount: i128) {
    s.refund.set_reserve_config(&s.admin, &s.token_addr, &0, &0);
    s.token_admin.mint(&s.merchant, &amount);
    s.token.approve(&s.merchant, &s.refund.address, &amount, &(s.env.ledger().sequence() + 1_000));
    s.refund.deposit_merchant_reserve(&s.merchant, &amount);
}

fn reserve_balance(s: &S) -> i128 {
    s.refund.get_reserve_balance_summary(&s.merchant).merchant_reserve_balance
}

fn request(s: &S) -> u32 {
    s.token_admin.mint(&s.customer, &(AMOUNT * 3));
    let pid = s.payments.create_payment(&s.customer, &s.merchant, &AMOUNT, &s.token_addr, &None, &None, &None);
    s.payments.complete_payment(&pid);
    s.refund.request_refund(&s.customer, &pid, &AMOUNT, &String::from_str(&s.env, "late"), &0)
}

fn pass_deadline(s: &S, rid: u32) {
    let deadline = s.refund.get_refund(&rid).auto_approval_deadline_ledger;
    s.env.ledger().set_sequence_number(deadline);
}

// ---------- admin config ----------

#[test]
fn test_sla_penalty_defaults_to_zero() {
    let s = setup();
    assert_eq!(s.refund.get_sla_penalty_bps(), 0);
}

#[test]
fn test_set_sla_penalty_bps_round_trip_and_cap_boundary() {
    let s = setup();
    s.refund.set_sla_penalty_bps(&s.admin, &250);
    assert_eq!(s.refund.get_sla_penalty_bps(), 250);
    s.refund.set_sla_penalty_bps(&s.admin, &MAX_SLA_PENALTY_BPS);
    assert_eq!(s.refund.get_sla_penalty_bps(), MAX_SLA_PENALTY_BPS);
}

#[test]
#[should_panic(expected = "SlaPenaltyBpsTooHigh")]
fn test_set_sla_penalty_bps_above_cap_rejected() {
    let s = setup();
    s.refund.set_sla_penalty_bps(&s.admin, &(MAX_SLA_PENALTY_BPS + 1));
}

#[test]
#[should_panic(expected = "Only admin")]
fn test_set_sla_penalty_bps_non_admin_rejected() {
    let s = setup();
    s.refund.set_sla_penalty_bps(&s.merchant, &100);
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_set_sla_penalty_bps_when_paused_rejected() {
    let s = setup();
    s.refund.pause_contract(&s.admin, &String::from_str(&s.env, "maint"));
    s.refund.set_sla_penalty_bps(&s.admin, &100);
}

// ---------- penalty payout ----------

#[test]
fn test_penalty_paid_from_reserve_on_missed_deadline() {
    let s = setup();
    fund_reserve(&s, 10_000);
    s.refund.set_sla_penalty_bps(&s.admin, &500); // 5%
    let rid = request(&s);
    let before = s.token.balance(&s.customer);

    pass_deadline(&s, rid);
    s.refund.trigger_auto_approval(&rid);

    assert_eq!(s.token.balance(&s.customer), before + AMOUNT + 50);
    assert_eq!(reserve_balance(&s), 10_000 - 50);
    assert_eq!(s.refund.get_sla_penalty_paid(&rid), 50);
    assert_eq!(s.refund.get_refund(&rid).status, RefundStatus::Processed);
}

#[test]
fn test_penalty_capped_at_available_reserve() {
    let s = setup();
    fund_reserve(&s, 20);
    s.refund.set_sla_penalty_bps(&s.admin, &500);
    let rid = request(&s);
    let before = s.token.balance(&s.customer);

    pass_deadline(&s, rid);
    s.refund.trigger_auto_approval(&rid);

    assert_eq!(s.token.balance(&s.customer), before + AMOUNT + 20);
    assert_eq!(reserve_balance(&s), 0);
    assert_eq!(s.refund.get_sla_penalty_paid(&rid), 20);
}

#[test]
fn test_no_penalty_without_reserve() {
    let s = setup();
    s.refund.set_sla_penalty_bps(&s.admin, &500);
    let rid = request(&s);
    let before = s.token.balance(&s.customer);

    pass_deadline(&s, rid);
    s.refund.trigger_auto_approval(&rid);

    assert_eq!(s.token.balance(&s.customer), before + AMOUNT);
    assert_eq!(s.refund.get_sla_penalty_paid(&rid), 0);
}

#[test]
fn test_no_penalty_when_bps_zero() {
    let s = setup();
    fund_reserve(&s, 10_000);
    let rid = request(&s);
    pass_deadline(&s, rid);
    s.refund.trigger_auto_approval(&rid);
    assert_eq!(reserve_balance(&s), 10_000);
    assert_eq!(s.refund.get_sla_penalty_paid(&rid), 0);
}

#[test]
fn test_penalty_paid_on_dispute_window_auto_approval() {
    let s = setup();
    fund_reserve(&s, 10_000);
    s.refund.set_sla_penalty_bps(&s.admin, &1_000); // 10%
    let rid = request(&s);
    let before = s.token.balance(&s.customer);

    s.env.ledger().with_mut(|l| l.timestamp += DISPUTE_WINDOW + 1);
    s.refund.auto_approve_refund(&rid);

    assert_eq!(s.token.balance(&s.customer), before + AMOUNT + 100);
    assert_eq!(reserve_balance(&s), 10_000 - 100);
    assert_eq!(s.refund.get_sla_penalty_paid(&rid), 100);
}

#[test]
fn test_no_penalty_when_merchant_responds() {
    let s = setup();
    fund_reserve(&s, 10_000);
    s.refund.set_sla_penalty_bps(&s.admin, &500);
    let rid = request(&s);
    s.refund.approve_refund(&s.admin, &rid);
    s.refund.process_refund(&s.admin, &rid);
    assert_eq!(s.refund.get_sla_penalty_paid(&rid), 0);
}

// ---------- exemption ----------

#[test]
#[should_panic(expected = "MerchantAutoApprovalExempt")]
fn test_exempt_merchant_not_auto_approved_or_penalised() {
    let s = setup();
    fund_reserve(&s, 10_000);
    s.refund.set_sla_penalty_bps(&s.admin, &500);
    s.refund.set_merchant_auto_approve_exempt(&s.admin, &s.merchant, &true);
    let rid = request(&s);
    pass_deadline(&s, rid);
    s.refund.trigger_auto_approval(&rid);
}

#[test]
fn test_exempt_merchant_not_penalised_on_dispute_window_auto_approval() {
    let s = setup();
    fund_reserve(&s, 10_000);
    s.refund.set_sla_penalty_bps(&s.admin, &500);
    s.refund.set_merchant_auto_approve_exempt(&s.admin, &s.merchant, &true);
    let rid = request(&s);
    let before = s.token.balance(&s.customer);

    s.env.ledger().with_mut(|l| l.timestamp += DISPUTE_WINDOW + 1);
    s.refund.auto_approve_refund(&rid);

    assert_eq!(s.token.balance(&s.customer), before + AMOUNT);
    assert_eq!(reserve_balance(&s), 10_000);
    assert_eq!(s.refund.get_sla_penalty_paid(&rid), 0);
}
