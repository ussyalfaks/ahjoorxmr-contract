#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token::{Client as TokenClient, StellarAssetClient as TokenAdminClient},
    Address, Env, IntoVal, Symbol,
};

fn setup_insurance<'a>(
    pool_amount: i128,
) -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    TokenClient<'a>,
    TokenAdminClient<'a>,
    soroban_sdk::Vec<Address>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..3 {
        let m = Address::generate(&env);
        token_admin_client.mint(&m, &5000);
        members.push_back(m);
    }

    // Mint to admin for insurance pool funding
    token_admin_client.mint(&admin, &10_000);

    client.init(
        &admin,
        &members,
        &100,
        &token_addr,
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
            grace_period_seconds: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );

    // Set coverage mode to Full
    client.set_insurance_coverage_mode(&admin, &InsuranceCoverageMode::Full);

    // Fund the insurance pool using a group member (admin is not a member)
    if pool_amount > 0 {
        let funder = members.get(0).unwrap();
        token_admin_client.mint(&funder, &pool_amount);
        client.contribute_to_insurance(&funder, &token_addr, &pool_amount);
    }

    (env, client, admin, token_addr, token_client, token_admin_client, members)
}

fn count_topic_events(env: &Env, topic: &str) -> u32 {
    let expected_topics = (Symbol::new(env, topic),).into_val(env);
    let mut count = 0u32;
    for event in env.events().all().iter() {
        if event.1 == expected_topics {
            count += 1;
        }
    }
    count
}

/// #395: Drawing from an underfunded Full-mode insurance pool must transfer
/// exactly pool_balance (not the full shortfall), leave the pool at 0, and
/// never allow the stored pool balance to go negative.
#[test]
fn test_insurance_pool_cannot_go_negative() {
    // Pool has 500 tokens; shortfall will be 700 (one member doesn't contribute,
    // contributing 200 instead of 300 total — we use 2 paying members out of 3).
    let pool_seed = 500i128;
    let (env, client, admin, token_addr, token_client, token_admin_client, members) =
        setup_insurance(pool_seed);

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();
    // member2 will be the defaulter

    // Advance time past start
    env.ledger().with_mut(|l| l.timestamp = 100);

    // Only 2 of 3 members contribute (shortfall = 100 tokens)
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);

    // Advance past round deadline and finalize
    env.ledger().with_mut(|l| l.timestamp = 3800);
    client.finalize_round();

    // Pool was 500; shortfall was 100 (one member defaulted out of 100 per member).
    // After draw: pool should be 400 (500 - 100), not negative.
    let pool_after = client.get_insurance_pool();
    assert!(pool_after >= 0, "InsurancePool must never go negative, got {}", pool_after);
    assert_eq!(pool_after, 400, "Pool should be 500 - 100 = 400 after covering shortfall");
}

/// #395: When the pool is smaller than the shortfall in Full mode, the contract
/// draws exactly pool_balance and leaves it at 0 (no negative storage value).
#[test]
fn test_insurance_pool_partial_cover_when_underfunded() {
    // Fund pool with only 50; shortfall will be 100 (one defaulter)
    let (env, client, admin, token_addr, _token_client, _token_admin_client, members) =
        setup_insurance(50);

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();

    env.ledger().with_mut(|l| l.timestamp = 100);
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);

    env.ledger().with_mut(|l| l.timestamp = 3800);
    client.finalize_round();

    let pool_after = client.get_insurance_pool();
    // Pool had 50 < shortfall of 100; all 50 should be drawn, leaving 0.
    assert!(pool_after >= 0, "InsurancePool must never go negative, got {}", pool_after);
    assert_eq!(pool_after, 0, "Pool should be 0 after exhausting 50 towards 100 shortfall");
}

#[test]
fn test_insurance_pool_low_emits_once_then_exhausts() {
    let (env, client, admin, token_addr, _token_client, _token_admin_client, members) =
        setup_insurance(250);

    client.set_insurance_pool_low_threshold(&admin, &200);
    assert_eq!(client.get_insurance_pool_low_threshold(), 200);
    assert_eq!(client.get_insurance_pool_balance(&0), 250);

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();

    // Round 0: draw 100 from 250 -> 150 (crosses below 200; low event should fire once).
    env.ledger().with_mut(|l| l.timestamp = 100);
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);
    env.ledger().with_mut(|l| l.timestamp = 3800);
    client.finalize_round();
    assert_eq!(client.get_insurance_pool(), 150);
    assert_eq!(count_topic_events(&env, "insurance_pool_low"), 1);
    assert_eq!(count_topic_events(&env, "InsExhausted"), 0);

    // Round 1: draw 100 from 150 -> 50 (already below threshold; no new low event).
    env.ledger().with_mut(|l| l.timestamp = 3900);
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);
    env.ledger().with_mut(|l| l.timestamp = 7600);
    client.finalize_round();
    assert_eq!(client.get_insurance_pool(), 50);
    assert_eq!(count_topic_events(&env, "insurance_pool_low"), 1);
    assert_eq!(count_topic_events(&env, "InsExhausted"), 0);

    // Round 2: draw remaining 50 from 50 -> 0; pool exhausted event should fire.
    env.ledger().with_mut(|l| l.timestamp = 7700);
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);
    env.ledger().with_mut(|l| l.timestamp = 11400);
    client.finalize_round();
    assert_eq!(client.get_insurance_pool(), 0);
    assert_eq!(count_topic_events(&env, "insurance_pool_low"), 1);
    assert_eq!(count_topic_events(&env, "InsExhausted"), 1);
}
