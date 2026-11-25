#![cfg(test)]
//! `deprecate_token` replacement pointer + `get_replacement_token` view.

use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

use crate::{TokenWhitelistClient, TokenWhitelistContract, TokenWhitelistContractClient};

fn setup(env: &Env) -> (TokenWhitelistContractClient<'static>, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let bridged = Address::generate(env);
    let native = Address::generate(env);
    client.add_token(&admin, &bridged);
    client.add_token(&admin, &native);
    (client, admin, bridged, native)
}

// ── happy path ───────────────────────────────────────────────────────────────

#[test]
fn deprecate_with_replacement_records_it() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &(now + 100), &Some(native.clone()));

    assert_eq!(client.get_replacement_token(&bridged), Some(native.clone()));
    // The replacement itself has no replacement.
    assert_eq!(client.get_replacement_token(&native), None);
}

#[test]
fn deprecate_without_replacement_returns_none() {
    let env = Env::default();
    let (client, admin, bridged, _) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &(now + 100), &None);
    assert_eq!(client.get_replacement_token(&bridged), None);
}

#[test]
fn replacement_is_none_for_unknown_token() {
    let env = Env::default();
    let (client, _, _, _) = setup(&env);
    assert_eq!(client.get_replacement_token(&Address::generate(&env)), None);
}

#[test]
fn replacement_survives_sunset() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &(now + 10), &Some(native.clone()));

    env.ledger().with_mut(|l| l.sequence_number += 10);
    // Triggers the lazy delist.
    assert!(!client.is_whitelisted(&bridged));
    assert!(client.get_token_deprecation(&bridged).is_none());
    // Integrators can still discover where to migrate.
    assert_eq!(client.get_replacement_token(&bridged), Some(native));
}

#[test]
fn undeprecate_clears_replacement() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &(now + 100), &Some(native));
    client.undeprecate_token(&admin, &bridged);
    assert_eq!(client.get_replacement_token(&bridged), None);
}

#[test]
fn readd_after_sunset_clears_replacement() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &(now + 10), &Some(native));
    env.ledger().with_mut(|l| l.sequence_number += 10);
    assert!(!client.is_whitelisted(&bridged));

    client.add_token(&admin, &bridged);
    assert_eq!(client.get_replacement_token(&bridged), None);
}

#[test]
fn cross_contract_client_reads_replacement() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let cross = TokenWhitelistClient::new(&env, &client.address);
    let now = env.ledger().sequence();
    cross.deprecate_token(&admin, &bridged, &(now + 100), &Some(native.clone()));
    assert_eq!(cross.get_replacement_token(&bridged), Some(native));
}

// ── error branches ───────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Replacement cannot be the deprecated token")]
fn replacement_equal_to_token_panics() {
    let env = Env::default();
    let (client, admin, bridged, _) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &(now + 100), &Some(bridged.clone()));
}

#[test]
#[should_panic(expected = "Replacement token not whitelisted")]
fn replacement_not_whitelisted_panics() {
    let env = Env::default();
    let (client, admin, bridged, _) = setup(&env);
    let now = env.ledger().sequence();
    let unlisted = Address::generate(&env);
    client.deprecate_token(&admin, &bridged, &(now + 100), &Some(unlisted));
}

#[test]
#[should_panic(expected = "Replacement token is deprecated")]
fn replacement_already_deprecated_panics() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &native, &(now + 100), &None);
    client.deprecate_token(&admin, &bridged, &(now + 100), &Some(native));
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn non_admin_with_replacement_panics() {
    let env = Env::default();
    let (client, _, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    let stranger = Address::generate(&env);
    client.deprecate_token(&stranger, &bridged, &(now + 100), &Some(native));
}

#[test]
#[should_panic(expected = "Token not whitelisted")]
fn unlisted_token_with_replacement_panics() {
    let env = Env::default();
    let (client, admin, _, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &Address::generate(&env), &(now + 100), &Some(native));
}

#[test]
#[should_panic(expected = "Token already deprecated")]
fn already_deprecated_with_replacement_panics() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &(now + 100), &None);
    client.deprecate_token(&admin, &bridged, &(now + 200), &Some(native));
}

#[test]
#[should_panic(expected = "sunset_ledger must be in the future")]
fn past_sunset_with_replacement_panics() {
    let env = Env::default();
    let (client, admin, bridged, native) = setup(&env);
    let now = env.ledger().sequence();
    client.deprecate_token(&admin, &bridged, &now, &Some(native));
}

#[test]
fn failed_deprecation_leaves_no_replacement() {
    let env = Env::default();
    let (client, admin, bridged, _) = setup(&env);
    let now = env.ledger().sequence();
    let unlisted = Address::generate(&env);
    assert!(client
        .try_deprecate_token(&admin, &bridged, &(now + 100), &Some(unlisted))
        .is_err());
    assert_eq!(client.get_replacement_token(&bridged), None);
    assert!(client.get_token_deprecation(&bridged).is_none());
}
