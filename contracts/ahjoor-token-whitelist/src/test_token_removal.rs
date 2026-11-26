#![cfg(test)]
//! #1216: Cover executing token removal before and after the timelock, and
//! cancellation; plus category assignment, removal and counts.
//!
//! All entry points exercised here are admin-gated ledger writes against a live
//! contract. There are no `try_*` variants on this client, so rejected
//! admin calls are asserted with `s.client.` (which panics) and success is
//! followed by a read-only getter.

use crate::{TokenCategory, TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env, Vec};

struct TokenRemovalSetup {
    env: Env,
    client: TokenWhitelistContractClient<'static>,
    admin: Address,
}

fn setup() -> TokenRemovalSetup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    TokenRemovalSetup { env, client, admin }
}

fn add_token(s: &TokenRemovalSetup, token: &Address) {
    s.client.add_token(&s.admin, token); // token is already a reference; pass it through
}

fn reason_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

// ── scheduled removal ────────────────────────────────────────────────────────

#[test]
fn test_schedule_token_removal_stores_removal_ledger_and_is_cancellable_before() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    let scheduled_at = s.env.ledger().sequence();
    let delay = 10u32;
    s.client
        .schedule_token_removal(&s.admin, &token, &delay, &reason_hash(&s.env));

    let req = s.client.get_token_removal_request(&token).unwrap();
    assert_eq!(req.token, token);
    assert_eq!(req.requester, s.admin);
    assert_eq!(req.reason_hash, reason_hash(&s.env));
    assert_eq!(req.scheduled_at_ledger, scheduled_at);
    assert_eq!(req.removal_ledger, scheduled_at + delay);

    // Still before the timelock: cancellable.
    s.client.cancel_token_removal(&s.admin, &token);
    assert!(s.client.get_token_removal_request(&token).is_none());
}

#[test]
fn test_cancel_token_removal_before_timelock_succeeds() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    let start = s.env.ledger().sequence();
    s.client
        .schedule_token_removal(&s.admin, &token, &(start + 50u32), &reason_hash(&s.env));

    // Advance inside the window so the request is still pending.
    s.env.ledger().with_mut(|l| l.sequence_number = start + 25);
    assert!(s.client.get_token_removal_request(&token).is_some());

    s.client.cancel_token_removal(&s.admin, &token);

    assert!(s.client.get_token_removal_request(&token).is_none());
}

#[test]
#[should_panic(expected = "Removal already active")]
fn test_cancel_token_removal_after_timelock_panics() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    let start = s.env.ledger().sequence();
    s.client
        .schedule_token_removal(&s.admin, &token, &(start + 50u32), &reason_hash(&s.env));

    // Advance past the removal ledger: cancellation must be rejected.
    s.env.ledger().with_mut(|l| l.sequence_number = start + 50);

    s.client.cancel_token_removal(&s.admin, &token);
}

#[test]
fn test_execute_token_removal_after_timelock_removes_token() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    let start = s.env.ledger().sequence();
    s.client
        .schedule_token_removal(&s.admin, &token, &(start + 50u32), &reason_hash(&s.env));

    // Advance past the removal ledger.
    s.env.ledger().with_mut(|l| l.sequence_number = start + 50);

    // Permissionless: anyone may execute now.
    s.client.execute_token_removal(&token);

    assert!(!s.client.is_token_allowed(&token));
    assert!(s.client.get_token_removal_request(&token).is_none());
}

#[test]
#[should_panic(expected = "Removal not yet active")]
fn test_execute_token_removal_before_timelock_panics() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    let start = s.env.ledger().sequence();
    s.client
        .schedule_token_removal(&s.admin, &token, &(start + 50u32), &reason_hash(&s.env));

    // Still inside the window: execution must be rejected.
    s.env.ledger().with_mut(|l| l.sequence_number = start + 10);

    s.client.execute_token_removal(&token);
}

#[test]
fn test_second_schedule_after_execution_is_allowed() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    let start = s.env.ledger().sequence();
    s.client
        .schedule_token_removal(&s.admin, &token, &(start + 10u32), &reason_hash(&s.env));

    // Deliver the timelock locally and execute so the request is gone.
    s.env.ledger().with_mut(|l| l.sequence_number = start + 10);
    s.client.execute_token_removal(&token);

    // A fresh request can be scheduled for the same token afterwards.
    let new_token = Address::generate(&s.env);
    add_token(&s, &new_token);
    s.client
        .schedule_token_removal(&s.admin, &new_token, &0u32, &reason_hash(&s.env));
    assert!(s.client.get_token_removal_request(&new_token).is_some());
}

// ── category assignment and removal ──────────────────────────────────────────

#[test]
fn test_assign_token_category_sets_category_and_increments_count() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    let category = TokenCategory::Stablecoin;
    s.client.assign_token_category(&s.admin, &token, &category);

    assert_eq!(s.client.get_token_category(&token), Some(category));
    assert_eq!(s.client.get_category_token_count(&category), 1);
}

#[test]
fn test_assign_token_category_twice_changes_category_and_keeps_count() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    s.client.assign_token_category(&s.admin, &token, &TokenCategory::Stablecoin);
    let stablecoin_before = s.client.get_category_token_count(&TokenCategory::Stablecoin);

    s.client.assign_token_category(&s.admin, &token, &TokenCategory::Governance);
    let governance_after = s.client.get_category_token_count(&TokenCategory::Governance);

    assert_eq!(s.client.get_token_category(&token), Some(TokenCategory::Governance));
    assert_eq!(s.client.get_category_token_count(&TokenCategory::Stablecoin), 0);
    assert_eq!(governance_after, stablecoin_before);
}

#[test]
#[should_panic(expected = "Token not whitelisted")]
fn test_assign_token_category_to_unlisted_token_panics() {
    let s = setup();
    let token = Address::generate(&s.env);

    s.client.assign_token_category(&s.admin, &token, &TokenCategory::Stablecoin);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_assign_token_category_non_admin_panics() {
    let s = setup();
    let token = Address::generate(&s.env);
    let other = Address::generate(&s.env);

    add_token(&s, &token);

    s.client.assign_token_category(&other, &token, &TokenCategory::Stablecoin);
}

#[test]
fn test_remove_token_category_clears_assignment_and_decrements_count() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);
    s.client.assign_token_category(&s.admin, &token, &TokenCategory::Utility);

    assert_eq!(s.client.get_token_category(&token), Some(TokenCategory::Utility));
    assert_eq!(s.client.get_category_token_count(&TokenCategory::Utility), 1);

    s.client.remove_token_category(&s.admin, &token);

    assert!(s.client.get_token_category(&token).is_none());
    assert_eq!(s.client.get_category_token_count(&TokenCategory::Utility), 0);
}

#[test]
#[should_panic(expected = "Token has no category assigned")]
fn test_remove_token_category_without_assignment_panics() {
    let s = setup();
    let token = Address::generate(&s.env);

    add_token(&s, &token);

    s.client.remove_token_category(&s.admin, &token);
}

#[test]
fn test_get_tokens_by_category_returns_only_tokens_in_that_category() {
    let s = setup();
    let mut tokens = Vec::new(&s.env);
    tokens.push_back(Address::generate(&s.env));
    tokens.push_back(Address::generate(&s.env));
    tokens.push_back(Address::generate(&s.env));

    for token in tokens.iter() {
        add_token(&s, &token);
        s.client.assign_token_category(&s.admin, &token, &TokenCategory::Stablecoin);
    }

    let other = Address::generate(&s.env);
    add_token(&s, &other);
    s.client.assign_token_category(&s.admin, &other, &TokenCategory::Governance);

    let mut listed = s.client.get_tokens_by_category(&TokenCategory::Stablecoin, &0, &50);
    assert_eq!(listed.len(), 3);

    let mut all = s.client.get_whitelisted_tokens(&0, &50);
    assert_eq!(all.len(), 4);
}

#[test]
fn test_get_tokens_by_category_empty_returns_empty_when_no_tokens() {
    let s = setup();
    let empty = s.client.get_tokens_by_category(&TokenCategory::Stablecoin, &0, &50);
    assert_eq!(empty.len(), 0);
}

#[test]
fn test_get_category_token_count_returns_zero_when_empty() {
    let s = setup();
    assert_eq!(s.client.get_category_token_count(&TokenCategory::Stablecoin), 0);
}

#[test]
fn test_get_tokens_by_category_respects_offset_and_limit() {
    let s = setup();
    for _ in 0..25 {
        let tok = Address::generate(&s.env);
        add_token(&s, &tok);
        s.client.assign_token_category(&s.admin, &tok, &TokenCategory::Stablecoin);
    }

    let page0 = s.client.get_tokens_by_category(&TokenCategory::Stablecoin, &0, &10);
    assert_eq!(page0.len(), 10);

    let page1 = s.client.get_tokens_by_category(&TokenCategory::Stablecoin, &10, &10);
    assert_eq!(page1.len(), 10);

    let page2 = s.client.get_tokens_by_category(&TokenCategory::Stablecoin, &20, &10);
    assert_eq!(page2.len(), 5);
}

