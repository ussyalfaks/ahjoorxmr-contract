#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    merchant: Address,
    fee_recipient: Address,
    token: Address,
    token_client: TokenClient<'a>,
    token_admin: TokenAdminClient<'a>,
}

/// Contract with a 100 bps (1%) default protocol fee.
fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token);
    let token_admin = TokenAdminClient::new(&env, &token);

    client.initialize(&admin, &fee_recipient, &100u32);
    client.set_min_collateral(&0i128);
    client.approve_merchant(&merchant);

    Setup { env, client, admin, merchant, fee_recipient, token, token_client, token_admin }
}

/// Pay and complete a payment of `amount`; returns the fee the recipient received.
fn settle(s: &Setup, amount: i128) -> i128 {
    let customer = Address::generate(&s.env);
    s.token_admin.mint(&customer, &amount);
    let before = s.token_client.balance(&s.fee_recipient);
    let pid = s.client.create_payment(&customer, &s.merchant, &amount, &s.token, &None, &None, &None);
    s.client.complete_payment(&pid);
    s.token_client.balance(&s.fee_recipient) - before
}

#[test]
fn test_no_caps_keeps_bps_fee() {
    let s = setup();
    assert_eq!(s.client.get_token_fee_caps(&s.token), None);
    assert_eq!(settle(&s, 10_000), 100);
}

#[test]
fn test_min_fee_lifts_micro_payment_fee() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &50, &0);
    // 1% of 1_000 = 10, lifted to the 50 minimum.
    assert_eq!(settle(&s, 1_000), 50);
}

#[test]
fn test_max_fee_caps_large_payment_fee() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &0, &500);
    // 1% of 1_000_000 = 10_000, capped at 500.
    assert_eq!(settle(&s, 1_000_000), 500);
}

#[test]
fn test_fee_within_bounds_is_unchanged() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &10, &500);
    assert_eq!(settle(&s, 20_000), 200);
}

#[test]
fn test_fee_never_exceeds_amount() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &1_000, &0);
    // Minimum 1_000 would exceed the 300 payment, so the fee is the whole amount.
    assert_eq!(settle(&s, 300), 300);
}

#[test]
fn test_caps_apply_to_fee_tiers() {
    let s = setup();
    s.client.update_fee_tiers(
        &s.admin,
        &vec![&s.env, FeeTier { min_volume: 0, fee_bps: 300 }],
    );
    s.client.set_token_fee_caps(&s.admin, &s.token, &0, &400);
    // 3% tier of 100_000 = 3_000, capped at 400.
    assert_eq!(settle(&s, 100_000), 400);
}

#[test]
fn test_caps_are_per_token() {
    let s = setup();
    let other = s.env.register_stellar_asset_contract_v2(s.admin.clone()).address();
    s.client.set_token_fee_caps(&s.admin, &other, &500, &0);
    // The configured token is unaffected.
    assert_eq!(settle(&s, 1_000), 10);
    assert_eq!(
        s.client.get_token_fee_caps(&other),
        Some(FeeCaps { min_fee: 500, max_fee: 0 })
    );
}

#[test]
fn test_zero_caps_remove_configuration() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &50, &0);
    s.client.set_token_fee_caps(&s.admin, &s.token, &0, &0);
    assert_eq!(s.client.get_token_fee_caps(&s.token), None);
    assert_eq!(settle(&s, 1_000), 10);
}

#[test]
fn test_split_payment_fee_is_capped_and_totals_balance() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &0, &100);
    let r1 = Address::generate(&s.env);
    let r2 = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    s.token_admin.mint(&customer, &100_000);

    let splits = vec![
        &s.env,
        SplitRecipient { recipient: r1.clone(), bps: 6000 },
        SplitRecipient { recipient: r2.clone(), bps: 4000 },
    ];
    let pid = s.client.create_payment_with_options(
        &customer,
        &s.merchant,
        &100_000,
        &s.token,
        &None,
        &None,
        &Some(splits),
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    // Uncapped fee would be 1_000; capped total is 100.
    assert_eq!(s.token_client.balance(&s.fee_recipient), 100);
    assert_eq!(
        s.token_client.balance(&r1) + s.token_client.balance(&r2) + 100,
        100_000
    );
}

#[test]
#[should_panic(expected = "Only admin can set fee caps")]
fn test_set_caps_non_admin_panics() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.client.set_token_fee_caps(&stranger, &s.token, &1, &10);
}

#[test]
#[should_panic(expected = "Fee caps cannot be negative")]
fn test_set_caps_negative_min_panics() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &-1, &10);
}

#[test]
#[should_panic(expected = "Fee caps cannot be negative")]
fn test_set_caps_negative_max_panics() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &0, &-10);
}

#[test]
#[should_panic(expected = "min_fee cannot exceed max_fee")]
fn test_set_caps_min_above_max_panics() {
    let s = setup();
    s.client.set_token_fee_caps(&s.admin, &s.token, &100, &10);
}
