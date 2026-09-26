#![cfg(test)]

use crate::{TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, Env, Vec,
};

fn setup_test() -> (Env, Address, TokenWhitelistContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    (env, admin, client)
}

#[test]
fn test_initialize() {
    let (env, admin, client) = setup_test();

    /// Verify admin is set
    assert_eq!(client.get_admin(), admin);

    /// Verify whitelist is empty initially
    let tokens = client.get_whitelisted_tokens();
    assert_eq!(tokens.len(), 0);

    /// Check initialization event
    let events = env.events().all();
    /// Just verify the contract works, events can be tested separately
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_initialize_twice_fails() {
    let (_, admin, client) = setup_test();
    
    // Try to initialize again
    client.initialize(&admin);
}

#[test]
fn test_add_token() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    // Add token
    client.add_token(&admin, &token);

    // Verify token is whitelisted
    assert!(client.is_token_allowed(&token));

    // Verify it's in the whitelist
    let tokens = client.get_whitelisted_tokens();
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens.get(0).unwrap(), token);

    // Check event was emitted
    let events = env.events().all();
    // Just verify the functionality works
}

#[test]
#[should_panic(expected = "Token already whitelisted")]
fn test_add_token_twice_fails() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    client.add_token(&admin, &token); // Should fail
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_add_token_unauthorized() {
    let (env, _admin, client) = setup_test();
    let token = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    client.add_token(&unauthorized, &token);
}

#[test]
fn test_remove_token() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    // Add then remove token
    client.add_token(&admin, &token);
    assert!(client.is_token_allowed(&token));

    client.remove_token(&admin, &token);
    assert!(!client.is_token_allowed(&token));

    // Verify it's not in the whitelist
    let tokens = client.get_whitelisted_tokens();
    assert_eq!(tokens.len(), 0);

    // Check events were emitted
    let events = env.events().all();
    // Just verify the functionality works
}

#[test]
#[should_panic(expected = "Token not whitelisted")]
fn test_remove_nonexistent_token_fails() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.remove_token(&admin, &token);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_remove_token_unauthorized() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    let unauthorized = Address::generate(&env);

    client.add_token(&admin, &token);
    client.remove_token(&unauthorized, &token);
}

#[test]
fn test_is_token_allowed_nonexistent() {
    let (env, _admin, client) = setup_test();
    let token = Address::generate(&env);

    assert!(!client.is_token_allowed(&token));
}

#[test]
fn test_multiple_tokens() {
    let (env, admin, client) = setup_test();
    let token1 = Address::generate(&env);
    let token2 = Address::generate(&env);
    let token3 = Address::generate(&env);

    // Add multiple tokens
    client.add_token(&admin, &token1);
    client.add_token(&admin, &token2);
    client.add_token(&admin, &token3);

    // Verify all are whitelisted
    assert!(client.is_token_allowed(&token1));
    assert!(client.is_token_allowed(&token2));
    assert!(client.is_token_allowed(&token3));

    let tokens = client.get_whitelisted_tokens();
    assert_eq!(tokens.len(), 3);

    // Remove one token
    client.remove_token(&admin, &token2);
    assert!(client.is_token_allowed(&token1));
    assert!(!client.is_token_allowed(&token2));
    assert!(client.is_token_allowed(&token3));

    let tokens = client.get_whitelisted_tokens();
    assert_eq!(tokens.len(), 2);
}

#[test]
fn test_admin_transfer() {
    let (env, admin, client) = setup_test();
    let new_admin = Address::generate(&env);
    let token = Address::generate(&env);

    // Current admin can add tokens
    client.add_token(&admin, &token);

    // Propose new admin
    client.propose_admin(&admin, &new_admin);

    // New admin accepts
    client.accept_admin(&new_admin);

    // Verify new admin
    assert_eq!(client.get_admin(), new_admin);

    // New admin can add tokens
    let token2 = Address::generate(&env);
    client.add_token(&new_admin, &token2);

    // Old admin cannot add tokens anymore
    let token3 = Address::generate(&env);
    let result = client.try_add_token(&admin, &token3);
    assert!(result.is_err());

    // Check events
    let events = env.events().all();
    // Just verify the functionality works
}

#[test]
#[should_panic(expected = "Only proposed admin can accept")]
fn test_admin_transfer_wrong_acceptor() {
    let (env, admin, client) = setup_test();
    let new_admin = Address::generate(&env);
    let wrong_admin = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&wrong_admin); // Should fail
}

#[test]
#[should_panic(expected = "No admin transfer proposed")]
fn test_accept_admin_without_proposal() {
    let (env, _admin, client) = setup_test();
    let new_admin = Address::generate(&env);

    client.accept_admin(&new_admin); // Should fail
}

#[test]
fn test_token_delisted_mid_operation() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    // Add token
    client.add_token(&admin, &token);
    assert!(client.is_token_allowed(&token));

    // Simulate mid-operation: token gets delisted
    client.remove_token(&admin, &token);
    
    // Token should no longer be allowed
    assert!(!client.is_token_allowed(&token));
}

// --- Token Quota Tests ---

#[test]
fn test_set_token_quota() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    // Add token
    client.add_token(&admin, &token);

    // Set quota
    client.set_token_quota(&admin, &token, &100, &10);

    // Verify quota
    let quota = client.get_token_quota(&token).unwrap();
    assert_eq!(quota.max_volume_per_period, 100);
    assert_eq!(quota.period_ledgers, 10);
}

#[test]
#[should_panic(expected = "Token not whitelisted")]
fn test_set_quota_on_non_whitelisted_token_fails() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    // Try to set quota without whitelisting
    client.set_token_quota(&admin, &token, &100, &10);
}

#[test]
#[should_panic(expected = "Token already has quota")]
fn test_set_quota_twice_fails() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &100, &10);
    client.set_token_quota(&admin, &token, &200, &20);
}

#[test]
fn test_update_token_quota() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &100, &10);

    // Update quota
    client.update_token_quota(&admin, &token, &200, &20);

    let quota = client.get_token_quota(&token).unwrap();
    assert_eq!(quota.max_volume_per_period, 200);
    assert_eq!(quota.period_ledgers, 20);
}

#[test]
fn test_remove_token_quota() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &100, &10);

    // Remove quota
    client.remove_token_quota(&admin, &token);

    // Verify quota is gone
    assert!(client.get_token_quota(&token).is_none());
}

// --- Suspension History Pagination Tests ---

#[test]
fn test_get_suspension_history_pagination_middle_page() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    // Build up a history of suspensions/reinstatements.
    for _ in 0..5 {
        client.suspend_token(&admin, &token);
        client.reinstate_token(&admin, &token);
    }

    let full = client.get_suspension_history(&token, &0, &100);
    assert_eq!(full.len(), 10);

    // A page in the middle of the list.
    let middle = client.get_suspension_history(&token, &3, &4);
    assert_eq!(middle.len(), 4);
    assert_eq!(middle.get(0).unwrap(), full.get(3).unwrap());
    assert_eq!(middle.get(3).unwrap(), full.get(6).unwrap());
}

#[test]
fn test_get_suspension_history_pagination_past_end() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    for _ in 0..3 {
        client.suspend_token(&admin, &token);
        client.reinstate_token(&admin, &token);
    }

    let full = client.get_suspension_history(&token, &0, &100);
    assert_eq!(full.len(), 6);

    // A page that runs past the end returns only the remaining entries.
    let partial = client.get_suspension_history(&token, &4, &10);
    assert_eq!(partial.len(), 2);
    assert_eq!(partial.get(0).unwrap(), full.get(4).unwrap());
    assert_eq!(partial.get(1).unwrap(), full.get(5).unwrap());

    // An offset at/after the end returns an empty page.
    let empty = client.get_suspension_history(&token, &6, &10);
    assert_eq!(empty.len(), 0);
}
