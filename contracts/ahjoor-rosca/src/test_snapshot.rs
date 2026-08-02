#![cfg(test)]
use super::*;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

fn setup_snapshot<'a>() -> (Env, AhjoorContractClient<'a>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let tac = TokenAdminClient::new(&env, &token_addr);

    let member1 = Address::generate(&env);
    let member2 = Address::generate(&env);
    tac.mint(&member1, &10_000);
    tac.mint(&member2, &10_000);

    let mut members = soroban_sdk::Vec::new(&env);
    members.push_back(member1.clone());
    members.push_back(member2.clone());

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

    (env, client, admin, member1, member2)
}

// ---------------------------------------------------------------------------
// Test: snapshot captures group state
// ---------------------------------------------------------------------------
#[test]
fn test_snapshot_captures_state() {
    let (env, client, admin, _m1, _m2) = setup_snapshot();

    let snapshot_id = client.take_snapshot(&admin);
    assert_eq!(snapshot_id, 0);

    let snapshot = client.get_snapshot(&snapshot_id);
    assert_eq!(snapshot.snapshot_id, 0);
    assert_eq!(snapshot.round_number, 0);
    assert_eq!(snapshot.taken_by, admin);
    assert_eq!(snapshot.payout_order.len(), 2);
}

// ---------------------------------------------------------------------------
// Test: state_hash is deterministic (same state → same hash)
// ---------------------------------------------------------------------------
#[test]
fn test_state_hash_integrity() {
    let (env, client, admin, _m1, _m2) = setup_snapshot();

    let id1 = client.take_snapshot(&admin);

    // Advance ledger so spam guard doesn't block
    env.ledger().with_mut(|l| l.sequence_number += 100);

    let id2 = client.take_snapshot(&admin);

    let s1 = client.get_snapshot(&id1);
    let s2 = client.get_snapshot(&id2);

    // Same state → same hash
    assert_eq!(s1.state_hash, s2.state_hash);
}

// ---------------------------------------------------------------------------
// Test: multiple snapshots accumulate (append-only)
// ---------------------------------------------------------------------------
#[test]
fn test_multiple_snapshots_accumulate() {
    let (env, client, admin, _m1, _m2) = setup_snapshot();

    assert_eq!(client.get_snapshot_count(), 0);

    client.take_snapshot(&admin);
    env.ledger().with_mut(|l| l.sequence_number += 100);
    client.take_snapshot(&admin);
    env.ledger().with_mut(|l| l.sequence_number += 100);
    client.take_snapshot(&admin);

    assert_eq!(client.get_snapshot_count(), 3);
}

// ---------------------------------------------------------------------------
// Test: spam guard rejects snapshot taken too soon
// ---------------------------------------------------------------------------
#[test]
fn test_spam_guard_rejects_too_soon() {
    let (env, client, admin, _m1, _m2) = setup_snapshot();

    client.set_min_snapshot_interval(&admin, &100u32);
    client.take_snapshot(&admin);

    // Try again immediately (same ledger sequence)
    let result = client.try_take_snapshot(&admin);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: member can take snapshot
// ---------------------------------------------------------------------------
#[test]
fn test_member_can_take_snapshot() {
    let (_env, client, _admin, member1, _m2) = setup_snapshot();

    let snapshot_id = client.take_snapshot(&member1);
    assert_eq!(snapshot_id, 0);
    let snapshot = client.get_snapshot(&snapshot_id);
    assert_eq!(snapshot.taken_by, member1);
}

// ---------------------------------------------------------------------------
// Test: get_snapshot by ID returns correct snapshot
// ---------------------------------------------------------------------------
#[test]
fn test_get_snapshot_by_id() {
    let (env, client, admin, _m1, _m2) = setup_snapshot();

    client.take_snapshot(&admin);
    env.ledger().with_mut(|l| l.sequence_number += 100);
    client.take_snapshot(&admin);

    let s0 = client.get_snapshot(&0u32);
    let s1 = client.get_snapshot(&1u32);
    assert_eq!(s0.snapshot_id, 0);
    assert_eq!(s1.snapshot_id, 1);
}

// ---------------------------------------------------------------------------
// Test: get_group_snapshot_at returns snapshot for the requested round
// ---------------------------------------------------------------------------
#[test]
fn test_get_group_snapshot_at_round_found() {
    let (_env, client, admin, _m1, _m2) = setup_snapshot();
    client.take_snapshot(&admin);

    let snap_opt = client.get_group_snapshot_at(&0u32, &0u32);
    assert!(snap_opt.is_some());
    let snap = snap_opt.unwrap();
    assert_eq!(snap.round_number, 0);
    assert_eq!(snap.snapshot_id, 0);
}

// ---------------------------------------------------------------------------
// Test: get_group_snapshot_at returns None when no snapshot exists
// ---------------------------------------------------------------------------
#[test]
fn test_get_group_snapshot_at_round_not_found() {
    let (_env, client, _admin, _m1, _m2) = setup_snapshot();
    let snap_opt = client.get_group_snapshot_at(&0u32, &99u32);
    assert!(snap_opt.is_none());
}
