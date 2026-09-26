#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

const DAY: u64 = 86_400;
const BASE_AMOUNT: i128 = 100;
const BASE_INTERVAL: u64 = 30 * DAY;

struct SubV2Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    subscriber: Address,
    merchant: Address,
}

fn sub_v2_setup<'a>() -> SubV2Setup<'a> {
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
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &fee_recipient, &0);

    let subscriber = Address::generate(&env);
    let merchant = Address::generate(&env);
    token_admin.mint(&subscriber, &10_000);

    SubV2Setup {
        env,
        client,
        admin,
        token_addr,
        token_client,
        subscriber,
        merchant,
    }
}

fn create_sub(s: &SubV2Setup) -> u32 {
    s.client.create_subscription(
        &s.subscriber,
        &s.merchant,
        &BASE_AMOUNT,
        &s.token_addr,
        &BASE_INTERVAL,
        &0,
    )
}

fn reason(s: &SubV2Setup) -> BytesN<32> {
    BytesN::from_array(&s.env, &[7u8; 32])
}

/// Ledgers the v2 resume path schedules ahead: interval rounded up to 5s ledgers.
fn interval_ledgers(interval_seconds: u64) -> u32 {
    ((interval_seconds + 4) / 5) as u32
}

// ===========================================================================
//  pause_subscription_v2 / resume_subscription_v2
// ===========================================================================

#[test]
fn test_subscriber_pause_and_resume_v2_cycle() {
    let s = sub_v2_setup();
    let id = create_sub(&s);

    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));
    let sub = s.client.get_subscription(&id);
    assert!(sub.paused);
    assert!(sub.active);
    assert_eq!(sub.pause_count, 1);
    assert_eq!(sub.paused_at, s.env.ledger().sequence() as u64);

    // Charging is blocked while paused.
    let res = s.client.try_charge_subscription(&id);
    assert_eq!(res.unwrap_err().unwrap(), Error::SubscriptionPaused.into());

    s.env.ledger().set_sequence_number(1_500);
    let next_due = s.client.resume_subscription_v2(&s.subscriber, &id);
    assert_eq!(next_due, 1_500 + interval_ledgers(BASE_INTERVAL));

    let sub = s.client.get_subscription(&id);
    assert!(!sub.paused);
    assert_eq!(sub.paused_at, 0);
    assert_eq!(sub.next_due_ledger, next_due as u64);
    // Pause history is retained across resume.
    assert_eq!(sub.pause_count, 1);

    // Billing works again after resume.
    s.client.charge_subscription(&id);
    assert_eq!(s.token_client.balance(&s.merchant), BASE_AMOUNT);
    assert_eq!(s.client.get_subscription(&id).charges_count, 1);
}

#[test]
fn test_merchant_can_resume_v2() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));

    s.client.resume_subscription_v2(&s.merchant, &id);
    assert!(!s.client.get_subscription(&id).paused);
}

#[test]
fn test_pause_v2_by_merchant_rejected_under_default_authority() {
    let s = sub_v2_setup();
    let id = create_sub(&s);

    let res = s
        .client
        .try_pause_subscription_v2(&s.merchant, &id, &reason(&s));
    assert_eq!(res.unwrap_err().unwrap(), Error::UnauthorizedPause.into());
    assert!(!s.client.get_subscription(&id).paused);
}

#[test]
#[should_panic(expected = "Unauthorized resume")]
fn test_resume_v2_by_stranger_panics() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));

    let stranger = Address::generate(&s.env);
    s.client.resume_subscription_v2(&stranger, &id);
}

#[test]
#[should_panic(expected = "Invalid subscription state")]
fn test_pause_v2_twice_panics() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));
    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));
}

#[test]
#[should_panic(expected = "Invalid subscription state")]
fn test_resume_v2_when_not_paused_panics() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.resume_subscription_v2(&s.subscriber, &id);
}

#[test]
fn test_pause_v2_count_limit_enforced() {
    let s = sub_v2_setup();
    let id = create_sub(&s);

    for _ in 0..MAX_SUBSCRIPTION_PAUSES {
        s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));
        s.client.resume_subscription_v2(&s.subscriber, &id);
    }
    assert_eq!(
        s.client.get_subscription(&id).pause_count,
        MAX_SUBSCRIPTION_PAUSES
    );

    let res = s
        .client
        .try_pause_subscription_v2(&s.subscriber, &id, &reason(&s));
    assert_eq!(res.unwrap_err().unwrap(), Error::PauseCountExceeded.into());
}

// ===========================================================================
//  admin_resume_subscription
// ===========================================================================

#[test]
fn test_admin_force_resume_paused_subscription() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));

    s.env.ledger().set_sequence_number(2_000);
    let next_due = s.client.admin_resume_subscription(&s.admin, &id);
    assert_eq!(next_due, 2_000 + interval_ledgers(BASE_INTERVAL));

    let sub = s.client.get_subscription(&id);
    assert!(!sub.paused);
    assert!(sub.active);
    assert_eq!(sub.paused_at, 0);
    assert_eq!(sub.next_due_ledger, next_due as u64);

    s.client.charge_subscription(&id);
    assert_eq!(s.token_client.balance(&s.merchant), BASE_AMOUNT);
}

#[test]
#[should_panic(expected = "Only admin can manage pause state")]
fn test_admin_resume_by_non_admin_panics() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));

    s.client.admin_resume_subscription(&s.subscriber, &id);
}

#[test]
#[should_panic(expected = "Invalid subscription state")]
fn test_admin_resume_when_not_paused_panics() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.admin_resume_subscription(&s.admin, &id);
}

// ===========================================================================
//  set_subscription_plan / change_subscription_plan
// ===========================================================================

#[test]
fn test_set_subscription_plan_stores_plan() {
    let s = sub_v2_setup();
    s.client
        .set_subscription_plan(&s.admin, &1, &s.merchant, &s.token_addr, &250, &7, &true);

    let plan = s.client.get_subscription_plan(&1);
    assert_eq!(plan.plan_id, 1);
    assert_eq!(plan.merchant, s.merchant);
    assert_eq!(plan.token, s.token_addr);
    assert_eq!(plan.amount, 250);
    assert_eq!(plan.interval_days, 7);
    assert!(plan.active);
}

#[test]
fn test_change_subscription_plan_applies_to_next_billing_cycle() {
    const NEW_AMOUNT: i128 = 250;
    const NEW_INTERVAL_DAYS: u64 = 7;
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client.set_subscription_plan(
        &s.admin,
        &1,
        &s.merchant,
        &s.token_addr,
        &NEW_AMOUNT,
        &NEW_INTERVAL_DAYS,
        &true,
    );

    // First charge on the original plan.
    let t0 = s.env.ledger().timestamp();
    s.client.charge_subscription(&id);
    assert_eq!(s.token_client.balance(&s.merchant), BASE_AMOUNT);

    // Switch plans halfway through the 30-day period: 15 unused days of the
    // old plan become a one-time credit of 100 * 15 / 30 = 50.
    let half_period_ledgers = (BASE_INTERVAL / 2 / 5) as u32;
    let change_seq = s.env.ledger().sequence() + half_period_ledgers;
    s.env.ledger().set_sequence_number(change_seq);
    s.client.change_subscription_plan(&id, &1);

    let sub = s.client.get_subscription(&id);
    assert_eq!(sub.plan_id, Some(1));
    assert_eq!(sub.amount, NEW_AMOUNT);
    assert_eq!(sub.interval_seconds, NEW_INTERVAL_DAYS * DAY);
    assert_eq!(sub.pending_prorated_credit, 50);
    assert_eq!(
        sub.next_due_ledger,
        change_seq as u64 + NEW_INTERVAL_DAYS * DAY / 5
    );

    // The new, shorter interval governs the next cycle.
    s.env.ledger().set_timestamp(t0 + NEW_INTERVAL_DAYS * DAY - 1);
    assert!(s.client.try_charge_subscription(&id).is_err());

    s.env.ledger().set_timestamp(t0 + NEW_INTERVAL_DAYS * DAY);
    s.client.charge_subscription(&id);
    assert_eq!(
        s.token_client.balance(&s.merchant),
        BASE_AMOUNT + (NEW_AMOUNT - 50)
    );
    assert_eq!(s.client.get_subscription(&id).pending_prorated_credit, 0);

    // After the one-time credit is used up, the full new amount is billed.
    s.env.ledger().set_timestamp(t0 + 2 * NEW_INTERVAL_DAYS * DAY);
    s.client.charge_subscription(&id);
    assert_eq!(
        s.token_client.balance(&s.merchant),
        BASE_AMOUNT + (NEW_AMOUNT - 50) + NEW_AMOUNT
    );
    assert_eq!(s.client.get_subscription(&id).charges_count, 3);
}

#[test]
fn test_change_subscription_plan_rejected_while_paused() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client
        .set_subscription_plan(&s.admin, &1, &s.merchant, &s.token_addr, &250, &7, &true);
    s.client.pause_subscription_v2(&s.subscriber, &id, &reason(&s));

    let res = s.client.try_change_subscription_plan(&id, &1);
    assert_eq!(res.unwrap_err().unwrap(), Error::SubscriptionPaused.into());
    assert_eq!(s.client.get_subscription(&id).plan_id, None);
}

#[test]
#[should_panic(expected = "Subscription plan inactive")]
fn test_change_to_inactive_plan_panics() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    s.client
        .set_subscription_plan(&s.admin, &1, &s.merchant, &s.token_addr, &250, &7, &false);
    s.client.change_subscription_plan(&id, &1);
}

#[test]
#[should_panic(expected = "Plan merchant mismatch")]
fn test_change_to_other_merchants_plan_panics() {
    let s = sub_v2_setup();
    let id = create_sub(&s);
    let other_merchant = Address::generate(&s.env);
    s.client
        .set_subscription_plan(&s.admin, &1, &other_merchant, &s.token_addr, &250, &7, &true);
    s.client.change_subscription_plan(&id, &1);
}
