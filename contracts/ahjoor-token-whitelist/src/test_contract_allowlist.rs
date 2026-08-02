#![cfg(test)]

//! Tests for the contract-level token allowlist feature.
//!
//! Acceptance criteria covered:
//! - Contract-level allowlist takes precedence over global whitelist
//! - Expired contract-level entries fall back to global whitelist
//! - Only the whitelist admin can modify contract allowlists
//! - Removing a token from the contract list does not affect the global whitelist
//! - Permanent (None expiry) vs time-bounded (Some(n)) approvals
//! - get_contract_token_entry query returns the stored entry correctly

use crate::{TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn setup() -> (Env, Address, TokenWhitelistContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    (env, admin, client)
}

/// Advance the ledger sequence by `n`.
fn advance_ledger(env: &Env, n: u32) {
    env.ledger().with_mut(|l| l.sequence_number += n);
}

// ─── Precedence: contract-level overrides global ──────────────────────────────

/// A token that is NOT on the global whitelist can still be allowed if the
/// contract-level allowlist has a permanent entry for it.
#[test]
fn test_contract_level_allows_non_globally_whitelisted_token() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    // Token is not in the global whitelist
    assert!(!client.is_token_allowed(&token));

    // Add a permanent contract-level entry
    client.set_contract_token(&admin, &contract_id, &token, &None);

    // Now it is allowed for that specific contract
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
    // But still not allowed globally
    assert!(!client.is_token_allowed(&token));
}

/// A token on the global whitelist remains allowed for a contract that has no
/// contract-level entry (pure fallback path).
#[test]
fn test_global_whitelist_fallback_when_no_contract_entry() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    // Add to global whitelist
    client.add_token(&admin, &token);

    // Contract has no specific entry — falls back to global: allowed
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
}

/// A token on the global whitelist is blocked for a contract that has a
/// permanent contract-level entry for a *different* token only — the absent
/// token still falls back to global (allowed).
#[test]
fn test_absent_contract_entry_falls_back_to_global() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    client.add_token(&admin, &token_a);
    client.add_token(&admin, &token_b);
    // Add contract-level entry only for token_a
    client.set_contract_token(&admin, &contract_id, &token_a, &None);

    // token_b has no contract-level entry → falls back to global → allowed
    assert!(client.is_token_allowed_for_contract(&contract_id, &token_b));
    // token_a has explicit contract-level entry → allowed (permanent)
    assert!(client.is_token_allowed_for_contract(&contract_id, &token_a));
}

// ─── Time-bounded approvals ───────────────────────────────────────────────────

/// A time-bounded contract-level entry is accepted before expiry and falls
/// back to global after expiry.
#[test]
fn test_time_bounded_entry_allowed_before_expiry() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    let current = env.ledger().sequence();
    let expiry = current + 100;

    // Set a time-bounded entry; token is NOT globally whitelisted
    client.set_contract_token(&admin, &contract_id, &token, &Some(expiry));

    // Before expiry: allowed via contract-level entry
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
}

/// After the time-bounded entry expires the check falls back to the global
/// whitelist. Since the token is also not globally whitelisted it returns false.
#[test]
fn test_time_bounded_entry_denied_after_expiry_no_global() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    let current = env.ledger().sequence();
    let expiry = current + 5;

    client.set_contract_token(&admin, &contract_id, &token, &Some(expiry));
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));

    // Advance past expiry
    advance_ledger(&env, 10);

    // Expired → falls back to global; token not globally whitelisted → false
    assert!(!client.is_token_allowed_for_contract(&contract_id, &token));
}

/// After the time-bounded entry expires, if the token IS on the global
/// whitelist, fallback allows it.
#[test]
fn test_time_bounded_entry_falls_back_to_global_after_expiry() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    // Add to global whitelist
    client.add_token(&admin, &token);

    let expiry = env.ledger().sequence() + 5;
    client.set_contract_token(&admin, &contract_id, &token, &Some(expiry));

    advance_ledger(&env, 10);

    // Expired contract-level entry → falls back to global → still allowed
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
}

/// A permanent entry (None expiry) never expires.
#[test]
fn test_permanent_entry_never_expires() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    client.set_contract_token(&admin, &contract_id, &token, &None);

    // Advance many ledgers
    advance_ledger(&env, 1_000_000);

    // Still allowed
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
}

/// Updating an entry from time-bounded to permanent upgrades it correctly.
#[test]
fn test_update_entry_from_time_bounded_to_permanent() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    let expiry = env.ledger().sequence() + 5;
    client.set_contract_token(&admin, &contract_id, &token, &Some(expiry));

    // Overwrite with permanent entry before expiry
    client.set_contract_token(&admin, &contract_id, &token, &None);

    advance_ledger(&env, 10);

    // Now permanent → still allowed after original expiry
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
}

// ─── Removal ─────────────────────────────────────────────────────────────────

/// Removing a contract-level entry falls back to global (allowed if globally
/// whitelisted, denied otherwise).
#[test]
fn test_remove_contract_entry_falls_back_to_global() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    // No global listing; add contract-level entry
    client.set_contract_token(&admin, &contract_id, &token, &None);
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));

    // Remove contract-level entry
    client.remove_contract_token(&admin, &contract_id, &token);

    // Falls back to global; token not globally whitelisted → denied
    assert!(!client.is_token_allowed_for_contract(&contract_id, &token));
}

/// Removing a contract-level entry does NOT remove the token from the global
/// whitelist.
#[test]
fn test_remove_contract_entry_does_not_affect_global_whitelist() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    // Add to global whitelist AND contract-level allowlist
    client.add_token(&admin, &token);
    client.set_contract_token(&admin, &contract_id, &token, &None);

    // Remove only the contract-level entry
    client.remove_contract_token(&admin, &contract_id, &token);

    // Global whitelist still has the token
    assert!(client.is_token_allowed(&token));

    // Contract now uses global fallback → still allowed
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
}

/// Removing a globally whitelisted token does not affect a contract-level
/// entry for a different contract.
#[test]
fn test_global_removal_does_not_affect_other_contract_entries() {
    let (env, admin, client) = setup();
    let contract_a = Address::generate(&env);
    let contract_b = Address::generate(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    client.set_contract_token(&admin, &contract_a, &token, &None);

    // Remove from global whitelist
    client.remove_token(&admin, &token);

    // contract_a still has a permanent entry → allowed
    assert!(client.is_token_allowed_for_contract(&contract_a, &token));
    // contract_b has no entry → falls back to global → not allowed
    assert!(!client.is_token_allowed_for_contract(&contract_b, &token));
}

// ─── Admin-only enforcement ───────────────────────────────────────────────────

/// A non-admin address cannot set a contract-level allowlist entry.
#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_set_contract_token_unauthorized() {
    let (env, _admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);
    let attacker = Address::generate(&env);

    // attacker is not the admin
    client.set_contract_token(&attacker, &contract_id, &token, &None);
}

/// A non-admin address cannot remove a contract-level allowlist entry.
#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_remove_contract_token_unauthorized() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);
    let attacker = Address::generate(&env);

    // Admin sets the entry first
    client.set_contract_token(&admin, &contract_id, &token, &None);

    // Attacker tries to remove it
    client.remove_contract_token(&attacker, &contract_id, &token);
}

/// The new admin (after a two-step transfer) can manage contract allowlists,
/// but the old admin can no longer do so.
#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_only_current_admin_can_manage_after_transfer() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    // Transfer admin
    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    // Old admin can no longer set contract tokens
    client.set_contract_token(&admin, &contract_id, &token, &None);
}

// ─── get_contract_token_entry query ──────────────────────────────────────────

/// Returns None when no entry exists.
#[test]
fn test_get_contract_token_entry_none_when_absent() {
    let (env, _admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    assert_eq!(client.get_contract_token_entry(&contract_id, &token), None);
}

/// Returns Some(None) for a permanent entry.
#[test]
fn test_get_contract_token_entry_permanent() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    client.set_contract_token(&admin, &contract_id, &token, &None);
    assert_eq!(
        client.get_contract_token_entry(&contract_id, &token),
        Some(None)
    );
}

/// Returns Some(Some(n)) for a time-bounded entry.
#[test]
fn test_get_contract_token_entry_time_bounded() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    let expiry = env.ledger().sequence() + 200;
    client.set_contract_token(&admin, &contract_id, &token, &Some(expiry));
    assert_eq!(
        client.get_contract_token_entry(&contract_id, &token),
        Some(Some(expiry))
    );
}

/// After removal, get_contract_token_entry returns None again.
#[test]
fn test_get_contract_token_entry_cleared_after_removal() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    client.set_contract_token(&admin, &contract_id, &token, &None);
    assert!(client.get_contract_token_entry(&contract_id, &token).is_some());

    client.remove_contract_token(&admin, &contract_id, &token);
    assert_eq!(client.get_contract_token_entry(&contract_id, &token), None);
}

// ─── Per-contract isolation ───────────────────────────────────────────────────

/// Each contract has an independent allowlist; one contract's entry does not
/// affect another.
#[test]
fn test_contract_allowlists_are_isolated() {
    let (env, admin, client) = setup();
    let contract_a = Address::generate(&env);
    let contract_b = Address::generate(&env);
    let token = Address::generate(&env);

    // Only allow for contract_a
    client.set_contract_token(&admin, &contract_a, &token, &None);

    // contract_a: allowed; contract_b: denied (no entry, no global)
    assert!(client.is_token_allowed_for_contract(&contract_a, &token));
    assert!(!client.is_token_allowed_for_contract(&contract_b, &token));
}

/// Two contracts can have different expiry times for the same token.
#[test]
fn test_contracts_can_have_different_expiries_for_same_token() {
    let (env, admin, client) = setup();
    let contract_a = Address::generate(&env);
    let contract_b = Address::generate(&env);
    let token = Address::generate(&env);

    let base = env.ledger().sequence();
    client.set_contract_token(&admin, &contract_a, &token, &Some(base + 5));
    client.set_contract_token(&admin, &contract_b, &token, &Some(base + 100));

    // Advance past contract_a's expiry but not contract_b's
    advance_ledger(&env, 10);

    // contract_a: expired → fallback to global (not listed) → denied
    assert!(!client.is_token_allowed_for_contract(&contract_a, &token));
    // contract_b: still valid → allowed
    assert!(client.is_token_allowed_for_contract(&contract_b, &token));
}

// ─── Interaction with global suspension ──────────────────────────────────────

/// A contract-level entry does NOT bypass a global suspension: the fallback
/// path honours suspensions, but a contract-level entry bypasses the global
/// check entirely, so a suspended token with a contract-level entry is still
/// reachable *only* through the contract-level path, not through global check.
///
/// Specifically: `is_token_allowed_for_contract` returning true for a contract
/// entry means the *contract* approved it independently of global status.
/// The global `is_token_allowed` still returns false while suspended.
#[test]
fn test_contract_entry_allows_even_if_globally_suspended() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    // Add to global and suspend it
    client.add_token(&admin, &token);
    client.suspend_token_timed(
        &admin,
        &token,
        &1000u32,
        &soroban_sdk::BytesN::from_array(&env, &[1u8; 32]),
    );
    // Globally suspended → is_token_allowed returns false
    assert!(!client.is_token_allowed(&token));

    // Add a contract-level entry
    client.set_contract_token(&admin, &contract_id, &token, &None);

    // Contract-level entry takes precedence; returns true without touching global
    assert!(client.is_token_allowed_for_contract(&contract_id, &token));
    // Global check is unaffected (still suspended)
    assert!(!client.is_token_allowed(&token));
}

/// When the contract-level entry is absent, a suspended global token is denied
/// even via the contract path.
#[test]
fn test_suspended_token_denied_via_contract_path_without_contract_entry() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    client.suspend_token_timed(
        &admin,
        &token,
        &1000u32,
        &soroban_sdk::BytesN::from_array(&env, &[2u8; 32]),
    );

    // No contract-level entry → falls back to global → suspended → denied
    assert!(!client.is_token_allowed_for_contract(&contract_id, &token));
}

// ─── Expired entry cleanup ────────────────────────────────────────────────────

/// An expired, time-bounded entry can be permissionlessly removed, and the
/// storage entry is actually gone afterwards (get returns None).
#[test]
fn test_cleanup_removes_expired_entry() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    let expiry = env.ledger().sequence() + 5;
    client.set_contract_token(&admin, &contract_id, &token, &Some(expiry));
    advance_ledger(&env, 10);

    // Anyone (no admin auth) can trigger cleanup once expired.
    client.cleanup_expired_contract_token(&contract_id, &token);

    assert_eq!(client.get_contract_token_entry(&contract_id, &token), None);
}

/// Cleanup on a not-yet-expired, time-bounded entry fails and leaves the
/// entry untouched.
#[test]
#[should_panic(expected = "Entry has not expired yet")]
fn test_cleanup_fails_on_active_entry() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    let expiry = env.ledger().sequence() + 1000;
    client.set_contract_token(&admin, &contract_id, &token, &Some(expiry));

    client.cleanup_expired_contract_token(&contract_id, &token);
}

/// Cleanup on a permanent (None expiry) entry fails and leaves it untouched.
#[test]
#[should_panic(expected = "Entry is permanent and cannot be cleaned up")]
fn test_cleanup_fails_on_permanent_entry() {
    let (env, admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    client.set_contract_token(&admin, &contract_id, &token, &None);

    client.cleanup_expired_contract_token(&contract_id, &token);
}

/// Cleanup on a nonexistent entry fails.
#[test]
#[should_panic(expected = "No contract-level allowlist entry")]
fn test_cleanup_fails_on_missing_entry() {
    let (env, _admin, client) = setup();
    let contract_id = Address::generate(&env);
    let token = Address::generate(&env);

    client.cleanup_expired_contract_token(&contract_id, &token);
}

/// Cleaning up a mix of expired and active entries only removes the expired
/// ones; active (and permanent) entries are left untouched.
#[test]
fn test_cleanup_mix_of_expired_and_active_entries() {
    let (env, admin, client) = setup();
    let contract_a = Address::generate(&env);
    let contract_b = Address::generate(&env);
    let contract_c = Address::generate(&env);
    let token = Address::generate(&env);

    let base = env.ledger().sequence();
    // contract_a: will be expired
    client.set_contract_token(&admin, &contract_a, &token, &Some(base + 5));
    // contract_b: stays active
    client.set_contract_token(&admin, &contract_b, &token, &Some(base + 1000));
    // contract_c: permanent, never expires
    client.set_contract_token(&admin, &contract_c, &token, &None);

    advance_ledger(&env, 10);

    // Only contract_a's entry is expired and cleanable.
    client.cleanup_expired_contract_token(&contract_a, &token);
    assert_eq!(client.get_contract_token_entry(&contract_a, &token), None);

    // contract_b and contract_c entries are untouched.
    assert_eq!(
        client.get_contract_token_entry(&contract_b, &token),
        Some(Some(base + 1000))
    );
    assert_eq!(
        client.get_contract_token_entry(&contract_c, &token),
        Some(None)
    );

    // Attempting cleanup on the still-active or permanent entries fails and
    // does not remove them.
    let result_b = client.try_cleanup_expired_contract_token(&contract_b, &token);
    assert!(result_b.is_err());
    let result_c = client.try_cleanup_expired_contract_token(&contract_c, &token);
    assert!(result_c.is_err());

    assert!(client.get_contract_token_entry(&contract_b, &token).is_some());
    assert!(client.get_contract_token_entry(&contract_c, &token).is_some());
}
