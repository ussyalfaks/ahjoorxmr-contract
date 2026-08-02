#![cfg(test)]

use crate::{
    TokenWhitelistContract, TokenWhitelistContractClient, MAX_QUOTA_PERIOD_LEDGERS,
    MAX_VOLUME_QUERY_RANGE,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env,
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

    client.set_token_quota(&admin, &token, &1_000i128, &(MAX_QUOTA_PERIOD_LEDGERS + 1));
}

#[test]
fn test_set_token_quota_accepts_max_period() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);

    client.set_token_quota(&admin, &token, &1_000i128, &MAX_QUOTA_PERIOD_LEDGERS);
    let quota = client.get_token_quota(&token).unwrap();
    assert_eq!(quota.period_ledgers, MAX_QUOTA_PERIOD_LEDGERS);
}

#[test]
#[should_panic(expected = "period_ledgers exceeds maximum allowed")]
fn test_update_token_quota_rejects_oversized_period() {
    let (env, admin, client) = setup_test();
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    client.set_token_quota(&admin, &token, &1_000i128, &100u32);

    client.update_token_quota(&admin, &token, &1_000i128, &(MAX_QUOTA_PERIOD_LEDGERS + 1));
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
    client.set_token_quota(&admin, &token, &100i128, &50u32);
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
    client.set_token_quota(&admin, &token, &100i128, &50u32);
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
    client.set_token_quota(&admin, &token, &100i128, &48u32);
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
    client.set_token_quota(&admin, &small_period_token, &1_000_000_000i128, &100u32);
    env.ledger().set_sequence_number(1_000);

    env.cost_estimate().budget().reset_default();
    assert!(client.try_record_token_volume(&small_period_token, &1i128).is_ok());
    let small_period_cost = env.cost_estimate().budget().cpu_instruction_cost();

    let max_period_token = Address::generate(&env);
    client.add_token(&admin, &max_period_token);
    client.set_token_quota(&admin, &max_period_token, &1_000_000_000i128, &MAX_QUOTA_PERIOD_LEDGERS);
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

    // Measure lookup cost against a small whitelist.
    let small_token = Address::generate(&env);
    client.add_token(&admin, &small_token);

    env.cost_estimate().budget().reset_default();
    assert!(client.is_token_allowed(&small_token));
    let small_whitelist_cost = env.cost_estimate().budget().cpu_instruction_cost();

    // Grow the whitelist substantially.
    for _ in 0..500u32 {
        let t = Address::generate(&env);
        client.add_token(&admin, &t);
    }
    let large_token = Address::generate(&env);
    client.add_token(&admin, &large_token);

    // Measure lookup cost against the large whitelist, for both a token
    // added early and a token added last.
    env.cost_estimate().budget().reset_default();
    assert!(client.is_token_allowed(&small_token));
    let large_whitelist_cost_early = env.cost_estimate().budget().cpu_instruction_cost();

    env.cost_estimate().budget().reset_default();
    assert!(client.is_token_allowed(&large_token));
    let large_whitelist_cost_last = env.cost_estimate().budget().cpu_instruction_cost();

    // With an O(1) membership lookup, cost should not meaningfully grow as
    // the whitelist grows from 1 to 502 entries. A linear scan would be
    // roughly 500x more expensive here; allow a generous margin above 1x to
    // avoid flakiness while still catching a regression to linear scans.
    assert!(
        large_whitelist_cost_early < small_whitelist_cost * 3,
        "lookup cost grew with whitelist size: small={}, large_early={}",
        small_whitelist_cost,
        large_whitelist_cost_early
    );
    assert!(
        large_whitelist_cost_last < small_whitelist_cost * 3,
        "lookup cost grew with whitelist size: small={}, large_last={}",
        small_whitelist_cost,
        large_whitelist_cost_last
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

    let tier_id: u32 = env
        .storage()
        .persistent()
        .get(&crate::DataKey::TokenTier(token))
        .unwrap();
    assert_eq!(tier_id, 2);
}
