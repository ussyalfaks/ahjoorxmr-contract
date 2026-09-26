#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

fn setup_getters<'a>() -> (
    Env,
    AhjoorRefundContractClient<'a>,
    AhjoorPaymentsContractClient<'a>,
    Address, // admin
    Address, // token
    TokenClient<'a>,
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    (env, refund_client, payment_client, admin, token_addr, token_client, token_admin)
}

// ===========================================================================
//  Test: get_merchant_auto_approve_exempt
// ===========================================================================

#[test]
fn test_get_merchant_auto_approve_exempt_default_false() {
    let (_env, refund_client, _payment_client, _admin, _token_addr, _tc, _token_admin) = setup_getters();
    let merchant = Address::generate(&_env);

    // Should return false by default when not set
    let exempt = refund_client.get_merchant_auto_approve_exempt(&merchant);
    assert_eq!(exempt, false);
}

#[test]
fn test_get_merchant_auto_approve_exempt_after_set_true() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();
    let merchant = Address::generate(&_env);

    // Set merchant as exempt
    refund_client.set_merchant_auto_approve_exempt(&admin, &merchant, &true);

    // Should return true
    let exempt = refund_client.get_merchant_auto_approve_exempt(&merchant);
    assert_eq!(exempt, true);
}

// ===========================================================================
//  Test: get_abuse_block_config
// ===========================================================================

#[test]
fn test_get_abuse_block_config_defaults() {
    let (_env, refund_client, _payment_client, _admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Should return defaults when not configured
    let (threshold, block_duration) = refund_client.get_abuse_block_config();
    
    // Default threshold is 100, default block_duration is DEFAULT_BLOCK_DURATION_LEDGERS (518_400)
    assert_eq!(threshold, 100);
    assert_eq!(block_duration, 518_400);
}

#[test]
fn test_get_abuse_block_config_after_set() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Set custom values
    refund_client.set_abuse_block_threshold(&admin, &50u32);
    refund_client.set_block_duration_ledgers(&admin, &100_000u64);

    // Should return the configured values
    let (threshold, block_duration) = refund_client.get_abuse_block_config();
    assert_eq!(threshold, 50);
    assert_eq!(block_duration, 100_000);
}

// ===========================================================================
//  Test: get_merchant_response_window
// ===========================================================================

#[test]
fn test_get_merchant_response_window_default_zero() {
    let (_env, refund_client, _payment_client, _admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Should return 0 by default when not configured
    let window = refund_client.get_merchant_response_window();
    assert_eq!(window, 0);
}

#[test]
fn test_get_merchant_response_window_after_set() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Set custom merchant response window
    let expected_window = 120_960u32; // ~7 days in ledgers
    refund_client.set_merchant_response_window(&admin, &expected_window);

    // Should return the configured value
    let window = refund_client.get_merchant_response_window();
    assert_eq!(window, expected_window);
}

// ===========================================================================
//  Test: get_senior_review_window (#895)
// ===========================================================================

#[test]
fn test_get_senior_review_window_default_and_after_set() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Default applies before configuration
    assert_eq!(refund_client.get_senior_review_window(), 34_560u32);

    refund_client.set_senior_review_window(&admin, &300u32);
    assert_eq!(refund_client.get_senior_review_window(), 300u32);
}

// ===========================================================================
//  Test: get_abuse_score_decay_params (#896)
// ===========================================================================

#[test]
fn test_get_abuse_score_decay_params_default_and_after_set() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Default applies before configuration
    assert_eq!(refund_client.get_abuse_score_decay_params(), (10_000u64, 5_000u32));

    refund_client.set_abuse_score_decay_params(&admin, &2_000u64, &7_500u32);
    assert_eq!(refund_client.get_abuse_score_decay_params(), (2_000u64, 7_500u32));
}

// ===========================================================================
//  Test: get_counter_offer_expiry_seconds (#891)
// ===========================================================================

#[test]
fn test_get_counter_offer_expiry_seconds_default() {
    let (_env, refund_client, _payment_client, _admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Default is 172_800 (48 hours) before any configuration
    assert_eq!(refund_client.get_counter_offer_expiry_seconds(), 172_800u64);
}

#[test]
fn test_get_counter_offer_expiry_seconds_after_set() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();

    refund_client.set_counter_offer_expiry_seconds(&admin, &3_600u64);
    assert_eq!(refund_client.get_counter_offer_expiry_seconds(), 3_600u64);
}

// ===========================================================================
//  Test: get_auto_approve_on_senior_miss (#889)
// ===========================================================================

#[test]
fn test_get_auto_approve_on_senior_miss_default_false() {
    let (_env, refund_client, _payment_client, _admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Default is false before any configuration
    assert_eq!(refund_client.get_auto_approve_on_senior_miss(), false);
}

#[test]
fn test_get_auto_approve_on_senior_miss_after_set() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();

    refund_client.set_auto_approve_on_senior_miss(&admin, &true);
    assert_eq!(refund_client.get_auto_approve_on_senior_miss(), true);
}

// ===========================================================================
//  Test: get_block_duration_ledgers (#890)
// ===========================================================================

#[test]
fn test_get_block_duration_ledgers_default() {
    let (_env, refund_client, _payment_client, _admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Default is 518_400 (~30 days) before any configuration
    assert_eq!(refund_client.get_block_duration_ledgers(), 518_400u64);
}

#[test]
fn test_get_block_duration_ledgers_after_set() {
    let (_env, refund_client, _payment_client, admin, _token_addr, _tc, _token_admin) = setup_getters();

    refund_client.set_block_duration_ledgers(&admin, &100_000u64);
    assert_eq!(refund_client.get_block_duration_ledgers(), 100_000u64);
}

// ===========================================================================
//  Test: get_global_refund_policy (#897)
// ===========================================================================

#[test]
fn test_get_global_refund_policy_default_and_after_set() {
    let (env, refund_client, _payment_client, _admin, _token_addr, _tc, _token_admin) = setup_getters();

    // Default applies before configuration
    assert_eq!(
        refund_client.get_global_refund_policy(),
        (u32::MAX, 10_000u32, Vec::new(&env))
    );

    let mut tags = Vec::new(&env);
    tags.push_back(Symbol::new(&env, "digital"));
    tags.push_back(Symbol::new(&env, "sale"));
    // Seed storage exactly as `set_global_refund_policy` writes it. The setter
    // calls `admin.require_auth()` and then `require_admin` (which calls it
    // again), which `mock_all_auths` rejects as a duplicate authorization — a
    // pre-existing issue in that setter (see `seed_global_refund_policy` in test.rs).
    env.as_contract(&refund_client.address, || {
        env.storage().instance().set(
            &DataKey2::GlobalRefundPolicy,
            &RefundPolicy {
                eligible_window_ledgers: 17_280,
                max_refund_bps: 5_000,
                excluded_tags: tags.clone(),
            },
        );
    });

    assert_eq!(
        refund_client.get_global_refund_policy(),
        (17_280u32, 5_000u32, tags)
    );
}
