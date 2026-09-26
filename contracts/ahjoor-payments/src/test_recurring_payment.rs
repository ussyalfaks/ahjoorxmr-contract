#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

const AMOUNT: i128 = 250;
const INTERVAL: u64 = 86_400;

struct RecurringSetup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    token_addr: Address,
    token_client: TokenClient<'a>,
    payer: Address,
    payee: Address,
}

fn recurring_setup<'a>() -> RecurringSetup<'a> {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &fee_recipient, &0);

    let payer = Address::generate(&env);
    let payee = Address::generate(&env);
    token_admin.mint(&payer, &10_000);
    // execute_recurring pulls funds via transfer_from, so the payer grants
    // the contract an allowance up front.
    token_client.approve(
        &payer,
        &client.address,
        &10_000,
        &(env.ledger().sequence() + 100_000),
    );

    RecurringSetup {
        env,
        client,
        token_addr,
        token_client,
        payer,
        payee,
    }
}

fn create_schedule(s: &RecurringSetup, max_cycles: u32) -> u32 {
    s.client.create_recurring_payment(
        &s.payer,
        &s.payee,
        &s.token_addr,
        &AMOUNT,
        &INTERVAL,
        &max_cycles,
    )
}

fn advance_to(s: &RecurringSetup, timestamp: u64) {
    s.env.ledger().set_timestamp(timestamp);
}

#[test]
fn test_create_recurring_payment_initial_schedule() {
    let s = recurring_setup();
    let now = s.env.ledger().timestamp();
    let id = create_schedule(&s, 3);

    let schedule = s.client.get_recurring_schedule(&id);
    assert_eq!(schedule.schedule_id, id);
    assert_eq!(schedule.payer, s.payer);
    assert_eq!(schedule.payee, s.payee);
    assert_eq!(schedule.amount, AMOUNT);
    assert_eq!(schedule.interval_seconds, INTERVAL);
    assert_eq!(schedule.max_cycles, 3);
    assert_eq!(schedule.cycles_executed, 0);
    assert_eq!(schedule.next_due, now);
    assert!(schedule.active);
}

#[test]
fn test_execute_recurring_on_time_transfers_and_advances_schedule() {
    let s = recurring_setup();
    let start = s.env.ledger().timestamp();
    let id = create_schedule(&s, 0);

    // First cycle is due immediately at creation.
    s.client.execute_recurring(&id);
    assert_eq!(s.token_client.balance(&s.payee), AMOUNT);
    assert_eq!(s.token_client.balance(&s.payer), 10_000 - AMOUNT);

    let schedule = s.client.get_recurring_schedule(&id);
    assert_eq!(schedule.cycles_executed, 1);
    assert_eq!(schedule.next_due, start + INTERVAL);
    assert!(schedule.active);

    // Exactly at next_due the next cycle is allowed and advances again.
    advance_to(&s, start + INTERVAL);
    s.client.execute_recurring(&id);
    assert_eq!(s.token_client.balance(&s.payee), AMOUNT * 2);

    let schedule = s.client.get_recurring_schedule(&id);
    assert_eq!(schedule.cycles_executed, 2);
    assert_eq!(schedule.next_due, start + 2 * INTERVAL);
}

#[test]
fn test_execute_recurring_next_due_is_based_on_execution_time() {
    let s = recurring_setup();
    let start = s.env.ledger().timestamp();
    let id = create_schedule(&s, 0);
    s.client.execute_recurring(&id);

    // A late execution schedules the next cycle one interval after it ran.
    let late = start + INTERVAL + 5_000;
    advance_to(&s, late);
    s.client.execute_recurring(&id);
    assert_eq!(s.client.get_recurring_schedule(&id).next_due, late + INTERVAL);
}

#[test]
#[should_panic(expected = "Next execution is not yet due")]
fn test_execute_recurring_before_next_due_panics() {
    let s = recurring_setup();
    let start = s.env.ledger().timestamp();
    let id = create_schedule(&s, 0);
    s.client.execute_recurring(&id);

    advance_to(&s, start + INTERVAL - 1);
    s.client.execute_recurring(&id);
}

#[test]
fn test_early_execution_is_rejected_without_side_effects() {
    let s = recurring_setup();
    let start = s.env.ledger().timestamp();
    let id = create_schedule(&s, 0);
    s.client.execute_recurring(&id);

    advance_to(&s, start + INTERVAL - 1);
    assert!(s.client.try_execute_recurring(&id).is_err());

    let schedule = s.client.get_recurring_schedule(&id);
    assert_eq!(schedule.cycles_executed, 1);
    assert_eq!(schedule.next_due, start + INTERVAL);
    assert_eq!(s.token_client.balance(&s.payee), AMOUNT);
}

#[test]
fn test_execute_recurring_auto_deactivates_at_max_cycles() {
    let s = recurring_setup();
    let start = s.env.ledger().timestamp();
    let id = create_schedule(&s, 2);

    s.client.execute_recurring(&id);
    advance_to(&s, start + INTERVAL);
    s.client.execute_recurring(&id);

    let schedule = s.client.get_recurring_schedule(&id);
    assert_eq!(schedule.cycles_executed, 2);
    assert!(!schedule.active);

    advance_to(&s, start + 2 * INTERVAL);
    assert!(s.client.try_execute_recurring(&id).is_err());
    assert_eq!(s.token_client.balance(&s.payee), AMOUNT * 2);
}

#[test]
fn test_cancel_recurring_payment_stops_executions() {
    let s = recurring_setup();
    let start = s.env.ledger().timestamp();
    let id = create_schedule(&s, 0);
    s.client.execute_recurring(&id);

    s.client.cancel_recurring_payment(&s.payer, &id);
    assert!(!s.client.get_recurring_schedule(&id).active);

    // Even once the next cycle is due, nothing more is executed.
    advance_to(&s, start + 3 * INTERVAL);
    assert!(s.client.try_execute_recurring(&id).is_err());

    let schedule = s.client.get_recurring_schedule(&id);
    assert_eq!(schedule.cycles_executed, 1);
    assert_eq!(s.token_client.balance(&s.payee), AMOUNT);
    assert_eq!(s.token_client.balance(&s.payer), 10_000 - AMOUNT);
}

#[test]
#[should_panic(expected = "Recurring schedule is not active")]
fn test_execute_recurring_after_cancel_panics() {
    let s = recurring_setup();
    let id = create_schedule(&s, 0);
    s.client.cancel_recurring_payment(&s.payer, &id);
    s.client.execute_recurring(&id);
}

#[test]
#[should_panic(expected = "Recurring schedule is already inactive")]
fn test_cancel_recurring_payment_twice_panics() {
    let s = recurring_setup();
    let id = create_schedule(&s, 0);
    s.client.cancel_recurring_payment(&s.payer, &id);
    s.client.cancel_recurring_payment(&s.payer, &id);
}

#[test]
#[should_panic(expected = "Only the payer can cancel this schedule")]
fn test_cancel_recurring_payment_by_non_payer_panics() {
    let s = recurring_setup();
    let id = create_schedule(&s, 0);
    s.client.cancel_recurring_payment(&s.payee, &id);
}
