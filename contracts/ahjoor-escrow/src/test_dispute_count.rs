#![cfg(test)]
use crate::{AhjoorEscrowContract, AhjoorEscrowContractClient, EscrowCreateRequest};
use soroban_sdk::{testutils::Address as _, token, Address, Env, String};

/// #578: Test that the active dispute count is correctly maintained
/// as escrows enter and exit disputed states.
#[test]
fn test_active_dispute_count() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer1 = Address::generate(&env);
    let buyer2 = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Create token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(&env, &token_contract);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract);

    // Mint tokens
    token_admin_client.mint(&buyer1, &10_000);
    token_admin_client.mint(&buyer2, &10_000);

    // Initial count should be 0
    assert_eq!(client.get_active_dispute_count(), 0);

    // Create first escrow
    let escrow1 = client.create_escrow_v2(
        &buyer1,
        &EscrowCreateRequest {
            seller: seller.clone(),
            arbiter: arbiter.clone(),
            amount: 1000,
            token: token_contract.clone(),
            deadline: env.ledger().timestamp() + 1000,
            metadata_hash: None,
            sellers: soroban_sdk::vec![&env, (seller.clone(), 10_000u32)],
            auto_renew: false,
            renewal_count: 0,
            buyer_inactivity_secs: 0,
            min_lock_until: None,
            release_base: None,
            release_quote: None,
            release_comparison: None,
            release_threshold_price: None,
            arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,
            auto_renew_interval_ledgers: None,
        },
    );

    // Create second escrow
    let escrow2 = client.create_escrow_v2(
        &buyer2,
        &EscrowCreateRequest {
            seller: seller.clone(),
            arbiter: arbiter.clone(),
            amount: 2000,
            token: token_contract.clone(),
            deadline: env.ledger().timestamp() + 1000,
            metadata_hash: None,
            sellers: soroban_sdk::vec![&env, (seller.clone(), 10_000u32)],
            auto_renew: false,
            renewal_count: 0,
            buyer_inactivity_secs: 0,
            min_lock_until: None,
            release_base: None,
            release_quote: None,
            release_comparison: None,
            release_threshold_price: None,
            arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,
            auto_renew_interval_ledgers: None,
        },
    );

    // Still 0 - escrows are Active
    assert_eq!(client.get_active_dispute_count(), 0);

    // Dispute first escrow (full dispute)
    client.dispute_escrow(&buyer1, &escrow1, &String::from_str(&env, "Issue"), &1000);
    assert_eq!(client.get_active_dispute_count(), 1);

    // Dispute second escrow (partial dispute)
    client.dispute_escrow(&seller, &escrow2, &String::from_str(&env, "Problem"), &1000);
    assert_eq!(client.get_active_dispute_count(), 2);

    // Resolve first dispute (buyer wins 100%)
    client.resolve_dispute(&arbiter, &escrow1, &100);
    assert_eq!(client.get_active_dispute_count(), 1);

    // Resolve second dispute (50/50 split)
    client.resolve_dispute(&arbiter, &escrow2, &50);
    assert_eq!(client.get_active_dispute_count(), 0);
}

#[test]
fn test_dispute_count_with_seller_transfer_veto() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let new_seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Create token
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract(token_admin.clone());
    let token_client = token::Client::new(&env, &token_contract);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_contract);

    token_admin_client.mint(&buyer, &5_000);

    // Initial count should be 0
    assert_eq!(client.get_active_dispute_count(), 0);

    // Create escrow
    let escrow_id = client.create_escrow_v2(
        &buyer,
        &EscrowCreateRequest {
            seller: seller.clone(),
            arbiter: arbiter.clone(),
            amount: 1000,
            token: token_contract.clone(),
            deadline: env.ledger().timestamp() + 1000,
            metadata_hash: None,
            sellers: soroban_sdk::vec![&env, (seller.clone(), 10_000u32)],
            auto_renew: false,
            renewal_count: 0,
            buyer_inactivity_secs: 0,
            min_lock_until: None,
            release_base: None,
            release_quote: None,
            release_comparison: None,
            release_threshold_price: None,
            arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,
            auto_renew_interval_ledgers: None,
        },
    );

    assert_eq!(client.get_active_dispute_count(), 0);

    // Seller initiates transfer - should increment dispute count
    client.transfer_seller_role(&seller, &escrow_id, &new_seller);
    assert_eq!(client.get_active_dispute_count(), 1);

    // Buyer approves transfer - should decrement dispute count
    client.approve_seller_transfer(&buyer, &escrow_id);
    assert_eq!(client.get_active_dispute_count(), 0);
}
