#![cfg(test)]

use crate::{AhjoorContract, AhjoorContractClient, PayoutStrategy, RoscaConfig, VotingMode, MIGRATION_TIMEOUT_SECONDS};
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, Vec};

fn default_config(fee_recipient: &Address) -> RoscaConfig {
    RoscaConfig {
        strategy: PayoutStrategy::RoundRobin,
        custom_order: None,
        penalty_amount: 50,
        exit_penalty_bps: 1000,
        collective_goal: None,
        member_goals: None,
        fee_bps: 100u32,
        fee_recipient: Some(fee_recipient.clone()),
        max_defaults: 3,
        grace_period_ledgers: 0,
        use_timestamp_schedule: false,
        round_duration_seconds: 0,
        max_members: Some(10),
        skip_fee: 10,
        max_skips_per_cycle: 1,
        voting_mode: VotingMode::Equal,
        late_fee_bps: 0,
        grace_period_seconds: 0,
        auction_enabled: false,
        auction_window_ledgers: 0,
        randomize_payout_order: false,
        reserve_enabled: false,
        reserve_contribution_bps: 0,
    }
}

fn setup_group<'a>(env: &'a Env, admin: &Address, member: &Address, token: &Address) -> AhjoorContractClient<'a> {
    let contract_id = env.register_contract(None, AhjoorContract);
    let client = AhjoorContractClient::new(env, &contract_id);
    let mut members = Vec::new(env);
    members.push_back(member.clone());
    let config = default_config(admin);
    client.init(admin, &members, &1000i128, token, &100u64, &config, &None);
    client
}

/// Test that approve_migration_exit fails after MIGRATION_TIMEOUT_SECONDS have elapsed.
#[test]
fn test_migration_timeout_blocks_approval() {
    let env = Env::default();
    env.mock_all_auths();

    // Shared token via SAC
    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = sac.address();

    let src_admin = Address::generate(&env);
    let member = Address::generate(&env);
    let src_client = setup_group(&env, &src_admin, &member, &token);

    let dest_admin = Address::generate(&env);
    let dest_member = Address::generate(&env);
    let dest_client = setup_group(&env, &dest_admin, &dest_member, &token);

    let start_time: u64 = 1_000_000;
    env.ledger().with_mut(|l| l.timestamp = start_time);

    // Member requests migration
    src_client.request_group_migration(&member, &dest_client.address, &1u32);

    // Verify created_at is stored correctly
    let req = src_client.get_migration_request(&member).unwrap();
    assert_eq!(req.created_at, start_time);

    // Fast-forward past the timeout boundary
    env.ledger().with_mut(|l| l.timestamp = start_time + MIGRATION_TIMEOUT_SECONDS + 1);

    // Source admin tries to approve — must fail with MigrationNotApproved (108)
    let res = src_client.try_approve_migration_exit(&member);
    assert!(res.is_err(), "Approval after timeout should be rejected");
}

/// Test that valid in-window migrations can still be approved normally.
#[test]
fn test_migration_within_window_can_be_approved() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = sac.address();

    let src_admin = Address::generate(&env);
    let member = Address::generate(&env);
    let src_client = setup_group(&env, &src_admin, &member, &token);

    let dest_admin = Address::generate(&env);
    let dest_member = Address::generate(&env);
    let dest_client = setup_group(&env, &dest_admin, &dest_member, &token);

    let start_time: u64 = 1_000_000;
    env.ledger().with_mut(|l| l.timestamp = start_time);

    src_client.request_group_migration(&member, &dest_client.address, &1u32);

    // Move to just before timeout (still within window)
    env.ledger().with_mut(|l| l.timestamp = start_time + MIGRATION_TIMEOUT_SECONDS);

    // Approval at exactly the boundary should succeed (> check, not >=)
    src_client.approve_migration_exit(&member);

    let req = src_client.get_migration_request(&member).unwrap();
    assert_eq!(
        req.state,
        crate::MigrationApprovalState::SourceApproved,
        "Request should be in SourceApproved state"
    );
}

/// Test that cancel_migration after timeout removes the migration storage.
#[test]
fn test_migration_timeout_cancellation() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = sac.address();

    let src_admin = Address::generate(&env);
    let member = Address::generate(&env);
    let src_client = setup_group(&env, &src_admin, &member, &token);

    let dest_admin = Address::generate(&env);
    let dest_member = Address::generate(&env);
    let dest_client = setup_group(&env, &dest_admin, &dest_member, &token);

    let start_time: u64 = 1_000_000;
    env.ledger().with_mut(|l| l.timestamp = start_time);

    // Request migration on source
    src_client.request_group_migration(&member, &dest_client.address, &1u32);

    // Destination admin approves entry (records IncomingMigration)
    dest_client.approve_migration_entry(&member, &src_client.address, &1u32);

    // Advance past the 7-day window
    env.ledger().with_mut(|l| l.timestamp = start_time + MIGRATION_TIMEOUT_SECONDS + 1);

    // Approve on source is now rejected
    let approve_res = src_client.try_approve_migration_exit(&member);
    assert!(approve_res.is_err(), "Approval must fail after timeout");

    // Execute on dest must also fail (source not BothApproved)
    let exec_res = dest_client.try_execute_migration(&member, &src_client.address);
    assert!(exec_res.is_err(), "Execute must fail when source not approved");

    // Member cancels on source: removes MigrationRequests[member]
    src_client.cancel_migration(&member, &member);
    let req_after = src_client.get_migration_request(&member);
    assert!(req_after.is_none(), "MigrationRequests entry must be removed after cancel");

    // Member cancels on dest: removes IncomingMigrations[member]
    dest_client.cancel_migration(&member, &member);
    let incoming_slots = dest_client.get_vacant_slots();
    // Slot 1 should be available again — dest member count was only 1 so slot 1 is a new slot
    // Key assertion: second cancel doesn't panic, i.e. idempotent on dest
}

/// Test that source admin can force-cancel an outbound migration regardless of timeout/state.
#[test]
fn test_admin_cancel_migration_clears_source_request() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = sac.address();

    let src_admin = Address::generate(&env);
    let member = Address::generate(&env);
    let src_client = setup_group(&env, &src_admin, &member, &token);

    let dest_admin = Address::generate(&env);
    let dest_member = Address::generate(&env);
    let dest_client = setup_group(&env, &dest_admin, &dest_member, &token);

    src_client.request_group_migration(&member, &dest_client.address, &0u32);
    src_client.approve_migration_exit(&member);
    assert!(src_client.get_migration_request(&member).is_some());

    src_client.admin_cancel_migration(&member);
    assert!(src_client.get_migration_request(&member).is_none());
}

/// Test that destination admin can force-cancel incoming migration approvals.
#[test]
fn test_admin_cancel_migration_clears_destination_incoming() {
    let env = Env::default();
    env.mock_all_auths();

    let token_admin = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
    let token = sac.address();

    let src_admin = Address::generate(&env);
    let member = Address::generate(&env);
    let src_client = setup_group(&env, &src_admin, &member, &token);

    let dest_admin = Address::generate(&env);
    let dest_member = Address::generate(&env);
    let dest_client = setup_group(&env, &dest_admin, &dest_member, &token);

    src_client.request_group_migration(&member, &dest_client.address, &0u32);
    dest_client.approve_migration_entry(&member, &src_client.address, &0u32);

    // Force-cancel on destination before source reaches BothApproved.
    dest_client.admin_cancel_migration(&member);

    // Incoming state should be gone; execute now fails because migration is no longer present.
    let exec_res = dest_client.try_execute_migration(&member, &src_client.address);
    assert!(exec_res.is_err(), "execute should fail after admin force-cancel");
}
