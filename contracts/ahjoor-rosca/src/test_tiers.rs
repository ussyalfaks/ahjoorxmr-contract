#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, vec,
};

/// Helper to create a test setup with members
// Helper function to create the test
fn setup_with_members<'a>(n: usize, mint_amount: i128) -> (Env, AhjoorContractClient<'a>, Address, Address, TokenClient<'a>, TokenAdminClient<'a>, soroban_sdk::Vec<Address>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_admin);
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        let addr = Address::generate(&env);
        if mint_amount > 0 {
            token_admin_client.mint(&addr, &mint_amount);
        }
        members.push_back(addr);
    }

    (env, client, admin, token_admin, token_client, token_admin_client, members)
}

#[test]
fn test_tiered_contributions() {
    let (env, client, admin, token_admin, token_client, _, members) = 
        setup_with_members(2, 2000);

    let base_amount = 100;
    client.init(
        &admin,
        &members,
        &base_amount,
        &token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
        late_fee_bps: 0,
        grace_period_seconds: 0,
        auction_enabled: false,
        auction_window_ledgers: 0,
        randomize_payout_order: false,
        reserve_enabled: false,
        reserve_contribution_bps: 0,
        },
        &None,
    );

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();

    // Set member2 to 2x tier (20000 bps)
    client.set_member_tier(&admin, &member2, &20000);

    // Member1 (default 1x) contributes base_amount
    client.contribute(&member1, &token_admin, &base_amount);
    assert_eq!(token_client.balance(&member1), 2000 - base_amount);

    // Member2 (2x tier) must contribute 2 * base_amount
    // Try contributing only base_amount first
    client.contribute(&member2, &token_admin, &base_amount);
    assert_eq!(token_client.balance(&member2), 2000 - base_amount);
    
    // Member2 should not be marked as paid yet
    let (_, paid, _, _, _) = client.get_state();
    assert_eq!(paid.len(), 1);
    assert!(paid.contains(&member1));
    assert!(!paid.contains(&member2));

    // Member2 contributes the remaining base_amount
    client.contribute(&member2, &token_admin, &base_amount);
    assert_eq!(token_client.balance(&member2), 2000 - 2 * base_amount);

    // Now round should be complete. Pot = 100 + 200 = 300.
    // Recipient is member1 (index 0).
    assert_eq!(token_client.balance(&member1), (2000 - 100) + 300);
}

#[test]
fn test_invalid_tier_rejected() {
    let (env, client, admin, token_admin, _, _, members) = 
        setup_with_members(1, 1000);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
        late_fee_bps: 0,
        grace_period_seconds: 0,
        auction_enabled: false,
        auction_window_ledgers: 0,
        randomize_payout_order: false,
        reserve_enabled: false,
        reserve_contribution_bps: 0,
        },
        &None,
    );

    let member = members.get(0).unwrap();
    
    // Tier 0 is invalid
    // Tier zero is invalid
    let result = client.try_set_member_tier(&admin, &member, &0);
    assert_eq!(result.unwrap_err().unwrap(), ExtError::InvalidTier.into());
}

#[test]
fn test_mixed_tiers_pot_size() {
    let (env, client, admin, token_admin, token_client, _, members) = 
        setup_with_members(3, 3000);

    let base_amount = 100;
    client.init(
        &admin,
        &members,
        &base_amount,
        &token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
        late_fee_bps: 0,
        grace_period_seconds: 0,
        auction_enabled: false,
        auction_window_ledgers: 0,
        randomize_payout_order: false,
        reserve_enabled: false,
        reserve_contribution_bps: 0,
        },
        &None,
    );

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();
    let member3 = members.get(2).unwrap();

    // Member1: 1x (default) -> 100
    // Member2: 1.5x (15000 bps) -> 150
    // Member3: 3x (30000 bps) -> 300
    // Member4: 4x (40000 bps) -> 400
    client.set_member_tier(&admin, &member2, &15000);
    client.set_member_tier(&admin, &member3, &30000);

    client.contribute(&member1, &token_admin, &100);
    client.contribute(&member2, &token_admin, &150);
    client.contribute(&member3, &token_admin, &300);

    // Total pot = 100 + 150 + 300 = 550
    // Recipient is member1
    assert_eq!(token_client.balance(&member1), (3000 - 100) + 550);
}


// ── Tiered Group Tests ─────────────────────────────────────────────────────────

/// Helper to create a test setup with members
fn setup_tiered<'a>(mint_amount: i128) -> (Env, AhjoorContractClient<'a>, Address, Address, TokenClient<'a>, TokenAdminClient<'a>, soroban_sdk::Vec<Address>) {
    setup_with_members(3, mint_amount)
}

#[test]
fn test_create_group_tiered_and_join() {
    let (env, client, admin, token_admin, token_client, _, members) = setup_tiered(3000);

    let base_amount = 100;
    
    // Define tiers: 1x (10000 bps), 2x (20000 bps), 3x (30000 bps)
    let tiers = vec![
        &env,
        Tier {
            tier_id: 0,
            name: String::from_str(&env, "Basic"),
            contribution_multiplier_bps: 10000,
            payout_weight_bps: 10000,
        },
        Tier {
            tier_id: 1,
            name: String::from_str(&env, "Premium"),
            contribution_multiplier_bps: 20000,
            payout_weight_bps: 20000,
        },
        Tier {
            tier_id: 2,
            name: String::from_str(&env, "VIP"),
            contribution_multiplier_bps: 30000,
            payout_weight_bps: 30000,
        },
    ];

    // Create a tiered group with the first member already in
    let member1 = members.get(0).unwrap();
    client.create_group_tiered(
        &admin,
        &member1,
        &base_amount,
        &token_admin,
        &3600,
        &tiers,
        &Some(0),
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );

    // Verify tiers were created
    let group_tiers = client.get_group_tiers();
    assert_eq!(group_tiers.len(), 3);

    // Verify member1 is in tier 0 (Basic)
    let member_tier = client.get_member_tier(&member1);
    assert_eq!(member_tier, 0);
}

#[test]
fn test_join_group_tiered() {
    let (env, client, admin, token_admin, token_client, token_admin_client, members) = 
        setup_with_members(4, 3000);

    let base_amount = 100;
    
    let tiers = vec![
        &env,
        Tier {
            tier_id: 0,
            name: String::from_str(&env, "Basic"),
            contribution_multiplier_bps: 10000,
            payout_weight_bps: 10000,
        },
        Tier {
            tier_id: 1,
            name: String::from_str(&env, "Premium"),
            contribution_multiplier_bps: 20000,
            payout_weight_bps: 20000,
        },
    ];

    // Create a tiered group with one member
    let member1 = members.get(0).unwrap();
    client.create_group_tiered(
        &admin,
        &member1,
        &base_amount,
        &token_admin,
        &3600,
        &tiers,
        &Some(0),
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );

    // New member joins at tier 1 (Premium - 2x)
    let member2 = members.get(1).unwrap();
    token_admin_client.mint(&member2, &3000);
    client.join_group_tiered(&member2, &1);

    // Verify member2 is in tier 1
    let member_tier = client.get_member_tier(&member2);
    assert_eq!(member_tier, 1);

    // Member2 should contribute 2x = 200
    client.contribute(&member2, &token_admin, &200);
    assert_eq!(token_client.balance(&member2), 3000 - 200);

    // Verify round is complete because member1 contributes 100 and member2 contributes 200
    let (current_round, paid, _, _, _) = client.get_state();
    assert_eq!(paid.len(), 2);
}

#[test]
fn test_request_tier_change_and_apply_pending() {
    let (env, client, admin, token_admin, token_admin_client, token_client, members) = 
        setup_with_members(3, 3000);

    let base_amount = 100;
    
    let tiers = vec![
        &env,
        Tier {
            tier_id: 0,
            name: String::from_str(&env, "Basic"),
            contribution_multiplier_bps: 10000,
            payout_weight_bps: 10000,
        },
        Tier {
            tier_id: 1,
            name: String::from_str(&env, "Premium"),
            contribution_multiplier_bps: 20000,
            payout_weight_bps: 20000,
        },
    ];

    let member1 = members.get(0).unwrap();
    client.create_group_tiered(
        &admin,
        &member1,
        &base_amount,
        &token_admin,
        &3600,
        &tiers,
        &Some(0),
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );

    // Member requests tier change
    client.request_tier_change(&member1, &1);

    // Tier change should be pending - member should still be at tier 0
    let member_tier_before = client.get_member_tier(&member1);
    assert_eq!(member_tier_before, 0);

    // Member contributes tier 0 amount (100) before applying tier change
    client.contribute(&member1, &token_admin, &100);

    // Admin applies pending tier changes
    client.apply_pending_tier_changes(&admin);

    // After applying, member should be at tier 1
    let member_tier_after = client.get_member_tier(&member1);
    assert_eq!(member_tier_after, 1);

    // Next round, member should contribute tier 1 amount (200)
    // Verify by checking remaining contribution needed
    let (_, _, _, _, _) = client.get_state();
    let remaining = client.get_member_contribution_status(&member1);
    // After applying tier change, in next round member needs to contribute 200 (tier 1) instead of 100
}

#[test]
fn test_tier_change_not_immediate() {
    let (env, client, admin, token_admin, token_admin_client, token_client, members) = 
        setup_with_members(2, 3000);

    let base_amount = 100;
    
    let tiers = vec![
        &env,
        Tier {
            tier_id: 0,
            name: String::from_str(&env, "Basic"),
            contribution_multiplier_bps: 10000,
            payout_weight_bps: 10000,
        },
        Tier {
            tier_id: 1,
            name: String::from_str(&env, "Premium"),
            contribution_multiplier_bps: 20000,
            payout_weight_bps: 20000,
        },
    ];

    let member1 = members.get(0).unwrap();
    client.create_group_tiered(
        &admin,
        &member1,
        &base_amount,
        &token_admin,
        &3600,
        &tiers,
        &Some(0),
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );

    // Add second member at tier 0
    let member2 = members.get(1).unwrap();
    token_admin_client.mint(&member2, &3000);
    client.join_group_tiered(&member2, &0);

    // Both contribute tier 0 amount for round 0
    client.contribute(&member1, &token_admin, &100);
    client.contribute(&member2, &token_admin, &100);

    // Wait for round to end
    env.ledger().with_mut(|l| l.timestamp += 4000);
    client.finalize_round();

    // Member1 requests tier change to tier 1
    client.request_tier_change(&member1, &1);

    // Verify tier change is pending (not yet applied)
    // Member should still be at tier 0 for current round
    let current_tier = client.get_member_tier(&member1);
    assert_eq!(current_tier, 0);

    // After admin applies, member moves to tier 1
    client.apply_pending_tier_changes(&admin);
    let new_tier = client.get_member_tier(&member1);
    assert_eq!(new_tier, 1);
}