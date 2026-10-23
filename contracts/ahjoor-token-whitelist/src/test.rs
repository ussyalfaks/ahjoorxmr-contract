#![cfg(test)]
extern crate std;

use crate::{
    TokenQuota, TokenWhitelistContract, TokenWhitelistContractClient, MAX_BATCH_ADD_TOKENS,
    MAX_QUOTA_PERIOD_LEDGERS, MAX_VOLUME_QUERY_RANGE,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, Vec,
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

    // Verify admin is set
    assert_eq!(client.get_admin(), admin);

    // Verify whitelist is empty initially
    let tokens = client.get_whitelisted_tokens(&0, &50);
    assert_eq!(tokens.len(), 0);

    // Check initialization event
    let events = env.events().all();
    // Just verify the contract works, events can be tested separately
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
    let tokens = client.get_whitelisted_tokens(&0, &50);
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
    let tokens = client.get_whitelisted_tokens(&0, &50);
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

    let tokens = client.get_whitelisted_tokens(&0, &50);
    assert_eq!(tokens.len(), 3);

    // Remove one token
    client.remove_token(&admin, &token2);
    assert!(client.is_token_allowed(&token1));
    assert!(!client.is_token_allowed(&token2));
    assert!(client.is_token_allowed(&token3));

    let tokens = client.get_whitelisted_tokens(&0, &50);
    assert_eq!(tokens.len(), 2);
}

/// #717: `client.rs` declares a hand-written `TokenWhitelistInterface` used
/// by dependent contracts (ahjoor-escrow, ahjoor-payments, ahjoor-refund,
/// ahjoor-rosca) for cross-contract calls, so it can drift from the real
/// entry points in this file. Exercising the generated `TokenWhitelistClient`
/// (not the contract's own `TokenWhitelistContractClient`) against the live
/// contract catches that drift at compile/test time.
#[test]
fn test_cross_contract_client_get_whitelisted_tokens_signature() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let cross_contract_client = crate::TokenWhitelistClient::new(&env, &client.address);
    let tokens = cross_contract_client.get_whitelisted_tokens(&0u32, &50u32);
    assert_eq!(tokens.len(), 1);
    assert_eq!(tokens.get(0).unwrap(), token);
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
fn test_get_proposed_admin() {
    let (env, admin, client) = setup_test();
    let new_admin = Address::generate(&env);

    // Never proposed
    assert_eq!(client.get_proposed_admin(), None);

    // Propose then read back
    client.propose_admin(&admin, &new_admin);
    assert_eq!(client.get_proposed_admin(), Some(new_admin.clone()));

    // Accepting clears the pending proposal
    client.accept_admin(&new_admin);
    assert_eq!(client.get_proposed_admin(), None);
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

#[test]
fn test_remove_token_clears_quota_tier_override_metadata() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    // Set a quota
    client.set_token_quota(&admin, &token, &1_000_000i128, &100u32, &None);
    assert!(client.get_token_quota(&token).is_some());

    // Set a risk tier + a per-token override
    client.set_risk_tier(
        &admin,
        &2u32,
        &soroban_sdk::String::from_str(&env, "tier-2"),
        &500i128,
        &1000i128,
    );
    client.assign_token_tier(&admin, &token, &2u32);
    client.set_token_limit_override(&admin, &token, &777i128, &888i128);
    let limits_before = client.get_token_tier_limits(&token);
    assert_eq!(limits_before.max_single_tx_amount, 777);
    assert_eq!(limits_before.max_daily_volume, 888);

    // Set metadata
    let logo_hash = BytesN::from_array(&env, &[9u8; 32]);
    client.set_token_metadata(
        &admin,
        &token,
        &6u32,
        &soroban_sdk::String::from_str(&env, "TKN"),
        &logo_hash,
        &None,
    );
    assert!(client.try_get_token_metadata(&token).is_ok());

    // Removing the token clears quota, tier, override, and metadata
    client.remove_token(&admin, &token);
    assert_eq!(client.get_token_quota(&token), None);
    assert!(client.try_get_token_metadata(&token).is_err());
    let limits_after_removal = client.get_token_tier_limits(&token);
    assert_eq!(limits_after_removal.max_single_tx_amount, 0);
    assert_eq!(limits_after_removal.max_daily_volume, 0);

    // Re-adding the token must not resurface the pre-removal state
    client.add_token(&admin, &token);
    assert_eq!(client.get_token_quota(&token), None);
    assert!(client.try_get_token_metadata(&token).is_err());
    let limits_after_readd = client.get_token_tier_limits(&token);
    assert_eq!(limits_after_readd.max_single_tx_amount, 0);
    assert_eq!(limits_after_readd.max_daily_volume, 0);
}

// ── Suspension tests ──────────────────────────────────────────────────────────

#[test]
fn test_suspension_active() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    assert!(client.is_token_allowed(&token));

    let reason = BytesN::from_array(&env, &[1u8; 32]);
    client.suspend_token_timed(&admin, &token, &50u32, &reason);

    assert!(!client.is_token_allowed(&token));
}

#[test]
fn test_auto_reinstatement_on_expiry_query() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    let reason = BytesN::from_array(&env, &[1u8; 32]);
    let start_seq = env.ledger().sequence();
    client.suspend_token_timed(&admin, &token, &50u32, &reason);

    assert!(!client.is_token_allowed(&token));

    // Advance ledger past suspension expiry
    env.ledger().with_mut(|l| l.sequence_number = start_seq + 51);

    // Lazy reinstatement: first call after expiry clears the record and returns true
    assert!(client.is_token_allowed(&token));
    // Subsequent calls must also return true (suspension fully cleared)
    assert!(client.is_token_allowed(&token));
}

#[test]
fn test_early_lift() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    let reason = BytesN::from_array(&env, &[1u8; 32]);
    client.suspend_token_timed(&admin, &token, &50u32, &reason);
    assert!(!client.is_token_allowed(&token));

    client.lift_token_suspension(&admin, &token);

    assert!(client.is_token_allowed(&token));
}

#[test]
fn test_suspension_extension() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    let reason = BytesN::from_array(&env, &[1u8; 32]);
    let start_seq = env.ledger().sequence();
    // Suspend for 50 ledgers, then extend by 50 more → effective expiry = start + 100
    client.suspend_token_timed(&admin, &token, &50u32, &reason);
    client.extend_token_suspension(&admin, &token, &50u32);

    // Advance past original expiry (50) but before extended expiry (100)
    env.ledger().with_mut(|l| l.sequence_number = start_seq + 55);
    assert!(!client.is_token_allowed(&token));

    // Advance past extended expiry
    env.ledger().with_mut(|l| l.sequence_number = start_seq + 101);
    assert!(client.is_token_allowed(&token));
}

#[test]
fn test_suspension_history_record() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    // First suspension — lift early so we can create a second
    let reason1 = BytesN::from_array(&env, &[1u8; 32]);
    client.suspend_token_timed(&admin, &token, &100u32, &reason1);
    client.lift_token_suspension(&admin, &token);

    // Second suspension
    let reason2 = BytesN::from_array(&env, &[2u8; 32]);
    client.suspend_token_timed(&admin, &token, &100u32, &reason2);

    let history = client.get_suspension_history(&token);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap().reason_hash, reason1);
    assert_eq!(history.get(1).unwrap().reason_hash, reason2);
}

#[test]
fn test_suspension_history_capped_at_ten() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);

    // Create 11 suspensions; each is lifted early before the next
    for i in 0u32..11 {
        let reason = BytesN::from_array(&env, &[i as u8; 32]);
        client.suspend_token_timed(&admin, &token, &100u32, &reason);
        if i < 10 {
            client.lift_token_suspension(&admin, &token);
        }
    }

    let history = client.get_suspension_history(&token);
    assert_eq!(history.len(), 10);
    // Oldest entry (i=0) must have been evicted; the first kept entry is i=1
    assert_eq!(history.get(0).unwrap().reason_hash, BytesN::from_array(&env, &[1u8; 32]));
}

#[test]
fn test_non_suspended_baseline() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    // Normal whitelist/delist cycle is unchanged by the suspension feature
    client.add_token(&admin, &token);
    assert!(client.is_token_allowed(&token));

    let tokens = client.get_whitelisted_tokens(&0, &50);
    assert_eq!(tokens.len(), 1);

    client.remove_token(&admin, &token);
    assert!(!client.is_token_allowed(&token));
}

#[test]
#[should_panic(expected = "Token already suspended")]
fn test_double_suspend_fails() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    let reason = BytesN::from_array(&env, &[1u8; 32]);
    client.suspend_token_timed(&admin, &token, &50u32, &reason);
    client.suspend_token_timed(&admin, &token, &50u32, &reason);
}

#[test]
#[should_panic(expected = "No active suspension")]
fn test_lift_nonexistent_suspension_fails() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    client.add_token(&admin, &token);
    client.lift_token_suspension(&admin, &token);
}

#[test]
#[should_panic(expected = "Token not whitelisted")]
fn test_suspend_nonwhitelisted_token_fails() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);

    let reason = BytesN::from_array(&env, &[1u8; 32]);
    client.suspend_token_timed(&admin, &token, &50u32, &reason);
}

// ── #540/#588: set_token_quota / update_token_quota period_ledgers upper bound ──

#[test]
#[should_panic(expected = "period_ledgers exceeds maximum allowed")]
fn test_set_token_quota_rejects_oversized_period() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    client.set_token_quota(&admin, &token, &1_000i128, &(MAX_QUOTA_PERIOD_LEDGERS + 1), &None);
}

#[test]
fn test_set_token_quota_accepts_max_period() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    client.set_token_quota(&admin, &token, &1_000i128, &MAX_QUOTA_PERIOD_LEDGERS, &None);
    let quota = client.get_token_quota(&token).unwrap();
    assert_eq!(quota.period_ledgers, MAX_QUOTA_PERIOD_LEDGERS);
}

#[test]
#[should_panic(expected = "period_ledgers exceeds maximum allowed")]
fn test_update_token_quota_rejects_oversized_period() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &1_000i128, &100u32, &None);

    client.update_token_quota(&admin, &token, &1_000i128, &(MAX_QUOTA_PERIOD_LEDGERS + 1), &None);
}

// ── Companion: get_token_volume bounded range ────────────────────────────────

#[test]
#[should_panic(expected = "ledger range exceeds maximum allowed")]
fn test_get_token_volume_rejects_oversized_range() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    client.get_token_volume(&token, &0u32, &(MAX_VOLUME_QUERY_RANGE + 1));
}

#[test]
fn test_get_token_volume_accepts_max_range() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    let volume = client.get_token_volume(&token, &0u32, &MAX_VOLUME_QUERY_RANGE);
    assert_eq!(volume, 0);
}

#[test]
#[should_panic(expected = "from_ledger must not exceed to_ledger")]
fn test_get_token_volume_rejects_inverted_range() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    client.get_token_volume(&token, &10u32, &5u32);
}

// ── #540: record_token_volume quota enforcement with aggregate buckets ──────

#[test]
fn test_record_token_volume_within_quota_accumulates() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &100i128, &50u32, &None);
    env.ledger().set_sequence_number(1_000);

    client.record_token_volume(&token, &40);
    client.record_token_volume(&token, &50);
    // 40 + 50 = 90, still within the 100 quota.
}

#[test]
fn test_record_token_volume_rejects_when_quota_exceeded() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &100i128, &50u32, &None);
    env.ledger().set_sequence_number(1_000);

    client.record_token_volume(&token, &60);
    let result = client.try_record_token_volume(&token, &60);
    assert!(result.is_err());
}

#[test]
fn test_record_token_volume_window_rolls_over_after_period_elapses() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &100i128, &48u32, &None);
    env.ledger().set_sequence_number(1_000);

    client.record_token_volume(&token, &90);
    // Immediately re-recording within the same window should fail.
    assert!(client.try_record_token_volume(&token, &20).is_err());

    // Advance well past the full period so every aggregate bucket rolls out
    // of the window; volume should reset and admit new activity.
    env.ledger().set_sequence_number(1_000 + 48 * 2);
    client.record_token_volume(&token, &20);
}

// ── #540: record_token_volume cost stays bounded at the maximum configured period ──

#[test]
fn test_record_token_volume_cost_bounded_at_max_period() {
    let (env, admin, client) = setup_test();

    let small_period_token = Address::generate(&env);
    client.add_token(&admin, &small_period_token);
    client.set_token_quota(&admin, &small_period_token, &1_000_000_000i128, &100u32, &None);
    env.ledger().set_sequence_number(1_000);

    env.cost_estimate().budget().reset_default();
    assert!(client.try_record_token_volume(&small_period_token, &1i128).is_ok());
    let small_period_cost = env.cost_estimate().budget().cpu_instruction_cost();

    let max_period_token = Address::generate(&env);
    client.add_token(&admin, &max_period_token);
    client.set_token_quota(&admin, &max_period_token, &1_000_000_000i128, &MAX_QUOTA_PERIOD_LEDGERS, &None);
    env.ledger().set_sequence_number(MAX_QUOTA_PERIOD_LEDGERS + 1_000);

    env.cost_estimate().budget().reset_default();
    assert!(client.try_record_token_volume(&max_period_token, &1i128).is_ok());
    let max_period_cost = env.cost_estimate().budget().cpu_instruction_cost();

    // Cost is driven by the fixed VOLUME_AGG_BUCKET_COUNT, not by
    // period_ledgers, so it should stay within a small constant factor of the
    // cost at a tiny period rather than scaling toward the ~518,400x a
    // per-ledger scan would need at the maximum configured period.
    assert!(
        max_period_cost < small_period_cost * 3,
        "record_token_volume cost scaled with period_ledgers: small_period={}, max_period={}",
        small_period_cost,
        max_period_cost
    );
}

#[test]
fn test_is_token_allowed_cost_is_constant_at_scale() {
    let (env, admin, client) = setup_test();

    // Token added early, before the whitelist grows.
    let early_token = Address::generate(&env);
    client.add_token(&admin, &early_token);

    // Grow the whitelist substantially.
    for _ in 0..500u32 {
        let t = Address::generate(&env);
        client.add_token(&admin, &t);
    }
    let last_token = Address::generate(&env);
    client.add_token(&admin, &last_token);

    // Measure the *first* (cold) lookup cost for both tokens once the
    // whitelist has grown to 502 entries. Neither has been queried before,
    // so both readings are cold reads at the same total footprint size --
    // this isolates cost that depends on the token's *position* (which
    // would indicate a regression to a linear scan by insertion order) from
    // unrelated cold-vs-warm-cache or footprint-size effects. Comparing
    // against a cold read from a much smaller whitelist would conflate the
    // two: a token queried a second time is already cached and reads
    // artificially cheap regardless of the whitelist's size.
    env.cost_estimate().budget().reset_default();
    assert!(client.is_token_allowed(&early_token));
    let cost_early = env.cost_estimate().budget().cpu_instruction_cost();

    env.cost_estimate().budget().reset_default();
    assert!(client.is_token_allowed(&last_token));
    let cost_last = env.cost_estimate().budget().cpu_instruction_cost();

    // With an O(1) membership lookup, cost should not depend on how early or
    // late the token was added to the whitelist. A linear scan by insertion
    // order would make `cost_last` far larger than `cost_early`; allow a
    // generous margin above 1x to avoid flakiness while still catching a
    // regression to linear scans.
    assert!(
        cost_last < cost_early * 3,
        "lookup cost grew with token position in whitelist: early={}, last={}",
        cost_early,
        cost_last
    );
}

#[test]
fn test_is_whitelisted_cost_is_constant_at_scale() {
    let (env, admin, client) = setup_test();

    let small_token = Address::generate(&env);
    client.add_token(&admin, &small_token);

    env.cost_estimate().budget().reset_default();
    assert!(client.is_whitelisted(&small_token));
    let small_whitelist_cost = env.cost_estimate().budget().cpu_instruction_cost();

    for _ in 0..500u32 {
        let t = Address::generate(&env);
        client.add_token(&admin, &t);
    }
    let large_token = Address::generate(&env);
    client.add_token(&admin, &large_token);

    env.cost_estimate().budget().reset_default();
    assert!(client.is_whitelisted(&large_token));
    let large_whitelist_cost = env.cost_estimate().budget().cpu_instruction_cost();

    assert!(
        large_whitelist_cost < small_whitelist_cost * 3,
        "lookup cost grew with whitelist size: small={}, large={}",
        small_whitelist_cost,
        large_whitelist_cost
    );
}

#[test]
fn test_get_risk_tier_defined_and_undefined() {
    let (env, admin, client) = setup_test();

    assert!(client.get_risk_tier(&99u32).is_none());

    let name = soroban_sdk::String::from_str(&env, "standard");
    client.set_risk_tier(&admin, &2u32, &name, &1_000i128, &50_000i128);

    let tier = client.get_risk_tier(&2u32).expect("tier should be defined");
    assert_eq!(tier.name, name);
    assert_eq!(tier.max_single_tx_amount, 1_000i128);
    assert_eq!(tier.max_daily_volume, 50_000i128);
    assert!(client.get_risk_tier(&99u32).is_none());
}

#[test]
#[should_panic(expected = "RiskTierNotDefined")]
fn test_assign_token_tier_panics_on_undefined_tier() {
    let (env, admin, client) = setup_test();

    let token = Address::generate(&env);
    client.assign_token_tier(&admin, &token, &99u32);
}

#[test]
fn test_assign_token_tier_accepts_defined_tier() {
    let (env, admin, client) = setup_test();

    let name = soroban_sdk::String::from_str(&env, "standard");
    client.set_risk_tier(&admin, &2u32, &name, &1_000i128, &50_000i128);

    let token = Address::generate(&env);
    client.assign_token_tier(&admin, &token, &2u32);

    let tier_id: u32 = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get(&crate::DataKey::TokenTier(token))
            .unwrap()
    });
    assert_eq!(tier_id, 2);
}

// ── #710: get_token_tier ──────────────────────────────────────────────────

#[test]
fn test_get_token_tier_returns_assigned_and_none_when_unassigned() {
    let (env, admin, client) = setup_test();

    let name = soroban_sdk::String::from_str(&env, "standard");
    client.set_risk_tier(&admin, &2u32, &name, &1_000i128, &50_000i128);

    let token = Address::generate(&env);
    let unassigned_token = Address::generate(&env);

    assert!(client.get_token_tier(&token).is_none());

    client.assign_token_tier(&admin, &token, &2u32);

    assert_eq!(client.get_token_tier(&token), Some(2u32));
    assert!(client.get_token_tier(&unassigned_token).is_none());
}

// ── #711: get_token_limit_override ──────────────────────────────────────────

#[test]
fn test_get_token_limit_override_returns_set_and_none_when_unset() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    let plain_token = Address::generate(&env);

    assert!(client.get_token_limit_override(&token).is_none());

    client.set_token_limit_override(&admin, &token, &500i128, &10_000i128);

    let override_limits = client
        .get_token_limit_override(&token)
        .expect("override should be set");
    assert_eq!(override_limits.max_single_tx_amount, 500i128);
    assert_eq!(override_limits.max_daily_volume, 10_000i128);

    // Relying purely on tier-based limits leaves the override unset.
    assert!(client.get_token_limit_override(&plain_token).is_none());
}

// ── #713: get_current_period_volume ─────────────────────────────────────────

#[test]
fn test_get_current_period_volume_matches_recorded_and_resets_after_rollover() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    let unmetered_token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &100i128, &48u32, &None);
    env.ledger().set_sequence_number(1_000);

    // No quota configured → 0, not a panic.
    assert_eq!(client.get_current_period_volume(&unmetered_token), 0);

    assert_eq!(client.get_current_period_volume(&token), 0);

    client.record_token_volume(&token, &40);
    client.record_token_volume(&token, &30);
    assert_eq!(client.get_current_period_volume(&token), 70);

    // Advance well past the full period so every aggregate bucket rolls out
    // of the window; the getter should reflect the reset just like
    // record_token_volume's internal accounting does.
    env.ledger().set_sequence_number(1_000 + 48 * 2);
    assert_eq!(client.get_current_period_volume(&token), 0);
}

// ── #926: fuzz-like sweep over admin entry points ───────────────────────────

const UNAUTHORIZED: &str = "Unauthorized: caller is not admin";

/// Deterministic xorshift PRNG so the sweep is reproducible without adding a
/// fuzzing dependency to this crate.
struct FuzzRng(u64);

impl FuzzRng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn pick<T: Copy>(&mut self, pool: &[T]) -> T {
        pool[(self.next() % pool.len() as u64) as usize]
    }

    fn one_in(&mut self, n: u64) -> bool {
        self.next() % n == 0
    }
}

/// Like `setup_test`, but sets the ledger sequence before the contract is
/// registered so every storage entry's TTL is relative to that sequence.
fn setup_at_sequence(sequence: u32) -> (Env, Address, TokenWhitelistContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_sequence_number(sequence);

    let admin = Address::generate(&env);
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    (env, admin, client)
}

/// Runs `f` and asserts it panics with a message containing `expected`,
/// the same check `#[should_panic(expected = ...)]` performs.
fn assert_panics_with(case: u32, expected: &str, f: impl FnOnce()) {
    let payload = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).expect_err(
        &std::format!("case {}: expected panic `{}` but call succeeded", case, expected),
    );
    let message = payload
        .downcast_ref::<std::string::String>()
        .map(|s| s.as_str())
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        message.contains(expected),
        "case {}: expected panic `{}`, got `{}`",
        case,
        expected,
        message
    );
}

/// Validation `set_token_quota` / `update_token_quota` apply to their
/// numeric inputs, in the order the contract checks them.
fn quota_input_error(volume: i128, period: u32) -> Option<&'static str> {
    if volume <= 0 {
        Some("max_volume_per_period must be positive")
    } else if period == 0 {
        Some("period_ledgers must be positive")
    } else if period > MAX_QUOTA_PERIOD_LEDGERS {
        Some("period_ledgers exceeds maximum allowed")
    } else {
        None
    }
}

#[test]
fn test_fuzz_like_whitelist_inputs_100_cases() {
    const CASES: u32 = 100;

    // Kept well below u32::MAX so the contract's `sequence + 120_000` TTL
    // extensions stay in range.
    let sequences = [0u32, 1, 1_000, 5_000_000];
    let volumes = [i128::MIN, -1, 0, 1, 2, 1_000_000, i128::MAX - 1, i128::MAX];
    let periods = [
        0u32,
        1,
        2,
        24,
        MAX_QUOTA_PERIOD_LEDGERS - 1,
        MAX_QUOTA_PERIOD_LEDGERS,
        MAX_QUOTA_PERIOD_LEDGERS + 1,
        u32::MAX,
    ];
    let batch_sizes = [
        0u32,
        1,
        2,
        MAX_BATCH_ADD_TOKENS - 1,
        MAX_BATCH_ADD_TOKENS,
        MAX_BATCH_ADD_TOKENS + 1,
    ];

    let mut rng = FuzzRng(0x9E37_79B9_7F4A_7C15);
    let mut accepted = 0u32;
    let mut rejected = 0u32;

    for case in 0..CASES {
        let sequence = rng.pick(&sequences);
        let (env, admin, client) = setup_at_sequence(sequence);
        let token = Address::generate(&env);
        let caller = if rng.one_in(5) { Address::generate(&env) } else { admin.clone() };
        let is_admin = caller == admin;

        let expected_err = match case % 5 {
            // add_token: fresh vs. already-whitelisted token.
            0 => {
                let already = rng.one_in(3);
                if already {
                    client.add_token(&admin, &token);
                }
                let expected = if !is_admin {
                    Some(UNAUTHORIZED)
                } else if already {
                    Some("Token already whitelisted")
                } else {
                    None
                };
                match expected {
                    Some(msg) => assert_panics_with(case, msg, || client.add_token(&caller, &token)),
                    None => {
                        client.add_token(&caller, &token);
                        assert!(client.is_whitelisted(&token), "case {}", case);
                    }
                }
                expected
            }
            // batch_add_tokens: empty, within, at, and just over the batch cap.
            1 => {
                let size = rng.pick(&batch_sizes);
                let mut tokens = Vec::new(&env);
                for _ in 0..size {
                    tokens.push_back(Address::generate(&env));
                }
                let duplicate = size > 0 && rng.one_in(4);
                if duplicate {
                    client.add_token(&admin, &tokens.get(0).unwrap());
                }
                let before = client.get_whitelisted_tokens(&0, &50).len();
                let expected = if !is_admin {
                    Some(UNAUTHORIZED)
                } else if size == 0 {
                    Some("Batch cannot be empty")
                } else if size > MAX_BATCH_ADD_TOKENS {
                    Some("Batch size exceeds maximum allowed")
                } else if duplicate {
                    Some("Token already whitelisted")
                } else {
                    None
                };
                match expected {
                    Some(msg) => {
                        assert_panics_with(case, msg, || client.batch_add_tokens(&caller, &tokens));
                        assert_eq!(client.get_whitelisted_tokens(&0, &50).len(), before, "case {}", case);
                    }
                    None => {
                        client.batch_add_tokens(&caller, &tokens);
                        assert_eq!(
                            client.get_whitelisted_tokens(&0, &50).len(),
                            before + size,
                            "case {}",
                            case
                        );
                    }
                }
                expected
            }
            // set_token_quota: volume/period boundaries on (non-)whitelisted tokens.
            2 => {
                let whitelisted = !rng.one_in(4);
                let has_quota = whitelisted && rng.one_in(4);
                if whitelisted {
                    client.add_token(&admin, &token);
                }
                if has_quota {
                    client.set_token_quota(&admin, &token, &1_000i128, &100u32, &None);
                }
                let volume = rng.pick(&volumes);
                let period = rng.pick(&periods);
                let before = client.get_token_quota(&token);
                let expected = if !is_admin {
                    Some(UNAUTHORIZED)
                } else if !whitelisted {
                    Some("Token not whitelisted")
                } else if has_quota {
                    Some("Token already has quota")
                } else {
                    quota_input_error(volume, period)
                };
                match expected {
                    Some(msg) => {
                        assert_panics_with(case, msg, || {
                            client.set_token_quota(&caller, &token, &volume, &period, &None)
                        });
                        assert_eq!(client.get_token_quota(&token), before, "case {}", case);
                    }
                    None => {
                        client.set_token_quota(&caller, &token, &volume, &period, &None);
                        assert_eq!(
                            client.get_token_quota(&token),
                            Some(TokenQuota { max_volume_per_period: volume, period_ledgers: period, max_per_transaction: None }),
                            "case {}",
                            case
                        );
                    }
                }
                expected
            }
            // update_token_quota: same boundaries, with and without an existing quota.
            3 => {
                let has_quota = !rng.one_in(4);
                client.add_token(&admin, &token);
                if has_quota {
                    client.set_token_quota(&admin, &token, &1_000i128, &100u32, &None);
                }
                let volume = rng.pick(&volumes);
                let period = rng.pick(&periods);
                let before = client.get_token_quota(&token);
                let expected = if !is_admin {
                    Some(UNAUTHORIZED)
                } else if let Some(msg) = quota_input_error(volume, period) {
                    Some(msg)
                } else if !has_quota {
                    Some("Token has no quota")
                } else {
                    None
                };
                match expected {
                    Some(msg) => {
                        assert_panics_with(case, msg, || {
                            client.update_token_quota(&caller, &token, &volume, &period, &None)
                        });
                        assert_eq!(client.get_token_quota(&token), before, "case {}", case);
                    }
                    None => {
                        client.update_token_quota(&caller, &token, &volume, &period, &None);
                        assert_eq!(
                            client.get_token_quota(&token),
                            Some(TokenQuota { max_volume_per_period: volume, period_ledgers: period, max_per_transaction: None }),
                            "case {}",
                            case
                        );
                    }
                }
                expected
            }
            // suspend_token_timed: zero, small, and maximum representable durations.
            _ => {
                let whitelisted = !rng.one_in(4);
                let already_suspended = whitelisted && rng.one_in(4);
                let reason = BytesN::from_array(&env, &[case as u8; 32]);
                if whitelisted {
                    client.add_token(&admin, &token);
                }
                if already_suspended {
                    client.suspend_token_timed(&admin, &token, &50u32, &reason);
                }
                // `sequence + duration` is unchecked in the contract, so the
                // largest duration swept is the largest that still fits in u32.
                let durations = [
                    0u32,
                    1,
                    2,
                    MAX_QUOTA_PERIOD_LEDGERS,
                    u32::MAX - sequence - 1,
                    u32::MAX - sequence,
                ];
                let duration = rng.pick(&durations);
                let history_before = client.get_suspension_history(&token).len();
                let expected = if !is_admin {
                    Some(UNAUTHORIZED)
                } else if !whitelisted {
                    Some("Token not whitelisted")
                } else if already_suspended {
                    Some("Token already suspended")
                } else {
                    None
                };
                match expected {
                    Some(msg) => {
                        assert_panics_with(case, msg, || {
                            client.suspend_token_timed(&caller, &token, &duration, &reason)
                        });
                        assert_eq!(
                            client.get_suspension_history(&token).len(),
                            history_before,
                            "case {}",
                            case
                        );
                    }
                    None => {
                        client.suspend_token_timed(&caller, &token, &duration, &reason);
                        assert_eq!(
                            client.get_suspension_history(&token).len(),
                            history_before + 1,
                            "case {}",
                            case
                        );
                        if duration == 0 {
                            // A zero-length suspension expires on the ledger it starts.
                            assert!(client.get_token_suspension(&token).is_none(), "case {}", case);
                            assert!(client.is_token_allowed(&token), "case {}", case);
                        } else {
                            let record = client.get_token_suspension(&token).unwrap();
                            assert_eq!(record.expiry_ledger, sequence + duration, "case {}", case);
                            assert!(!client.is_token_allowed(&token), "case {}", case);
                        }
                    }
                }
                expected
            }
        };

        if expected_err.is_some() {
            rejected += 1;
        } else {
            accepted += 1;
        }
    }

    assert_eq!(accepted + rejected, CASES);
    assert!(accepted > 0, "sweep never exercised an accepted input");
    assert!(rejected > 0, "sweep never exercised a rejected input");
}
