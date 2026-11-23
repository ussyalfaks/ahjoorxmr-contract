//! Tests for multi-asset escrows: `create_multi_asset_escrow`,
//! `release_multi_asset_escrow`, `refund_multi_asset_escrow`, and
//! `get_multi_asset_escrow`.
//!
//! A multi-asset escrow locks several distinct tokens at once under one
//! buyer/seller agreement. Release and refund settle every asset together
//! (all-or-nothing) — there is no per-asset partial settlement in this flow.

#![cfg(test)]
use super::*;

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

struct Setup<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    token_a: Address,
    token_a_client: TokenClient<'a>,
    token_a_admin: TokenAdminClient<'a>,
    token_b: Address,
    token_b_client: TokenClient<'a>,
    token_b_admin: TokenAdminClient<'a>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_a = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    client.initialize(&admin);

    Setup {
        env,
        client,
        token_a_client: TokenClient::new(&env, &token_a),
        token_a_admin: TokenAdminClient::new(&env, &token_a),
        token_a,
        token_b_client: TokenClient::new(&env, &token_b),
        token_b_admin: TokenAdminClient::new(&env, &token_b),
        token_b,
    }
}

fn assets_vec(env: &Env, pairs: &[(Address, i128)]) -> Vec<AssetAllocation> {
    let mut v = Vec::new(env);
    for (token, amount) in pairs {
        v.push_back(AssetAllocation {
            token: token.clone(),
            amount: *amount,
        });
    }
    v
}

fn sellers_vec(env: &Env, pairs: &[(Address, u32)]) -> Vec<(Address, u32)> {
    let mut v = Vec::new(env);
    for (addr, bps) in pairs {
        v.push_back((addr.clone(), *bps));
    }
    v
}

// ===========================================================================
//  create_multi_asset_escrow
// ===========================================================================

#[test]
fn test_create_pulls_every_asset_from_buyer() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    s.token_b_admin.mint(&buyer, &500);

    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000), (s.token_b.clone(), 500)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    assert_eq!(escrow_id, 0);
    assert_eq!(s.token_a_client.balance(&buyer), 0);
    assert_eq!(s.token_b_client.balance(&buyer), 0);
    assert_eq!(s.token_a_client.balance(&s.client.address), 1_000);
    assert_eq!(s.token_b_client.balance(&s.client.address), 500);

    let escrow = s.client.get_multi_asset_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.assets.len(), 2);
    assert_eq!(escrow.buyer, buyer);
    assert_eq!(escrow.arbiter, arbiter);
}

#[test]
fn test_create_rejects_empty_asset_list() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let result = s.client.try_create_multi_asset_escrow(
        &buyer,
        &Address::generate(&s.env),
        &Vec::new(&s.env),
        &Address::generate(&s.env),
        &Vec::new(&s.env),
        &deadline,
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_create_rejects_non_positive_asset_amount() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 0)]);

    let result = s.client.try_create_multi_asset_escrow(
        &buyer,
        &Address::generate(&s.env),
        &Vec::new(&s.env),
        &Address::generate(&s.env),
        &assets,
        &deadline,
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_create_rejects_duplicate_token_in_asset_list() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    s.token_a_admin.mint(&buyer, &1_000);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 500), (s.token_a.clone(), 500)]);

    let result = s.client.try_create_multi_asset_escrow(
        &buyer,
        &Address::generate(&s.env),
        &Vec::new(&s.env),
        &Address::generate(&s.env),
        &assets,
        &deadline,
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_create_rejects_past_deadline() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    s.token_a_admin.mint(&buyer, &1_000);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000)]);

    let result = s.client.try_create_multi_asset_escrow(
        &buyer,
        &Address::generate(&s.env),
        &Vec::new(&s.env),
        &Address::generate(&s.env),
        &assets,
        &0u64,
        &None,
    );
    assert!(result.is_err());
}

#[test]
fn test_create_rejects_seller_bps_not_summing_to_10000() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    s.token_a_admin.mint(&buyer, &1_000);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000)]);
    let bad_sellers = sellers_vec(&s.env, &[
        (Address::generate(&s.env), 4_000),
        (Address::generate(&s.env), 4_000),
    ]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let result = s.client.try_create_multi_asset_escrow(
        &buyer,
        &Address::generate(&s.env),
        &bad_sellers,
        &Address::generate(&s.env),
        &assets,
        &deadline,
        &None,
    );
    assert!(result.is_err());
}

// ===========================================================================
//  release_multi_asset_escrow
// ===========================================================================

#[test]
fn test_release_transfers_every_asset_to_single_seller() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    s.token_b_admin.mint(&buyer, &500);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000), (s.token_b.clone(), 500)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    s.client.release_multi_asset_escrow(&buyer, &escrow_id);

    assert_eq!(s.token_a_client.balance(&seller), 1_000);
    assert_eq!(s.token_b_client.balance(&seller), 500);
    let escrow = s.client.get_multi_asset_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_release_splits_every_asset_across_sellers_by_bps() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller_a = Address::generate(&s.env);
    let seller_b = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    s.token_b_admin.mint(&buyer, &200);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000), (s.token_b.clone(), 200)]);
    let sellers = sellers_vec(&s.env, &[(seller_a.clone(), 7_000), (seller_b.clone(), 3_000)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller_a,
        &sellers,
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    s.client.release_multi_asset_escrow(&arbiter, &escrow_id);

    assert_eq!(s.token_a_client.balance(&seller_a), 700);
    assert_eq!(s.token_a_client.balance(&seller_b), 300);
    assert_eq!(s.token_b_client.balance(&seller_a), 140);
    assert_eq!(s.token_b_client.balance(&seller_b), 60);
}

#[test]
fn test_release_rejects_caller_other_than_buyer_or_arbiter() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    let result = s.client.try_release_multi_asset_escrow(&stranger, &escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_release_rejects_already_released_escrow() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    s.client.release_multi_asset_escrow(&buyer, &escrow_id);
    let result = s.client.try_release_multi_asset_escrow(&buyer, &escrow_id);
    assert!(result.is_err());
}

// ===========================================================================
//  refund_multi_asset_escrow
// ===========================================================================

#[test]
fn test_arbiter_can_refund_before_deadline() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    s.token_b_admin.mint(&buyer, &500);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000), (s.token_b.clone(), 500)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    s.client.refund_multi_asset_escrow(&arbiter, &escrow_id);

    assert_eq!(s.token_a_client.balance(&buyer), 1_000);
    assert_eq!(s.token_b_client.balance(&buyer), 500);
    let escrow = s.client.get_multi_asset_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
}

#[test]
fn test_buyer_cannot_self_refund_before_deadline() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    let result = s.client.try_refund_multi_asset_escrow(&buyer, &escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_buyer_can_self_refund_after_deadline() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000)]);
    let deadline = s.env.ledger().timestamp() + 1_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    s.env.ledger().with_mut(|l| l.timestamp = deadline + 1);
    s.client.refund_multi_asset_escrow(&buyer, &escrow_id);

    assert_eq!(s.token_a_client.balance(&buyer), 1_000);
    let escrow = s.client.get_multi_asset_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
}

#[test]
fn test_refund_rejects_caller_other_than_buyer_or_arbiter() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);

    s.token_a_admin.mint(&buyer, &1_000);
    let assets = assets_vec(&s.env, &[(s.token_a.clone(), 1_000)]);
    let deadline = s.env.ledger().timestamp() + 10_000;

    let escrow_id = s.client.create_multi_asset_escrow(
        &buyer,
        &seller,
        &Vec::new(&s.env),
        &arbiter,
        &assets,
        &deadline,
        &None,
    );

    let result = s.client.try_refund_multi_asset_escrow(&stranger, &escrow_id);
    assert!(result.is_err());
}
