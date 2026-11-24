#![cfg(test)]
//! Tests for subscription seat management: `add_seat`, `remove_seat`, `is_seat_active`.

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

// ---------------------------------------------------------------------------
//  Test harness
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    tac: TokenAdminClient<'a>,
    merchant: Address,
    subscriber: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().set_timestamp(1_000_000);
    env.ledger().set_sequence_number(1_000);

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let tac = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &fee_recipient, &0u32);
    client.set_min_collateral(&0i128);

    let merchant = Address::generate(&env);
    let subscriber = Address::generate(&env);

    // Fund subscriber with enough for many charges.
    tac.mint(&subscriber, &100_000);
    // Fund merchant for collateral.
    tac.mint(&merchant, &10_000_000);

    Setup { env, client, admin, token_addr, token_client, tac, merchant, subscriber }
}

/// Create a plan with `max_seats` and return plan_id.
fn create_plan(s: &Setup<'_>, max_seats: u32) -> u32 {
    let plan_id = 42u32;
    s.client.set_subscription_plan(
        &s.admin,
        &plan_id,
        &s.merchant,
        &s.token_addr,
        &100i128,
        &30u64,
        &true,
        &max_seats,
    );
    plan_id
}

/// Create a subscription linked to a plan. Returns subscription_id.
fn create_sub_on_plan(s: &Setup<'_>, plan_id: u32) -> u32 {
    let sub_id = s.client.create_subscription(
        &s.subscriber,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &(30 * 86_400u64), // 30 days
        &0u32,
    );
    // Link subscription to plan via change_subscription_plan.
    // Set up the plan first (already done by caller) then link.
    // We call change_subscription_plan which links plan_id into the sub.
    s.client.change_subscription_plan(&sub_id, &plan_id);
    sub_id
}

// ---------------------------------------------------------------------------
//  Happy path
// ---------------------------------------------------------------------------

#[test]
fn test_add_seat_makes_seat_active() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat);

    assert!(s.client.is_seat_active(&sub_id, &seat));
}

#[test]
fn test_multiple_seats_all_active() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);

    let seat1 = Address::generate(&s.env);
    let seat2 = Address::generate(&s.env);
    let seat3 = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat1);
    s.client.add_seat(&sub_id, &seat2);
    s.client.add_seat(&sub_id, &seat3);

    assert!(s.client.is_seat_active(&sub_id, &seat1));
    assert!(s.client.is_seat_active(&sub_id, &seat2));
    assert!(s.client.is_seat_active(&sub_id, &seat3));
}

#[test]
fn test_remove_seat_still_active_until_next_period() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat);
    assert!(s.client.is_seat_active(&sub_id, &seat));

    // Schedule removal.
    s.client.remove_seat(&sub_id, &seat);

    // Seat is still active because charge_subscription hasn't been called.
    assert!(s.client.is_seat_active(&sub_id, &seat));
}

#[test]
fn test_seat_removed_after_charge_subscription() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat);
    s.client.remove_seat(&sub_id, &seat);
    assert!(s.client.is_seat_active(&sub_id, &seat));

    // Advance time past the charge interval so charge_subscription succeeds.
    s.env.ledger().set_timestamp(1_000_000 + 30 * 86_400 + 1);
    s.client.charge_subscription(&sub_id);

    // Now the seat should have been flushed.
    assert!(!s.client.is_seat_active(&sub_id, &seat));
}

#[test]
fn test_add_seat_after_removal_flushed() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat);
    s.client.remove_seat(&sub_id, &seat);

    // Flush via charge.
    s.env.ledger().set_timestamp(1_000_000 + 30 * 86_400 + 1);
    s.client.charge_subscription(&sub_id);

    assert!(!s.client.is_seat_active(&sub_id, &seat));

    // Owner can add it again.
    s.client.add_seat(&sub_id, &seat);
    assert!(s.client.is_seat_active(&sub_id, &seat));
}

#[test]
fn test_is_seat_active_returns_false_for_unknown_address() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);

    let stranger = Address::generate(&s.env);
    assert!(!s.client.is_seat_active(&sub_id, &stranger));
}

#[test]
fn test_is_seat_active_returns_false_for_cancelled_subscription() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat);
    assert!(s.client.is_seat_active(&sub_id, &seat));

    s.client.cancel_subscription(&s.subscriber, &sub_id);
    assert!(!s.client.is_seat_active(&sub_id, &seat));
}

#[test]
fn test_unlinked_subscription_is_seat_active_always_false() {
    let s = setup();
    // A plain subscription with no plan_id.
    let sub_id = s.client.create_subscription(
        &s.subscriber,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &86_400u64,
        &0u32,
    );
    let seat = Address::generate(&s.env);
    assert!(!s.client.is_seat_active(&sub_id, &seat));
}

// ---------------------------------------------------------------------------
//  Error branches
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_add_seat_plan_has_no_seats_panics() {
    let s = setup();
    // Plan with max_seats = 0 → no seat management.
    let plan_id = create_plan(&s, 0);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);
    // Should panic with PlanHasNoSeats.
    s.client.add_seat(&sub_id, &seat);
}

#[test]
#[should_panic]
fn test_add_seat_at_capacity_panics() {
    let s = setup();
    let plan_id = create_plan(&s, 2); // only 2 seats
    let sub_id = create_sub_on_plan(&s, plan_id);

    let seat1 = Address::generate(&s.env);
    let seat2 = Address::generate(&s.env);
    let seat3 = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat1);
    s.client.add_seat(&sub_id, &seat2);
    // Third seat exceeds max_seats → PlanSeatsFull.
    s.client.add_seat(&sub_id, &seat3);
}

#[test]
#[should_panic]
fn test_add_seat_duplicate_panics() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);

    s.client.add_seat(&sub_id, &seat);
    // Adding same seat again → SeatAlreadyActive.
    s.client.add_seat(&sub_id, &seat);
}

#[test]
#[should_panic]
fn test_remove_seat_not_in_list_panics() {
    let s = setup();
    let plan_id = create_plan(&s, 5);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);

    // Seat was never added → SeatNotFound.
    s.client.remove_seat(&sub_id, &seat);
}

#[test]
#[should_panic]
fn test_remove_seat_plan_has_no_seats_panics() {
    let s = setup();
    let plan_id = create_plan(&s, 0);
    let sub_id = create_sub_on_plan(&s, plan_id);
    let seat = Address::generate(&s.env);
    // → PlanHasNoSeats
    s.client.remove_seat(&sub_id, &seat);
}

#[test]
#[should_panic]
fn test_add_seat_unlinked_subscription_panics() {
    let s = setup();
    // A subscription with no plan_id.
    let sub_id = s.client.create_subscription(
        &s.subscriber,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &86_400u64,
        &0u32,
    );
    let seat = Address::generate(&s.env);
    // → PlanHasNoSeats (no plan_id on sub)
    s.client.add_seat(&sub_id, &seat);
}
