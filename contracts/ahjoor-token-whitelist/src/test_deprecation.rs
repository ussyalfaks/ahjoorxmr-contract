#![cfg(test)]
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

use crate::{TokenWhitelistContract, TokenWhitelistContractClient};

fn setup(env: &Env) -> (TokenWhitelistContractClient<'static>, Address) {
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    (client, admin)
}

#[test]
fn test_deprecate_token_records_state() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 100));

    let dep = client.get_token_deprecation(&token).unwrap();
    assert_eq!(dep.deprecated_at_ledger, now);
    assert_eq!(dep.sunset_ledger, now + 100);
}

#[test]
fn test_not_deprecated_token_allowed_for_new() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    assert!(client.is_token_allowed_for_new(&token));
    assert!(client.get_token_deprecation(&token).is_none());
}

#[test]
fn test_pre_sunset_rejected_for_new_but_allowed_for_existing() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 100));

    env.ledger().with_mut(|l| l.sequence_number += 50);

    assert!(!client.is_token_allowed_for_new(&token));
    assert!(client.is_token_allowed(&token));
    assert!(client.is_whitelisted(&token));
}

#[test]
fn test_post_sunset_both_checks_return_false() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 100));

    env.ledger().with_mut(|l| l.sequence_number += 100);

    assert!(!client.is_token_allowed(&token));
    assert!(!client.is_token_allowed_for_new(&token));
    assert!(!client.is_whitelisted(&token));
    // Lazily delisted: removed from enumeration and deprecation cleared.
    assert_eq!(client.get_whitelisted_tokens(&0, &50).len(), 0);
    assert!(client.get_token_deprecation(&token).is_none());
}

#[test]
fn test_is_whitelisted_applies_sunset() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 10));
    env.ledger().with_mut(|l| l.sequence_number += 20);

    assert!(!client.is_whitelisted(&token));
    assert!(!client.is_token_allowed(&token));
}

#[test]
fn test_undeprecate_restores_full_status() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 100));
    assert!(!client.is_token_allowed_for_new(&token));

    client.undeprecate_token(&admin, &token);
    assert!(client.get_token_deprecation(&token).is_none());
    assert!(client.is_token_allowed_for_new(&token));

    // Passing the old sunset ledger no longer delists the token.
    env.ledger().with_mut(|l| l.sequence_number += 200);
    assert!(client.is_token_allowed(&token));
    assert!(client.is_whitelisted(&token));
}

#[test]
fn test_readd_after_sunset_starts_clean() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 10));
    env.ledger().with_mut(|l| l.sequence_number += 20);
    assert!(!client.is_token_allowed(&token));

    client.add_token(&admin, &token);
    assert!(client.is_token_allowed_for_new(&token));
}

#[test]
#[should_panic(expected = "Token already sunset")]
fn test_undeprecate_after_sunset_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 10));
    env.ledger().with_mut(|l| l.sequence_number += 20);
    client.undeprecate_token(&admin, &token);
}

#[test]
#[should_panic(expected = "Token not deprecated")]
fn test_undeprecate_not_deprecated_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.undeprecate_token(&admin, &token);
}

#[test]
#[should_panic(expected = "Token already deprecated")]
fn test_double_deprecate_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &(now + 100));
    client.deprecate_token(&admin, &token, &(now + 200));
}

#[test]
#[should_panic(expected = "sunset_ledger must be in the future")]
fn test_deprecate_with_past_sunset_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &token, &now);
}

#[test]
#[should_panic(expected = "Token not whitelisted")]
fn test_deprecate_unlisted_token_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.deprecate_token(&admin, &token, &1000);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_deprecate_non_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    let stranger = Address::generate(&env);
    client.deprecate_token(&stranger, &token, &1000);
}

/// Consuming contracts call the deprecation checks through the hand-written
/// `TokenWhitelistClient`; exercise it against the live contract so the
/// interface cannot drift from the real entry points.
#[test]
fn test_cross_contract_client_deprecation_checks() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let cross = crate::TokenWhitelistClient::new(&env, &client.address);
    assert!(cross.is_token_allowed_for_new(&token));

    let now = env.ledger().sequence();
    cross.deprecate_token(&admin, &token, &(now + 100));
    assert!(!cross.is_token_allowed_for_new(&token));
    assert!(cross.is_token_allowed(&token));
    assert_eq!(cross.get_token_deprecation(&token).unwrap().sunset_ledger, now + 100);

    cross.undeprecate_token(&admin, &token);
    assert!(cross.is_token_allowed_for_new(&token));
}

#[test]
fn test_sunset_still_applies_after_other_deprecations_resolve() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    let (a, b, c) = (Address::generate(&env), Address::generate(&env), Address::generate(&env));
    client.add_token(&admin, &a);
    client.add_token(&admin, &b);
    client.add_token(&admin, &c);

    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &a, &(now + 10));
    client.deprecate_token(&admin, &b, &(now + 10));
    client.deprecate_token(&admin, &c, &(now + 10));
    // Resolve two deprecations through different paths.
    client.undeprecate_token(&admin, &a);
    client.remove_token(&admin, &b);

    env.ledger().with_mut(|l| l.sequence_number = now + 10);
    assert!(client.is_whitelisted(&a));
    assert!(!client.is_whitelisted(&c));
}
