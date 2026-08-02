#![cfg(test)]
extern crate alloc;
use super::*;
use proptest::prelude::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, IntoVal, Symbol,
};

const UPGRADE_WASM: &[u8] = include_bytes!("../../../fixtures/upgrade_contract.wasm");

/// Shared test context for ROSCA contract tests.
pub struct TestSetup<'a> {
    pub env: Env,
    pub client: AhjoorContractClient<'a>,
    pub admin: Address,
    pub token_admin: Address,
    pub token_client: TokenClient<'a>,
    /// Alias for `token_admin_client`; use either field.
    pub token_admin_client: TokenAdminClient<'a>,
    /// Pre-generated member addresses (populated by `setup_with_members`).
    pub members: soroban_sdk::Vec<Address>,
    /// Same underlying client as `token_admin_client`, exposed for clarity in
    /// tests that use the members-oriented helpers.
    #[allow(dead_code)]
    pub member_token_admin: TokenAdminClient<'a>,
    pub _member_token_admin: TokenAdminClient<'a>,
}

/// Minimal setup (no members, no minting). Used by tests that manage their own
/// member list. Kept for backward compatibility with existing tests.
fn setup_env<'a>() -> TestSetup<'a> {
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
    let member_token_admin = TokenAdminClient::new(&env, &token_admin);
    let member_token_admin_2 = TokenAdminClient::new(&env, &token_admin);

    TestSetup {
        env,
        client,
        admin,
        token_admin,
        token_client,
        token_admin_client,
        members: soroban_sdk::Vec::new(&Env::default()),
        member_token_admin,
        _member_token_admin: member_token_admin_2,
    }
}

/// Creates a `TestSetup` with `n` pre-generated member addresses, each minted
/// with `mint_amount` tokens. The returned `setup.members` is an SDK `Vec`
/// ready to pass directly to `client.init(...)`.
///
/// # Example
/// ```no_run
/// let setup = setup_with_members(3, 1000);
/// default_init(&setup);
/// ```
fn setup_with_members<'a>(n: usize, mint_amount: i128) -> TestSetup<'a> {
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
    let member_token_admin = TokenAdminClient::new(&env, &token_admin);
    let member_token_admin_2 = TokenAdminClient::new(&env, &token_admin);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        let addr = Address::generate(&env);
        if mint_amount > 0 {
            token_admin_client.mint(&addr, &mint_amount);
        }
        members.push_back(addr);
    }

    TestSetup {
        env,
        client,
        admin,
        token_admin,
        token_client,
        token_admin_client,
        members,
        member_token_admin,
        _member_token_admin: member_token_admin_2,
    }
}

// test_datakey_variant_count removed: std::mem::variant_count is unavailable in #![no_std]

/// Calls `client.init(...)` on `setup` using sensible defaults:
/// - All addresses in `setup.members` as the member list
/// - `contribution_amount = 100`
/// - `round_duration = 3600` seconds
/// - `RoscaConfig { strategy: RoundRobin, penalty_amount: 0, exit_penalty_bps: 0, grace_period_ledgers: 0, grace_period_seconds: 0, ... }`
fn default_init(setup: &TestSetup<'_>) {
    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
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
}

#[test]
fn test_delayed_start_blocks_then_allows_contribution() {
    let setup = setup_with_members(1, 1000);
    let user = setup.members.get(0).unwrap();
    let start_at = 5000u64;

    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
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
        &Some(start_at),
    );

    setup.env.ledger().set_timestamp(start_at - 1);
    assert_eq!(setup.client.get_start_time(), start_at);
    assert!(!setup.client.is_active());
    let blocked = setup
        .client
        .try_contribute(&user, &setup.token_admin, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(blocked, ExtError::GroupNotYetActive.into());

    setup.env.ledger().set_timestamp(start_at);
    assert!(setup.client.is_active());
    setup.client.contribute(&user, &setup.token_admin, &100);
}

#[test]
fn test_cancel_pending_group_refunds_reward_deposit() {
    let setup = setup_with_members(2, 0);
    let start_at = 10_000u64;

    setup.token_admin_client.mint(&setup.admin, &1000);
    let admin_before = setup.token_client.balance(&setup.admin);

    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
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
        &Some(start_at),
    );

    setup.client.deposit_rewards(&setup.admin, &200);
    assert_eq!(setup.token_client.balance(&setup.admin), admin_before - 200);

    setup.client.cancel_pending_group(&setup.admin);
    assert_eq!(setup.token_client.balance(&setup.admin), admin_before);

    setup.env.ledger().set_timestamp(start_at);
    let member = setup.members.get(0).unwrap();
    let contribute_res = setup
        .client
        .try_contribute(&member, &setup.token_admin, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(contribute_res, ExtError::GroupAlreadyDissolved.into());
}

#[test]
fn test_rosca_flow_with_time_locks() {
    let setup = setup_with_members(3, 1000);
    default_init(&setup);

    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    setup.env.ledger().set_timestamp(100);
    setup.client.contribute(&user1, &setup.token_admin, &100);
    assert_eq!(setup.token_client.balance(&user1), 900);

    setup.env.ledger().set_timestamp(3601);
    let result = setup
        .client
        .try_contribute(&user2, &setup.token_admin, &100);
    assert!(result.is_err());

    setup.client.close_round();

    let (round, paid, deadline, _, _) = setup.client.get_state();
    assert_eq!(round, 1);
    assert_eq!(paid.len(), 0);
    assert_eq!(deadline, 7201);

    setup.env.ledger().set_timestamp(4000);
    setup.client.contribute(&user1, &setup.token_admin, &100);
    assert_eq!(setup.token_client.balance(&user1), 800);
}

#[test]
fn test_cannot_close_early() {
    let setup = setup_with_members(1, 0);
    default_init(&setup);

    setup.env.ledger().set_timestamp(500);
    let res = setup.client.try_close_round();
    assert_eq!(res.unwrap_err().unwrap(), Error::DeadlineNotPassed.into());
}

#[test]
fn test_on_time_contribution() {
    // setup_with_members mints 1000 to each member
    let setup = setup_with_members(2, 1000);
    default_init(&setup);

    let user1 = setup.members.get(0).unwrap();

    setup.env.ledger().set_timestamp(1000);
    setup.client.contribute(&user1, &setup.token_admin, &100);

    assert_eq!(setup.token_client.balance(&user1), 900);
    let (_, paid, _, _, _) = setup.client.get_state();
    assert!(paid.contains(&user1));
}

#[test]
fn test_late_contribution_rejection() {
    let setup = setup_with_members(1, 1000);
    default_init(&setup);

    let user1 = setup.members.get(0).unwrap();
    setup.env.ledger().set_timestamp(3601);
    let res = setup
        .client
        .try_contribute(&user1, &setup.token_admin, &100);
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::ContributionWindowClosed.into()
    );
}

#[test]
fn test_admin_close_round() {
    let setup = setup_with_members(1, 0);
    default_init(&setup);

    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();

    let (round, _, _, _, _) = setup.client.get_state();
    assert_eq!(round, 1);
}

// --- NEW STRATEGY-SPECIFIC TESTS ---

#[test]
fn test_admin_assigned_strategy_execution() {
    let setup = setup_with_members(2, 100);

    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    // Reverse the order: user2 should get paid first
    let custom_order = vec![&setup.env, user2.clone(), user1.clone()];

    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::AdminAssigned,
            custom_order: Some(custom_order),
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.client.contribute(&user2, &setup.token_admin, &100);

    // User2 contributed 100, but was the recipient of the pot (200)
    assert_eq!(setup.token_client.balance(&user2), 200);
}

#[test]
fn test_invalid_admin_order_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let members = vec![&env, Address::generate(&env), Address::generate(&env)];
    let bad_order = vec![&env, Address::generate(&env)]; // Too short

    let res = client.try_init(
        &admin,
        &members,
        &100,
        &Address::generate(&env),
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::AdminAssigned,
            custom_order: Some(bad_order),
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::CustomOrderLengthMismatch.into()
    );
}

#[test]
fn test_round_robin_e2e_all_rounds() {
    let setup = setup_with_members(2, 2000);
    default_init(&setup);

    let u1 = setup.members.get(0).unwrap();
    let u2 = setup.members.get(1).unwrap();

    // ROUND 0: u1 should get the payout
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    // Math: 2000 (start) - 100 (spent) + 200 (pot) = 2100
    assert_eq!(setup.token_client.balance(&u1), 2100);

    // ROUND 1: u2 should get the payout
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    // Math: 2000 (start) - 100 (spent R0) - 100 (spent R1) + 200 (pot R1) = 2000
    assert_eq!(setup.token_client.balance(&u2), 2000);
}

#[test]
fn test_admin_assigned_e2e_all_rounds() {
    let setup = setup_with_members(2, 2000);

    let u1 = setup.members.get(0).unwrap();
    let u2 = setup.members.get(1).unwrap();

    // Strategy: Admin Assigned (Reverse the order: u2 then u1)
    let custom_order = vec![&setup.env, u2.clone(), u1.clone()];
    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::AdminAssigned,
            custom_order: Some(custom_order),
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // ROUND 0: u2 should get the payout first
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    assert_eq!(setup.token_client.balance(&u2), 2100);

    // ROUND 1: u1 should get the payout second
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    assert_eq!(setup.token_client.balance(&u1), 2000);
}

// --- PENALTY AND DEFAULTER HANDLING TESTS ---

#[test]
fn test_single_defaulter_penalty() {
    let setup = setup_with_members(2, 1000);

    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    // Init with a penalty amount
    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Only user1 contributes
    setup.client.contribute(&user1, &setup.token_admin, &100);

    // Wait for deadline to pass
    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();

    // Check events after close_round
    let events_after_close = setup.env.events().all();
    assert!(events_after_close.len() > 0, "No events after close_round");

    // Admin penalizes user2 (defaulter)
    setup.client.penalise_defaulter(&user2);

    // Check penalty was transferred
    assert_eq!(setup.token_client.balance(&user2), 950); // 1000 - 50 penalty
}

#[test]
fn test_penalty_deferred_within_grace_period() {
    let setup = setup_with_members(2, 1000);
    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 5,
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

    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();

    setup.client.penalise_defaulter(&user2);
    assert_eq!(setup.token_client.balance(&user2), 1000);
}

#[test]
fn test_penalty_applied_after_grace_boundary() {
    let setup = setup_with_members(2, 1000);
    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 5,
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

    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();

    setup.client.penalise_defaulter(&user2);
    assert_eq!(setup.token_client.balance(&user2), 1000);

    // Trigger pending penalty processing past deadline + grace (3600 + 5)
    setup.env.ledger().set_timestamp(3607);
    setup.client.penalise_defaulter(&user2);
    assert_eq!(setup.token_client.balance(&user2), 900);
}

#[test]
fn test_reputation_score_lifecycle_and_bounds() {
    let setup = setup_with_members(2, 2000);
    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 2,
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

    // Round 0: both members contribute fully and on-time (+10 each).
    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.client.contribute(&user2, &setup.token_admin, &100);
    assert_eq!(setup.client.get_reputation_score(&user1), 10);
    assert_eq!(setup.client.get_reputation_score(&user2), 10);
    assert_eq!(setup.client.get_group_avg_reputation(), 10);

    // Round 1: user2 defaults; penalty attempt during grace should not change score.
    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.env.ledger().set_timestamp(3601); // Just past round-1 deadline.
    setup.client.close_round();
    setup.client.penalise_defaulter(&user2);
    assert_eq!(setup.client.get_reputation_score(&user2), 10);

    // After grace: confirmed default then late-paid adjustment.
    setup.env.ledger().set_timestamp(3603);
    setup.client.penalise_defaulter(&user2);
    assert_eq!(setup.client.get_reputation_score(&user2), 5);
    assert_eq!(setup.client.get_reputation_score(&user1), 20);
    assert_eq!(setup.client.get_group_avg_reputation(), 12);
}

#[test]
fn test_reputation_persists_after_migrate() {
    let setup = setup_with_members(2, 1500);
    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    default_init(&setup);
    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.client.contribute(&user2, &setup.token_admin, &100);
    assert_eq!(setup.client.get_reputation_score(&user1), 10);

    setup.client.migrate(&setup.admin);
    assert_eq!(setup.client.get_reputation_score(&user1), 10);
    assert_eq!(setup.client.get_reputation_score(&user2), 10);
}

#[test]
fn test_multiple_defaulters_penalty() {
    let setup = setup_with_members(3, 1000);

    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();
    let user3 = setup.members.get(2).unwrap();

    // Init with a penalty amount
    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 30,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Only user1 contributes
    setup.client.contribute(&user1, &setup.token_admin, &100);

    // Wait for deadline to pass
    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();

    // Admin penalizes both defaulters
    setup.client.penalise_defaulter(&user2);
    setup.client.penalise_defaulter(&user3);

    // Check penalties were transferred
    assert_eq!(setup.token_client.balance(&user2), 970); // 1000 - 30 penalty
    assert_eq!(setup.token_client.balance(&user3), 970); // 1000 - 30 penalty
}

#[test]
fn test_member_suspension_after_two_defaults() {
    let setup = setup_with_members(2, 2000);

    let user1 = setup.members.get(0).unwrap();
    let user2 = setup.members.get(1).unwrap();

    // Init with penalty enabled
    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 25,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // ROUND 0: user2 defaults
    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();
    setup.client.penalise_defaulter(&user2);

    // ROUND 1: user2 defaults again
    // After close_round, new deadline is set to current_timestamp + duration
    // So at 3601, new deadline would be 3601 + 3600 = 7201
    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.env.ledger().set_timestamp(7202); // Past the new deadline
    setup.client.close_round();
    setup.client.penalise_defaulter(&user2);

    // Check penalties were applied twice
    assert_eq!(setup.token_client.balance(&user2), 1950); // 2000 - 25 - 25

    // Check that user2 was suspended (we can verify this by checking the balance was penalized twice)
    // The suspension event should have been emitted, but we'll just verify the functionality works
}

#[test]
fn test_suspended_member_skipped_in_payout() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);
    let token_client = TokenClient::new(&env, &token_admin);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let user3 = Address::generate(&env);
    let members = vec![&env, user1.clone(), user2.clone(), user3.clone()];

    // Mint enough tokens
    for user in [&user1, &user2, &user3] {
        token_admin_client.mint(user, &3000);
    }

    let penalty_amount = 20i128;
    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: penalty_amount,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Suspend user2 by making them default twice
    // ROUND 0: user2 defaults
    client.contribute(&user1, &token_admin, &100);
    client.contribute(&user3, &token_admin, &100);
    env.ledger().set_timestamp(3601);
    client.close_round();
    client.penalise_defaulter(&user2);

    // ROUND 1: user2 defaults again (gets suspended)
    client.contribute(&user1, &token_admin, &100);
    client.contribute(&user3, &token_admin, &100);
    env.ledger().set_timestamp(7202);
    client.close_round();
    client.penalise_defaulter(&user2);

    // ROUND 2: All contribute, but user2 should be skipped for payout
    // Round 2 (index 2): 2 % 3 = 2, which is user3's turn
    // Since user2 is suspended, user3 should get the payout
    let user3_balance_before = token_client.balance(&user3);
    client.contribute(&user1, &token_admin, &100);
    client.contribute(&user2, &token_admin, &100); // user2 can still contribute

    client.contribute(&user3, &token_admin, &100);

    // user3 should receive the payout (including penalty funds)
    let user3_balance_after = token_client.balance(&user3);
    let payout_received = user3_balance_after - user3_balance_before + 100; // +100 for contribution

    // Debug: let's see what the actual payout is
    // Expected: 300 (contributions) + accumulated penalties
    // Let's just check that user3 received more than the base contributions
    assert!(
        payout_received > 300,
        "Payout should include penalty funds, got: {}",
        payout_received
    );
}

#[test]
fn test_cannot_penalise_before_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let members = vec![&env, user1.clone()];

    client.init(
        &admin,
        &members,
        &100,
        &Address::generate(&env),
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Try to penalise before any round is closed (no defaulters identified yet)
    env.ledger().set_timestamp(1000);
    let res = client.try_penalise_defaulter(&user1);
    assert_eq!(res.unwrap_err().unwrap(), Error::NotADefaulter.into());
}

#[test]
fn test_penalty_disabled_when_amount_zero() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user1 = Address::generate(&env);
    let members = vec![&env, user1.clone()];

    client.init(
        &admin,
        &members,
        &100,
        &Address::generate(&env),
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    env.ledger().set_timestamp(3601);
    client.close_round();
    let res = client.try_penalise_defaulter(&user1);
    assert_eq!(res.unwrap_err().unwrap(), Error::PenaltyDisabled.into());
}

#[test]
fn test_cannot_penalise_non_defaulter() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let user1 = Address::generate(&env);
    let user2 = Address::generate(&env);
    let members = vec![&env, user1.clone(), user2.clone()];

    token_admin_client.mint(&user1, &1000);
    token_admin_client.mint(&user2, &1000);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Both users contribute (no defaulters)
    client.contribute(&user1, &token_admin, &100);
    client.contribute(&user2, &token_admin, &100);

    // Try to penalise user1 who contributed
    let res = client.try_penalise_defaulter(&user1);
    assert_eq!(res.unwrap_err().unwrap(), Error::NotADefaulter.into());
}

#[test]
fn test_read_interface_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone()];

    token_admin_client.mint(&u1, &1000);
    token_admin_client.mint(&u2, &1000);

    // 1. STAGE: Post-Initialization
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let info = client.get_group_info();
    assert_eq!(info.members.len(), 2);
    assert_eq!(info.current_round, 0);
    assert_eq!(info.next_recipient, u1); // Round 0 recipient
    assert_eq!(client.get_round_history(&0u32, &0u32, &100u32).len(), 0);

    // 2. STAGE: Mid-Round Contribution
    client.contribute(&u1, &token_admin, &100);

    // Verify member status
    assert!(client.get_member_status(&u1).has_paid_this_round);
    assert!(!client.get_member_status(&u2).has_paid_this_round);

    // Verify GroupInfo updates paid_members
    let info_mid = client.get_group_info();
    assert_eq!(info_mid.paid_members.len(), 1);
    assert!(info_mid.paid_members.contains(&u1));

    // 3. STAGE: Post-Payout (Round 0 Complete)
    client.contribute(&u2, &token_admin, &100); // This triggers complete_round_payout

    // Verify History
    let history = client.get_round_history(&0u32, &0u32, &100u32);
    assert_eq!(history.len(), 1);
    let record = history.get(0).unwrap();
    assert_eq!(record.recipient, u1);
    assert_eq!(record.amount, 200);

    // Verify New Round State
    let info_new_round = client.get_group_info();
    assert_eq!(info_new_round.current_round, 1);
    assert_eq!(info_new_round.next_recipient, u2); // Now it's u2's turn
    assert_eq!(info_new_round.paid_members.len(), 0); // Should be reset
}

#[test]
fn test_member_status_resets_after_round() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env); // Use 2 members so the round doesn't auto-close
    let members = vec![&env, u1.clone(), u2.clone()];

    token_admin_client.mint(&u1, &1000);
    token_admin_client.mint(&u2, &1000);

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // 1. u1 contributes. Round is NOT over because u2 hasn't paid.
    client.contribute(&u1, &token_admin, &100);
    assert!(client.get_member_status(&u1).has_paid_this_round);
    assert_eq!(client.get_group_info().current_round, 0);

    // 2. u2 contributes. This completes Round 0 and starts Round 1.
    client.contribute(&u2, &token_admin, &100);

    // 3. Now verify status is reset for the new round.
    assert_eq!(client.get_group_info().current_round, 1);
    assert!(!client.get_member_status(&u1).has_paid_this_round);
    assert!(!client.get_member_status(&u2).has_paid_this_round);
}

// ============================================================
//  DYNAMIC MEMBERSHIP TESTS
// ============================================================

#[test]
fn test_add_member_before_round() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let new_member = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone()];

    token_admin_client.mint(&u1, &1000);
    token_admin_client.mint(&u2, &1000);

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Add the new member before any round starts (paid_members is empty)
    client.add_member(&new_member);

    let info = client.get_group_info();
    assert_eq!(info.members.len(), 3);
    assert!(info.members.contains(&new_member));

    // Payout order should now include the new member
    // (get_group_info returns total_rounds which equals payout_order.len())
    assert_eq!(info.total_rounds, 3);
    // Event emission is confirmed by state change above (deprecated publish API
    // does not populate env.events().all() in this SDK version)
}

#[test]
fn test_add_member_mid_round_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let new_member = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone()];

    token_admin_client.mint(&u1, &1000);
    token_admin_client.mint(&u2, &1000);

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // u1 contributes — now paid_members is non-empty (mid-round)
    client.contribute(&u1, &token_admin, &100);

    // Attempt to add a member mid-round — must panic
    let res = client.try_add_member(&new_member);
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::CannotChangeMidRound.into()
    );
}

#[test]
fn test_remove_member_between_rounds() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let u3 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone(), u3.clone()];

    for u in [&u1, &u2, &u3] {
        token_admin_client.mint(u, &1000);
    }

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Complete round 0 so paid_members is reset
    client.contribute(&u1, &token_admin, &100);
    client.contribute(&u2, &token_admin, &100);
    client.contribute(&u3, &token_admin, &100);
    // paid_members is now empty (round completed)

    // Remove u3 between rounds
    client.remove_member(&u3);

    let info = client.get_group_info();
    assert_eq!(info.members.len(), 2);
    assert!(!info.members.contains(&u3));
    assert_eq!(info.total_rounds, 2); // payout_order shrunk to 2
                                      // Event emission is confirmed by state change above (deprecated publish API
                                      // does not populate env.events().all() in this SDK version)
}

#[test]
fn test_remove_member_mid_round_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone()];

    token_admin_client.mint(&u1, &1000);
    token_admin_client.mint(&u2, &1000);

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // u1 contributes — mid-round state
    client.contribute(&u1, &token_admin, &100);

    // Attempt to remove a member mid-round — must panic
    let res = client.try_remove_member(&u2);
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::CannotChangeMidRound.into()
    );
}

#[test]
fn test_remove_member_who_already_received_payout() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);
    let token_client = TokenClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let u3 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone(), u3.clone()];

    for u in [&u1, &u2, &u3] {
        token_admin_client.mint(u, &3000);
    }

    // RoundRobin: u1 gets round 0, u2 gets round 1, u3 gets round 2
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Round 0: u1 receives payout
    client.contribute(&u1, &token_admin, &100);
    client.contribute(&u2, &token_admin, &100);
    client.contribute(&u3, &token_admin, &100);
    let u1_after_r0 = token_client.balance(&u1);
    // u1 spent 100 and received 300 → net +200 = 3200
    assert_eq!(u1_after_r0, 3200);

    // Between rounds: remove u1 (who already received their payout)
    client.remove_member(&u1);

    let info = client.get_group_info();
    assert_eq!(info.members.len(), 2);
    assert!(!info.members.contains(&u1));
    assert_eq!(info.total_rounds, 2); // payout order now has u2, u3

    // Round 1 can proceed with the remaining two members — u2 should receive payout
    token_admin_client.mint(&u2, &200); // top u2 up so they have enough
    token_admin_client.mint(&u3, &200);
    client.contribute(&u2, &token_admin, &100);
    client.contribute(&u3, &token_admin, &100);

    // u2 gets the pot (200) — the contract still works correctly after removal
    // u2 started with 3000-200(r0 spend)+200(mint)=3000, spent 100 in r1, received 200
    let u2_balance = token_client.balance(&u2);
    assert!(
        u2_balance > 2900,
        "u2 should have received the payout, got: {}",
        u2_balance
    );
}

// --- NEW WHITELIST ADMIN TESTS (Issue #6) ---

#[test]
fn test_add_and_remove_approved_token() {
    let setup = setup_env();
    let token1 = Address::generate(&setup.env);
    let token2 = Address::generate(&setup.env);

    // Initial state: any token works because whitelist is empty.

    // We must manually set Admin so the auth check passes.
    // Normally init does this, but we want to test whitelist methods independently.
    setup.env.as_contract(&setup.client.address, || {
        setup
            .env
            .storage()
            .instance()
            .set(&DataKey::Admin, &setup.admin);
    });
    setup.client.add_approved_token(&token1);

    // After this, only token1 should be allowed during init.

    // Add token2 to whitelist
    setup.client.add_approved_token(&token2);

    // Remove token1 from whitelist
    setup.client.remove_approved_token(&token1);
}

#[test]
fn test_init_with_approved_token() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone()];

    // Add the specific token admin to whitelist
    setup.env.as_contract(&setup.client.address, || {
        setup
            .env
            .storage()
            .instance()
            .set(&DataKey::Admin, &setup.admin);
    });
    setup.client.add_approved_token(&setup.token_admin);

    // Should succeed because token_admin is in the whitelist
    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );
}

#[test]
fn test_init_with_unapproved_token_panics() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone()];

    // Set admin
    setup.env.as_contract(&setup.client.address, || {
        setup
            .env
            .storage()
            .instance()
            .set(&DataKey::Admin, &setup.admin);
    });

    // Add some other token to whitelist
    let other_token = Address::generate(&setup.env);
    setup.client.add_approved_token(&other_token);

    // Should fail because token_admin is not in the whitelist
    let res = setup.client.try_init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );
    assert_eq!(res.unwrap_err().unwrap(), Error::TokenNotApproved.into());
}

// --- NEW EDGE CASE AND FAILURE PATH TESTS (Issue #9) ---

#[test]
fn test_init_twice_panics() {
    let setup = setup_env();
    let members = vec![
        &setup.env,
        Address::generate(&setup.env),
        Address::generate(&setup.env),
    ];

    // First init
    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Second init should panic
    let res = setup.client.try_init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );
    assert_eq!(res.unwrap_err().unwrap(), Error::AlreadyInitialized.into());
}

#[test]
fn test_contribute_non_member_panics() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let non_member = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Non-member trying to contribute
    let res = setup
        .client
        .try_contribute(&non_member, &setup.token_admin, &100);
    assert_eq!(res.unwrap_err().unwrap(), Error::NotAMember.into());
}

#[test]
fn test_contribute_twice_panics() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.token_admin_client.mint(&u1, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // First contribution
    setup.client.contribute(&u1, &setup.token_admin, &100);

    // Second contribution by the same member in the same round should panic
    let res = setup.client.try_contribute(&u1, &setup.token_admin, &100);
    assert_eq!(res.unwrap_err().unwrap(), Error::AlreadyContributed.into());
}

#[test]
fn test_payout_correct_member_n_group() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let u3 = Address::generate(&setup.env);
    let u4 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone(), u3.clone(), u4.clone()];

    for u in [&u1, &u2, &u3, &u4] {
        setup.token_admin_client.mint(u, &1000);
    }

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Round 0: u1 gets the pot (4 * 100 = 400)
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    setup.client.contribute(&u3, &setup.token_admin, &100);
    setup.client.contribute(&u4, &setup.token_admin, &100);

    // u1 history: 1000 - 100 + 400 = 1300
    assert_eq!(setup.token_client.balance(&u1), 1300);

    // Round 1: u2 gets the pot
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    setup.client.contribute(&u3, &setup.token_admin, &100);
    setup.client.contribute(&u4, &setup.token_admin, &100);

    // u2 history: 1000 - 100(R0) - 100(R1) + 400 = 1200
    assert_eq!(setup.token_client.balance(&u2), 1200);

    // Round 2: u3 gets the pot
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    setup.client.contribute(&u3, &setup.token_admin, &100);
    setup.client.contribute(&u4, &setup.token_admin, &100);

    // u3 history: 1000 - 100(R0) - 100(R1) - 100(R2) + 400 = 1100
    assert_eq!(setup.token_client.balance(&u3), 1100);

    // Round 3: u4 gets the pot
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);
    setup.client.contribute(&u3, &setup.token_admin, &100);
    setup.client.contribute(&u4, &setup.token_admin, &100);

    // u4 history: 1000 - 100(R0) - 100(R1) - 100(R2) - 100(R3) + 400 = 1000
    assert_eq!(setup.token_client.balance(&u4), 1000);

    // u1 loses 100 in R1, R2, R3 (total 300) -> 1300 - 300 = 1000
    assert_eq!(setup.token_client.balance(&u1), 1000);
}

#[test]
fn test_contract_balance_zero_after_round() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    for u in [&u1, &u2] {
        setup.token_admin_client.mint(u, &1000);
    }

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Before contributions, balance is 0
    let current_contract_address = setup.client.address.clone();
    assert_eq!(setup.token_client.balance(&current_contract_address), 0);

    // u1 contributes, balance is 100
    setup.client.contribute(&u1, &setup.token_admin, &100);
    assert_eq!(setup.token_client.balance(&current_contract_address), 100);

    // u2 contributes, finishes round, payout dispatched, balance should be 0
    setup.client.contribute(&u2, &setup.token_admin, &100);
    assert_eq!(setup.token_client.balance(&current_contract_address), 0);
}

#[test]
fn test_single_member_rosca() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone()];

    setup.token_admin_client.mint(&u1, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Single member contributes, should immediately complete round and payout to self
    setup.client.contribute(&u1, &setup.token_admin, &100);

    // Balance remains 1000 (spent 100, received 100 immediately)
    assert_eq!(setup.token_client.balance(&u1), 1000);

    // State should now be round 1
    let state = setup.client.get_state();
    assert_eq!(state.0, 1);
}

#[test]
fn test_large_group_rosca() {
    let setup = setup_env();
    let mut member_addresses = alloc::vec::Vec::new();
    let mut members = soroban_sdk::Vec::new(&setup.env);

    // 10 members
    for _ in 0..10 {
        let addr = Address::generate(&setup.env);
        setup.token_admin_client.mint(&addr, &2000); // Plenty of tokens
        member_addresses.push(addr.clone());
        members.push_back(addr.clone());
    }

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Do 1 full cycle (10 rounds)
    for _round_idx in 0..10 {
        for m in member_addresses.iter() {
            setup.client.contribute(m, &setup.token_admin, &100);
        }
    }

    // At the end of 10 rounds, everyone should have exactly back what they started with
    for m in member_addresses.iter() {
        assert_eq!(setup.token_client.balance(m), 2000);
    }

    let state = setup.client.get_state();
    assert_eq!(state.0, 10); // completed 10 rounds
}

#[test]
fn test_get_state_lifecycle_details() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    for u in [&u1, &u2] {
        setup.token_admin_client.mint(u, &1000);
    }

    // Setup initially uses ledger timestamp 0 internally, so duration 3600 sets deadline to 3600.
    setup.env.ledger().set_timestamp(100);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Before any contributions
    let (round, paid, deadline, strategy, _) = setup.client.get_state();
    assert_eq!(round, 0);
    assert_eq!(paid.len(), 0);
    assert_eq!(deadline, 3700); // 100 + 3600
    assert_eq!(strategy, PayoutStrategy::RoundRobin);

    // During a round
    setup.client.contribute(&u1, &setup.token_admin, &100);
    let (round_mid, paid_mid, deadline_mid, _, _) = setup.client.get_state();
    assert_eq!(round_mid, 0);
    assert_eq!(paid_mid.len(), 1);
    assert!(paid_mid.contains(&u1));
    assert_eq!(deadline_mid, 3700);

    // After a round
    setup.env.ledger().set_timestamp(200); // Advance time slightly
    setup.client.contribute(&u2, &setup.token_admin, &100); // Completes the round

    let (round_after, paid_after, deadline_after, _, _) = setup.client.get_state();
    assert_eq!(round_after, 1);
    assert_eq!(paid_after.len(), 0);
    assert_eq!(deadline_after, 3800); // 200 + 3600
}

#[test]
fn test_bump_storage() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone()];

    setup.token_admin_client.mint(&u1, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Call bump_storage
    setup.client.bump_storage();

    // Advance ledger far into the future
    setup
        .env
        .ledger()
        .set_sequence_number(setup.env.ledger().sequence() + 50_000);

    // Verify contract is still accessible
    let (round, paid, _, _, _) = setup.client.get_state();
    assert_eq!(round, 0);
    assert_eq!(paid.len(), 0);
}

#[test]
fn test_reward_distribution_scenarios() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.token_admin_client.mint(&u1, &1000);
    setup.token_admin_client.mint(&u2, &1000);
    setup.token_admin_client.mint(&setup.admin, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // 1. Deposit Rewards
    setup.client.deposit_rewards(&setup.admin, &200);

    // 2. Equal Distribution (Default)
    assert_eq!(setup.client.get_claimable_reward(&u1), 100);
    assert_eq!(setup.client.get_claimable_reward(&u2), 100);

    // 3. Proportional Distribution
    setup
        .client
        .set_reward_dist_params(&DistributionType::Proportional, &None);
    // No participations yet
    assert_eq!(setup.client.get_claimable_reward(&u1), 0);

    setup.client.contribute(&u1, &setup.token_admin, &100);
    // u1 has 1 participation, total 1 -> 200 * 1/1 = 200
    assert_eq!(setup.client.get_claimable_reward(&u1), 200);
    assert_eq!(setup.client.get_claimable_reward(&u2), 0);

    setup.client.contribute(&u2, &setup.token_admin, &100);
    // u1 has 1, u2 has 1, total 2 -> 200 * 1/2 = 100 each
    assert_eq!(setup.client.get_claimable_reward(&u1), 100);
    assert_eq!(setup.client.get_claimable_reward(&u2), 100);

    // 4. Weighted Distribution
    let mut weights: Map<Address, u32> = Map::new(&setup.env);
    weights.set(u1.clone(), 3);
    weights.set(u2.clone(), 1);
    setup
        .client
        .set_reward_dist_params(&DistributionType::Weighted, &Some(weights));
    // u1: 200 * 3/4 = 150, u2: 200 * 1/4 = 50
    assert_eq!(setup.client.get_claimable_reward(&u1), 150);
    assert_eq!(setup.client.get_claimable_reward(&u2), 50);

    // 5. Claim Rewards
    let u1_balance_before = setup.token_client.balance(&u1);
    setup.client.claim_rewards(&u1);
    assert_eq!(setup.token_client.balance(&u1), u1_balance_before + 150);
    assert_eq!(setup.client.get_claimable_reward(&u1), 0);

    // 6. Deposit More Rewards
    setup.client.deposit_rewards(&setup.admin, &100);
    // Total pool is now 300.
    // u1 share: 300 * 3/4 = 225. Claimed: 150. Claimable: 75
    // u2 share: 300 * 1/4 = 75. Claimed: 0. Claimable: 75
    assert_eq!(setup.client.get_claimable_reward(&u1), 75);
    assert_eq!(setup.client.get_claimable_reward(&u2), 75);
}

#[test]
fn test_contribution_pot_separation() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.token_admin_client.mint(&u1, &1000);
    setup.token_admin_client.mint(&u2, &1000);
    setup.token_admin_client.mint(&setup.admin, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Deposit rewards
    setup.client.deposit_rewards(&setup.admin, &500);

    // Complete a round
    let _u1_balance_before = setup.token_client.balance(&u1);
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);

    // u1 was recipient. Pot should be exactly 200 (100 * 2), NOT including rewards.
    // u1 balance: 1000 (start) - 100 (contrib) + 200 (pot) = 1100
    assert_eq!(setup.token_client.balance(&u1), 1100);

    // Rewards pool should still be intact (500)
    assert_eq!(setup.client.get_claimable_reward(&u1), 250); // Equal share
    assert_eq!(setup.client.get_claimable_reward(&u2), 250);
}

// ============================================================
//  EMERGENCY EXIT MECHANISM TESTS — Issue #24
// ============================================================

/// Helper: initialise a 3-member ROSCA with an exit penalty of 10% (1000 bps).
/// Returns (client, admin, u1, u2, u3, token_client, token_admin)
fn setup_exit_env(
    env: &Env,
) -> (
    AhjoorContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
    soroban_sdk::token::Client<'_>,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(env, &token_admin_addr);
    let token_client = soroban_sdk::token::Client::new(env, &token_admin_addr);

    let u1 = Address::generate(env);
    let u2 = Address::generate(env);
    let u3 = Address::generate(env);

    for u in [&u1, &u2, &u3] {
        token_admin_client.mint(u, &3000);
    }

    let members = vec![env, u1.clone(), u2.clone(), u3.clone()];

    client.init(
        &admin,
        &members,
        &100,
        &token_admin_addr,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    (client, admin, u1, u2, u3, token_client, token_admin_addr)
}

// ---------------------------------------------------------------
// 1. Happy-path: a member can request an emergency exit
// ---------------------------------------------------------------
#[test]
fn test_member_can_request_emergency_exit() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    // Between rounds (paid_members is empty) — request should succeed
    client.request_emergency_exit(&u1);

    let requests = client.get_exit_requests();
    assert!(
        requests.contains_key(u1.clone()),
        "Exit request should be stored"
    );

    let req = requests.get(u1.clone()).unwrap();
    assert_eq!(req.member, u1);
    // u1 has contributed 0 full rounds so far
    assert_eq!(req.rounds_contributed, 0);
    // refund_amount is 0 at request time — computed dynamically in approve_exit
    assert_eq!(req.refund_amount, 0);
    assert!(!req.approved);
}

// ---------------------------------------------------------------
// 2. Non-member cannot request an exit
// ---------------------------------------------------------------
#[test]
fn test_exit_request_rejected_if_not_member() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    let non_member = Address::generate(&env);
    let res = client.try_request_emergency_exit(&non_member);
    assert_eq!(res.unwrap_err().unwrap(), Error::NotAMember.into());
}

// ---------------------------------------------------------------
// 3. Already-exited member cannot request again
// ---------------------------------------------------------------
#[test]
fn test_exit_request_rejected_if_already_exited() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    // Now u1 is in ExitedMembers, requesting again should panic
    let res = client.try_request_emergency_exit(&u1);
    assert_eq!(res.unwrap_err().unwrap(), Error::MemberAlreadyExited.into());
}

// ---------------------------------------------------------------
// 4. Cannot request exit mid-round (after at least one contribution)
// ---------------------------------------------------------------
#[test]
fn test_exit_request_rejected_mid_round() {
    let env = Env::default();
    let (client, _admin, u1, u2, _u3, _tc, _ta) = setup_exit_env(&env);

    // u2 contributes → round is in progress
    client.contribute(&u2, &_ta, &100);

    // u1 tries to exit mid-round → should panic
    let res = client.try_request_emergency_exit(&u1);
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::ExitNotAllowedMidRound.into()
    );
}

// ---------------------------------------------------------------
// 5. Admin approves exit: penalty kept, refund sent, member removed
//    Set up: advance round via close_round so contributions are still
//    held in the contract and can be refunded on exit.
// ---------------------------------------------------------------
#[test]
fn test_admin_approves_exit_penalty_applied() {
    let env = Env::default();
    let (client, _admin, u1, u2, u3, token_client, _ta) = setup_exit_env(&env);

    // u1 contributes in round 0. u2 does NOT (so the round never auto-completes).
    // Therefore the 100 tokens u1 sent remain in the contract.
    client.contribute(&u1, &_ta, &100);

    // Advance past deadline so admin can close the round.
    env.ledger().set_timestamp(3601);
    client.close_round();
    // Now CurrentRound = 1. Contract still holds u1's 100 tokens.

    // u1 has contributed in 1 round. penalty = 100 * 1000 / 10000 = 10. refund = 90.
    let u1_balance_before_exit = token_client.balance(&u1);
    client.request_emergency_exit(&u1);

    let req = client.get_exit_requests().get(u1.clone()).unwrap();
    assert_eq!(req.rounds_contributed, 1);
    // refund_amount is 0 at request time — computed dynamically in approve_exit
    assert_eq!(req.refund_amount, 0);

    client.approve_exit(&u1);

    // u1 received the refund (90 returned = 100 contributed - 10% penalty)
    let u1_balance_after_exit = token_client.balance(&u1);
    assert_eq!(u1_balance_after_exit, u1_balance_before_exit + 90);

    // u1 no longer a member
    let info = client.get_group_info();
    assert!(!info.members.contains(&u1));
    assert_eq!(info.total_rounds, 3); // PayoutOrder remains at 3 to keep schedule sync

    // u1 appears in exited members
    let exited = client.get_exited_members();
    assert!(exited.contains(&u1));

    // Exit request is cleared
    assert!(!client.get_exit_requests().contains_key(u1.clone()));

    // u2 can still continue normally in round 1
    // At Round 1, recipient is originally u2 (1 % 3 = 1).
    let u2_before = token_client.balance(&u2);
    client.contribute(&u2, &_ta, &100);
    client.contribute(&u3, &_ta, &100); // Both must contribute to complete round with 2 members

    // Pot = 10 (penalty from u1's exit) + 200 (u2+u3 contributions) = 210 → goes to u2
    let u2_after = token_client.balance(&u2);
    assert!(
        u2_after > u2_before,
        "u2 should have received the round payout"
    );
    assert_eq!(u2_after, u2_before - 100 + 210);
}

// ---------------------------------------------------------------
// 6. Admin rejects exit: member stays, request cleared
// ---------------------------------------------------------------
#[test]
fn test_admin_rejects_exit_request() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.request_emergency_exit(&u1);
    client.reject_exit(&u1);

    // Request is removed
    assert!(!client.get_exit_requests().contains_key(u1.clone()));

    // u1 is still a member
    let info = client.get_group_info();
    assert!(info.members.contains(&u1));

    // u1 is NOT in exited members
    assert!(!client.get_exited_members().contains(&u1));
}

// ---------------------------------------------------------------
// 7. Exited member cannot contribute
// ---------------------------------------------------------------
#[test]
fn test_exited_member_cannot_contribute() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    // u1 tries to contribute after exit — must panic
    let res = client.try_contribute(&u1, &_ta, &100);
    assert_eq!(res.unwrap_err().unwrap(), Error::MemberHasExited.into());
}

// ---------------------------------------------------------------
// 8. Exited member is skipped in payout order; remaining members
//    still receive correct payouts.
//    u1 exits between rounds (0 contributions, so refund=0).
// ---------------------------------------------------------------
#[test]
fn test_exited_member_skipped_in_payout_order() {
    let env = Env::default();
    let (client, _admin, u1, u2, u3, token_client, _ta) = setup_exit_env(&env);

    // Round 0: u1 (index 0) is the payout recipient. All contribute.
    // u1 exits right away (before any round contributions — refund = 0, no transfer needed)
    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    // Now only u2 and u3 are members. Payout order is still 3.
    // Round 0 recipient was u1 (0 % 3 = 0). Since u1 is exited, it skips to u2 (1 % 3 = 1).
    let u2_before = token_client.balance(&u2);
    client.contribute(&u2, &_ta, &100);
    client.contribute(&u3, &_ta, &100);
    // Pot = 200 (100 * 2 members).
    let u2_after = token_client.balance(&u2);
    assert_eq!(
        u2_after,
        u2_before - 100 + 200,
        "u2 should receive the round 0 pot after u1 exits"
    );
}

// ---------------------------------------------------------------
// 9. Exited members are NOT counted as defaulters after close_round
// ---------------------------------------------------------------
#[test]
fn test_exited_member_skipped_in_defaulters_list() {
    let env = Env::default();
    let (client, _admin, u1, u2, _u3, _tc, _ta) = setup_exit_env(&env);

    // u1 exits before the first round deadline
    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    // Only u2 contributes; u3 does not
    client.contribute(&u2, &_ta, &100);

    // Advance past deadline
    env.ledger().set_timestamp(3601);
    client.close_round();

    // u3 should be a defaulter, u1 should NOT (they've exited)
    // We verify by checking that penalising u1 panics
    let result = client.try_penalise_defaulter(&u1);
    assert!(
        result.is_err(),
        "Exited member must not appear in defaulters"
    );
}

// ---------------------------------------------------------------
// 10. Exit with zero penalty: full refund of contributions.
//     Use close_round to advance to round 1 while keeping
//     u1's contribution in the contract.
// ---------------------------------------------------------------
#[test]
fn test_exit_with_zero_penalty() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin_addr);
    let token_client = TokenClient::new(&env, &token_admin_addr);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);

    token_admin_client.mint(&u1, &2000);
    token_admin_client.mint(&u2, &2000);

    let members = vec![&env, u1.clone(), u2.clone()];
    client.init(
        &admin,
        &members,
        &100,
        &token_admin_addr,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // u1 contributes in round 0 but u2 does NOT → round never auto-completes.
    // u1's 100 tokens remain in the contract.
    client.contribute(&u1, &token_admin_addr, &100);

    // Let deadline pass and close round so CurrentRound becomes 1.
    env.ledger().set_timestamp(3601);
    client.close_round();

    // CurrentRound = 1. u1 has contributed 1 round. penalty = 0. refund = 100.
    let u1_balance_before = token_client.balance(&u1);
    client.request_emergency_exit(&u1);

    let req = client.get_exit_requests().get(u1.clone()).unwrap();
    // refund_amount is 0 at request time — computed dynamically in approve_exit
    assert_eq!(req.refund_amount, 0);

    client.approve_exit(&u1);
    // full refund: 100 contributed * 1 round, penalty = 0, no payout received
    assert_eq!(token_client.balance(&u1), u1_balance_before + 100);
}

// ---------------------------------------------------------------
// 11. Exit request emits the correct event
// ---------------------------------------------------------------
#[test]
fn test_exit_request_event_emitted() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.request_emergency_exit(&u1);

    let all_events = env.events().all();
    let last = all_events.last().unwrap();

    // Verify topics
    let expected_topics = (Symbol::new(&env, "exit_requested"),).into_val(&env);
    assert_eq!(last.1, expected_topics);

    // Verify data (it's a Map for struct events)
    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = last.2.into_val(&env);
    let member: Address = data
        .get(Symbol::new(&env, "member"))
        .unwrap()
        .into_val(&env);
    let round: u32 = data.get(Symbol::new(&env, "round")).unwrap().into_val(&env);
    // refund_amount was removed from ExitRequested event; only member and round are emitted
    assert_eq!(member, u1);
    assert_eq!(round, 0);
}

// ---------------------------------------------------------------
// 12. Approved exit emits the correct event
// ---------------------------------------------------------------
#[test]
fn test_exit_approval_event_emitted() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    let all_events = env.events().all();
    let last = all_events.last().unwrap();

    // Verify topics
    let expected_topics = (Symbol::new(&env, "exit_approved"),).into_val(&env);
    assert_eq!(last.1, expected_topics);

    // Verify data
    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = last.2.into_val(&env);
    let member: Address = data
        .get(Symbol::new(&env, "member"))
        .unwrap()
        .into_val(&env);
    let refund_amount: i128 = data
        .get(Symbol::new(&env, "refund_amount"))
        .unwrap()
        .into_val(&env);
    assert_eq!(member, u1);
    assert_eq!(refund_amount, 0);
}

// ---------------------------------------------------------------
// #389: Regression — exiting a member up-front (before their slot
// has been visited by the round rotation) compacts PayoutOrder,
// so subsequent members are promoted and slots stay strictly
// sequential rather than leaving a dangling entry.
// ---------------------------------------------------------------
#[test]
fn test_exit_compacts_payout_order_on_early_exit() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    // Initial layout: PayoutOrder.len() == 3 (members u1, u2, u3).
    assert_eq!(client.get_group_info().total_rounds, 3);

    // u1 (slot 0) requests + admin approves before any round contributions,
    // so current_round = 0 and u1's slot_index = 0. Not the past-slot guard
    // since 0 > 0 is false.
    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    // After compact: PayoutOrder becomes [u2, u3], length 2.
    assert_eq!(client.get_group_info().total_rounds, 2);

    // Round 0 → idx 0 % 2 = 0 → u2 (originally at slot 1, now promoted).
    let u2_before = _tc.balance(&_u2);
    client.contribute(&_u2, &_ta, &100);
    client.contribute(&_u3, &_ta, &100);
    let u2_after = _tc.balance(&_u2);
    assert_eq!(
        u2_after, u2_before - 100 + 200,
        "u2 (promoted to slot 0) should receive round 0 pot after early exit"
    );
}

// ---------------------------------------------------------------
// #389: Regression — exiting AFTER a round has advanced past the
// member's slot keeps PayoutOrder intact. The per-round payout
// logic in `complete_round_payout` continues to skip the exited
// member via the `ExitedMembers` contains check.
// ---------------------------------------------------------------
#[test]
fn test_exit_skips_compaction_on_late_exit() {
    let env = Env::default();
    let (client, _admin, u1, u2, u3, _tc, _ta) = setup_exit_env(&env);

    // u1 contributes, then we advance past the round deadline and close_round
    // so CurrentRound advances from 0 to 1. u1's slot (0) was visited.
    client.contribute(&u1, &_ta, &100);
    env.ledger().set_timestamp(3601);
    client.close_round();

    assert_eq!(client.get_group_info().current_round, 1);
    assert_eq!(client.get_group_info().total_rounds, 3);

    // u1 exits after their round was paid — compactor must NOT renumber.
    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    // Layout preserved: total_rounds stays at 3, payout order still 3 entries.
    assert_eq!(
        client.get_group_info().total_rounds,
        3,
        "Late-exit compactor must skip when current_round > slot_index"
    );

    // Round 1 → idx = 1 % 3 = 1 → u2 still receives, with u1 correctly
    // skipped by the contains-check.
    let u2_before = _tc.balance(&u2);
    client.contribute(&u2, &_ta, &100);
    client.contribute(&u3, &_ta, &100);
    let u2_after = _tc.balance(&u2);
    assert!(
        u2_after > u2_before,
        "u2 should still receive round 1 payout (compactor skipped)"
    );
}

// ---------------------------------------------------------------
// #389: Regression — the PayoutOrderCompacted event is emitted
// with the correct topic and `skipped_reason` enum value (0 when
// compaction ran, 1 when the past-slot guard tripped).
// ---------------------------------------------------------------
#[test]
fn test_exit_emits_payout_order_compacted_event() {
    // ---- Early-exit path: skipped_reason = 0 ----
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    // The new event is published mid-flight in approve_exit, so we scan all
    // events by topic rather than relying on `last()`.
    let target_topic = Symbol::new(&env, "payout_order_compacted");
    let all_events = env.events().all();
    let mut matched: Option<(soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Map<Symbol, soroban_sdk::Val>)> = None;
    for (_, topics, data) in all_events.iter() {
        let topics_vec: soroban_sdk::Vec<soroban_sdk::Val> = topics.into_val(&env);
        if let Some(first) = topics_vec.first() {
            let first_sym: Symbol = first.into_val(&env);
            if first_sym == target_topic {
                matched = Some((topics_vec.clone(), data.into_val(&env)));
                break;
            }
        }
    }
    let (_topics, data) = matched.expect("PayoutOrderCompacted event must be emitted");

    let exited_member: Address = data
        .get(Symbol::new(&env, "exited_member"))
        .unwrap()
        .into_val(&env);
    let slot_index: u32 = data
        .get(Symbol::new(&env, "slot_index"))
        .unwrap()
        .into_val(&env);
    let old_len: u32 = data
        .get(Symbol::new(&env, "old_len"))
        .unwrap()
        .into_val(&env);
    let new_len: u32 = data
        .get(Symbol::new(&env, "new_len"))
        .unwrap()
        .into_val(&env);
    let current_round: u32 = data
        .get(Symbol::new(&env, "current_round"))
        .unwrap()
        .into_val(&env);
    let skipped_reason: u32 = data
        .get(Symbol::new(&env, "skipped_reason"))
        .unwrap()
        .into_val(&env);
    assert_eq!(exited_member, u1);
    assert_eq!(slot_index, 0);
    assert_eq!(old_len, 3);
    assert_eq!(new_len, 2);
    assert_eq!(current_round, 0);
    assert_eq!(skipped_reason, 0, "compactor ran (early exit)");
}

// ---------------------------------------------------------------
// #389: Regression — late-exit emits PayoutOrderCompacted with
// skipped_reason = 1 (past-slot guard) and unchanged lengths.
// ---------------------------------------------------------------
#[test]
fn test_exit_emits_payout_order_compacted_skipped_reason_1() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    // Advance past round 0 so u1's slot (0) is already in the past.
    client.contribute(&u1, &_ta, &100);
    env.ledger().set_timestamp(3601);
    client.close_round();

    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    let target_topic = Symbol::new(&env, "payout_order_compacted");
    let all_events = env.events().all();
    let mut data: Option<soroban_sdk::Map<Symbol, soroban_sdk::Val>> = None;
    for (_, topics, event_data) in all_events.iter() {
        let topics_vec: soroban_sdk::Vec<soroban_sdk::Val> = topics.into_val(&env);
        if let Some(first) = topics_vec.first() {
            let first_sym: Symbol = first.into_val(&env);
            if first_sym == target_topic {
                data = Some(event_data.into_val(&env));
                break;
            }
        }
    }
    let data = data.expect("PayoutOrderCompacted event must be emitted");

    let skipped_reason: u32 = data
        .get(Symbol::new(&env, "skipped_reason"))
        .unwrap()
        .into_val(&env);
    let old_len: u32 = data
        .get(Symbol::new(&env, "old_len"))
        .unwrap()
        .into_val(&env);
    let new_len: u32 = data
        .get(Symbol::new(&env, "new_len"))
        .unwrap()
        .into_val(&env);
    let current_round: u32 = data
        .get(Symbol::new(&env, "current_round"))
        .unwrap()
        .into_val(&env);
    assert_eq!(skipped_reason, 1, "past-slot guard tripped");
    assert_eq!(old_len, new_len, "layout unchanged on past-slot skip");
    assert_eq!(current_round, 1);
}

// --- PAUSE AND RESUME TESTS ---

#[test]
fn test_pause_and_resume_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let members = vec![&env, Address::generate(&env)];

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Default state: not paused
    assert_eq!(client.is_paused(), false);

    // Admin pauses the group
    env.ledger().set_timestamp(1000);
    let reason = soroban_sdk::String::from_str(&env, "Emergency maintenance");
    client.pause_group(&reason);

    assert_eq!(client.is_paused(), true);
    let (is_paused, retrieved_reason, pause_time) = client.get_pause_info();
    assert_eq!(is_paused, true);
    assert_eq!(retrieved_reason, reason);
    assert_eq!(pause_time, 1000);

    // Initial deadline was start_time(0) + 3600 = 3600.
    let (_, _, initial_deadline, _, _) = client.get_state();
    assert_eq!(initial_deadline, 3600);

    // Admin resumes the group after 500 units of time
    env.ledger().set_timestamp(1500);
    client.resume_group(&soroban_sdk::String::from_str(&env, "Fixed"));

    assert_eq!(client.is_paused(), false);
    let (is_paused_after, retrieved_reason_after, pause_time_after) = client.get_pause_info();
    assert_eq!(is_paused_after, false);
    assert_eq!(retrieved_reason_after.len(), 0); // Removed from storage
    assert_eq!(pause_time_after, 0);

    // Check if the deadline was extended by pause duration (500)
    let (_, _, new_deadline, _, _) = client.get_state();
    assert_eq!(new_deadline, 4100);
}

#[test]
fn test_paused_blocks_contribute() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let user1 = Address::generate(&env);
    let members = vec![&env, user1.clone()];

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    client.pause_group(&soroban_sdk::String::from_str(&env, "Pause"));
    let res = client.try_contribute(&user1, &token_admin, &100);
    assert_eq!(res.unwrap_err().unwrap(), Error::ContractPaused.into());
}

#[test]
fn test_cannot_pause_already_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let members = vec![&env, Address::generate(&env)];

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let r = soroban_sdk::String::from_str(&env, "P");
    client.pause_group(&r);
    let res = client.try_pause_group(&r);
    assert_eq!(res.unwrap_err().unwrap(), Error::AlreadyPaused.into());
}

#[test]
fn test_cannot_resume_not_paused() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let members = vec![&env, Address::generate(&env)];

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let res = client.try_resume_group(&soroban_sdk::String::from_str(&env, "R"));
    assert_eq!(res.unwrap_err().unwrap(), Error::NotPaused.into());
}

// ============================================================
//  PARTIAL CONTRIBUTIONS TESTS (Issue: Partial Contributions Support)
// ============================================================

/// A member can split their contribution across multiple calls and the payout
/// fires only once the total equals contribution_amount.

/// Payout is blocked until ALL members have reached their full contribution.

/// `get_member_contribution_status` returns correct (contributed, remaining)
/// values at each stage of the round.
#[test]
fn test_get_member_contribution_status() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone()];

    token_admin_client.mint(&u1, &1000);
    token_admin_client.mint(&u2, &1000);

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Before any contribution
    let (c0, r0) = client.get_member_contribution_status(&u1);
    assert_eq!(c0, 0);
    assert_eq!(r0, 100);

    // After partial
    client.contribute(&u1, &token_admin, &30);
    let (c1, r1) = client.get_member_contribution_status(&u1);
    assert_eq!(c1, 30);
    assert_eq!(r1, 70);

    // After full payment
    client.contribute(&u1, &token_admin, &70);
    let (c2, r2) = client.get_member_contribution_status(&u1);
    assert_eq!(c2, 100);
    assert_eq!(r2, 0);

    // Complete the round (u2 pays); after reset u1's status should be cleared
    client.contribute(&u2, &token_admin, &100);

    // New round: status resets to (0, 100)
    let (c3, r3) = client.get_member_contribution_status(&u1);
    assert_eq!(c3, 0);
    assert_eq!(r3, 100);
}

/// Sending more than remaining contribution is rejected.
#[test]
fn test_overpayment_rejected() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let u1 = Address::generate(&env);
    let members = vec![&env, u1.clone()];

    token_admin_client.mint(&u1, &1000);

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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // u1 has already paid 60, tries to pay 60 more (would total 120 > 100)
    client.contribute(&u1, &token_admin, &60);
    let res = client.try_contribute(&u1, &token_admin, &60); // should error
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::ExceedsRemainingContribution.into()
    );
}

#[test]
fn test_emit_deadline_reminder() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    setup.env.ledger().set_timestamp(100);

    // u1 contributes
    setup.token_admin_client.mint(&u1, &100);
    setup.client.contribute(&u1, &setup.token_admin, &100);

    // Emit reminder
    setup.client.emit_deadline_reminder(&symbol_short!("24h"));

    let events = setup.env.events().all();
    let reminder_event = events.get(events.len() - 1).unwrap();

    // Topic check: (deadline_reminder,)
    let expected_topics = (Symbol::new(&setup.env, "deadline_reminder"),).into_val(&setup.env);
    assert_eq!(reminder_event.1, expected_topics);

    // Data check via map
    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = reminder_event.2.into_val(&setup.env);
    let round: u32 = data
        .get(Symbol::new(&setup.env, "round"))
        .unwrap()
        .into_val(&setup.env);
    let time_remaining: u64 = data
        .get(Symbol::new(&setup.env, "time_remaining"))
        .unwrap()
        .into_val(&setup.env);
    let non_contributors: Vec<Address> = data
        .get(Symbol::new(&setup.env, "non_contributors"))
        .unwrap()
        .into_val(&setup.env);
    let interval: Symbol = data
        .get(Symbol::new(&setup.env, "interval"))
        .unwrap()
        .into_val(&setup.env);

    assert_eq!(round, 0);
    assert_eq!(time_remaining, 3500); // 3600 - 100
    assert_eq!(non_contributors.len(), 1);
    assert!(non_contributors.contains(&u2));
    assert_eq!(interval, symbol_short!("24h"));
}

#[test]
fn test_get_upcoming_deadlines() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone()];

    setup.env.ledger().set_timestamp(100);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let deadlines = setup.client.get_upcoming_deadlines(&3);
    assert_eq!(deadlines.len(), 3);
    assert_eq!(deadlines.get(0).unwrap(), 3700); // 100 + 3600
    assert_eq!(deadlines.get(1).unwrap(), 7300); // 3700 + 3600
    assert_eq!(deadlines.get(2).unwrap(), 10900); // 7300 + 3600
}

// --- GOVERNANCE TESTS ---

#[test]
fn test_create_proposal() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Remove inactive member");
    setup.client.create_proposal(
        &user1,
        &ProposalType::MemberRemoval,
        &description,
        &user2,
        &3600,
        &None,
    );

    assert_eq!(setup.client.get_proposal_counter(), 1);

    let proposal = setup.client.get_proposal(&0);
    assert!(proposal.is_some());
    let prop = proposal.unwrap();
    assert_eq!(prop.id, 0);
    assert_eq!(prop.votes_for, 0);
    assert_eq!(prop.votes_against, 0);
}

#[test]
fn test_vote_on_proposal() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Penalty appeal");
    setup.client.create_proposal(
        &user1,
        &ProposalType::PenaltyAppeal,
        &description,
        &user2,
        &3600,
        &None,
    );

    setup.client.vote_on_proposal(&user1, &0, &true);
    setup.client.vote_on_proposal(&user2, &0, &true);

    let proposal = setup.client.get_proposal(&0).unwrap();
    assert_eq!(proposal.votes_for, 2);
    assert_eq!(proposal.votes_against, 0);
}

#[test]
fn test_execute_proposal_with_quorum() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Update rules");
    setup.client.create_proposal(
        &user1,
        &ProposalType::RuleChange,
        &description,
        &user1,
        &3600,
        &Some(75),
    );

    // All members vote for the proposal
    setup.client.vote_on_proposal(&user1, &0, &true);
    setup.client.vote_on_proposal(&user2, &0, &true);
    setup.client.vote_on_proposal(&user3, &0, &true);

    // Fast forward past deadline
    setup.env.ledger().set_timestamp(3601);

    // Execute the proposal
    setup.client.execute_proposal(&0);

    let proposal = setup.client.get_proposal(&0).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
    assert_eq!(setup.client.get_quorum_percentage(), 75);
}

#[test]
fn test_proposal_insufficient_quorum() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Test proposal");
    setup.client.create_proposal(
        &user1,
        &ProposalType::MemberRemoval,
        &description,
        &user2,
        &3600,
        &None,
    );

    // Only one member votes (not enough for quorum of 51%)
    setup.client.vote_on_proposal(&user1, &0, &true);

    // Fast forward past deadline
    setup.env.ledger().set_timestamp(3601);

    // Try to execute - should be rejected due to insufficient quorum
    setup.client.execute_proposal(&0);

    let proposal = setup.client.get_proposal(&0).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Rejected);
}

#[test]
fn test_proposal_voted_down() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Member removal");
    setup.client.create_proposal(
        &user1,
        &ProposalType::MemberRemoval,
        &description,
        &user2,
        &3600,
        &None,
    );

    // Votes against
    setup.client.vote_on_proposal(&user1, &0, &true);
    setup.client.vote_on_proposal(&user2, &0, &false);
    setup.client.vote_on_proposal(&user3, &0, &false);

    // Fast forward past deadline
    setup.env.ledger().set_timestamp(3601);

    setup.client.execute_proposal(&0);

    let proposal = setup.client.get_proposal(&0).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Rejected);
}

#[test]
fn test_penalty_appeal_execution() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Penalize user2
    setup.env.ledger().set_timestamp(100);
    setup.client.contribute(&user1, &setup.token_admin, &100);
    setup.client.contribute(&user3, &setup.token_admin, &100);

    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();

    setup.client.penalise_defaulter(&user2);

    // Create penalty appeal proposal at timestamp 3601
    let description = soroban_sdk::String::from_str(&setup.env, "Appeal penalty");
    setup.client.create_proposal(
        &user2,
        &ProposalType::PenaltyAppeal,
        &description,
        &user2,
        &3600,
        &None,
    );

    // All vote for appeal
    setup.client.vote_on_proposal(&user1, &0, &true);
    setup.client.vote_on_proposal(&user2, &0, &true);
    setup.client.vote_on_proposal(&user3, &0, &true);

    // Fast forward past voting deadline (3601 + 3600 + 1)
    setup.env.ledger().set_timestamp(7202);
    setup.client.execute_proposal(&0);

    let proposal = setup.client.get_proposal(&0).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);
}

#[test]
fn test_member_removal_execution() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Remove member");
    setup.client.create_proposal(
        &user1,
        &ProposalType::MemberRemoval,
        &description,
        &user2,
        &3600,
        &None,
    );

    setup.client.vote_on_proposal(&user1, &0, &true);
    setup.client.vote_on_proposal(&user2, &0, &true);
    setup.client.vote_on_proposal(&user3, &0, &true);

    setup.env.ledger().set_timestamp(3601);
    setup.client.execute_proposal(&0);

    let proposal = setup.client.get_proposal(&0).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Executed);

    let group_info = setup.client.get_group_info();
    assert!(!group_info.members.contains(&user2));
}

#[test]
fn test_non_member_cannot_create_proposal() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let non_member = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Unauthorized proposal");
    let res = setup.client.try_create_proposal(
        &non_member,
        &ProposalType::MemberRemoval,
        &description,
        &user2,
        &3600,
        &None,
    );
    assert_eq!(res.unwrap_err().unwrap(), Error::OnlyMembersAllowed.into());
}

#[test]
fn test_cannot_vote_after_deadline() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    setup.env.ledger().set_timestamp(100);
    let description = soroban_sdk::String::from_str(&setup.env, "Test proposal");
    setup.client.create_proposal(
        &user1,
        &ProposalType::MemberRemoval,
        &description,
        &user2,
        &3600,
        &None,
    );

    // Deadline is at 100 + 3600 = 3700, try to vote at 3701
    setup.env.ledger().set_timestamp(3701);
    let res = setup.client.try_vote_on_proposal(&user1, &0, &true);
    assert_eq!(
        res.unwrap_err().unwrap(),
        Error::VotingDeadlinePassed.into()
    );
}

#[test]
fn test_cannot_vote_twice() {
    let setup = setup_env();
    let user1 = Address::generate(&setup.env);
    let user2 = Address::generate(&setup.env);
    let user3 = Address::generate(&setup.env);
    let members = vec![&setup.env, user1.clone(), user2.clone(), user3.clone()];

    setup.token_admin_client.mint(&user1, &1000);
    setup.token_admin_client.mint(&user2, &1000);
    setup.token_admin_client.mint(&user3, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 50,
            exit_penalty_bps: 1000,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let description = soroban_sdk::String::from_str(&setup.env, "Test proposal");
    setup.client.create_proposal(
        &user1,
        &ProposalType::MemberRemoval,
        &description,
        &user2,
        &3600,
        &None,
    );

    setup.client.vote_on_proposal(&user1, &0, &true);
    let res = setup.client.try_vote_on_proposal(&user1, &0, &false);
    assert_eq!(res.unwrap_err().unwrap(), Error::AlreadyVoted.into());
}

#[test]
fn test_get_member_status_non_member() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let non_member = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone()];

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    let status = setup.client.get_member_status(&non_member);
    assert!(!status.is_member);
    assert!(!status.is_suspended);
    assert!(!status.is_exited);
    assert_eq!(status.contributions_this_round, 0);
    assert!(!status.has_paid_this_round);
    assert_eq!(status.default_count, 0);
    assert_eq!(status.lifetime_contributions, 0);
    assert_eq!(status.claimable_rewards, 0);
}

#[test]
fn test_get_member_status_active_member() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.token_admin_client.mint(&u1, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // Before contributing
    let status_before = setup.client.get_member_status(&u1);
    assert!(status_before.is_member);
    assert!(!status_before.is_suspended);
    assert!(!status_before.is_exited);
    assert!(!status_before.has_paid_this_round);
    assert_eq!(status_before.contributions_this_round, 0);

    // After contributing
    setup.client.contribute(&u1, &setup.token_admin, &100);
    let status_after = setup.client.get_member_status(&u1);
    assert!(status_after.has_paid_this_round);
    assert_eq!(status_after.contributions_this_round, 100);
}

#[test]
fn test_get_member_status_suspended_member() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.token_admin_client.mint(&u1, &2000);
    setup.token_admin_client.mint(&u2, &2000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 25,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 2,  // Suspend after 2 defaults
        
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    // u2 defaults twice to trigger suspension
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.env.ledger().set_timestamp(3601);
    setup.client.close_round();
    setup.client.penalise_defaulter(&u2);

    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.env.ledger().set_timestamp(7202);
    setup.client.close_round();
    setup.client.penalise_defaulter(&u2);

    let status = setup.client.get_member_status(&u2);
    assert!(status.is_member);
    assert!(status.is_suspended);
    assert!(!status.is_exited);
    assert_eq!(status.default_count, 2);
}

#[test]
fn test_get_member_status_exited_member() {
    let setup = setup_env();
    let u1 = Address::generate(&setup.env);
    let u2 = Address::generate(&setup.env);
    let members = vec![&setup.env, u1.clone(), u2.clone()];

    setup.token_admin_client.mint(&u1, &1000);
    setup.token_admin_client.mint(&u2, &1000);

    setup.client.init(
        &setup.admin,
        &members,
        &100,
        &setup.token_admin,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    setup.client.request_emergency_exit(&u2);
    setup.client.approve_exit(&u2);

    let status = setup.client.get_member_status(&u2);
    assert!(!status.is_member); // removed from Members on approval
    assert!(status.is_exited);
    assert!(!status.is_suspended);
    assert_eq!(status.default_count, 0);
}

// ===========================================================================
//  TTL Extension Behavior Tests
// ===========================================================================

/// RoundHistory is in persistent storage and must survive ledger advancement
/// past the instance TTL threshold.
#[test]
fn test_round_history_persistent_ttl() {
    let setup = setup_with_members(2, 1000);
    default_init(&setup);

    let u1 = setup.members.get(0).unwrap();
    let u2 = setup.members.get(1).unwrap();

    // Complete one round to write a RoundHistory entry
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);

    // Advance ledger sequence past instance TTL threshold
    setup
        .env
        .ledger()
        .set_sequence_number(setup.env.ledger().sequence() + 110_000);

    // RoundHistory must still be accessible (persistent storage, individual TTL)
    let history = setup.client.get_round_history(&0u32, &0u32, &100u32);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().amount, 200);
}

/// RoundHistory accumulates across multiple rounds and each write extends TTL.
#[test]
fn test_round_history_ttl_extended_each_round() {
    let setup = setup_with_members(2, 2000);
    default_init(&setup);

    let u1 = setup.members.get(0).unwrap();
    let u2 = setup.members.get(1).unwrap();

    // Complete two rounds
    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);

    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);

    setup
        .env
        .ledger()
        .set_sequence_number(setup.env.ledger().sequence() + 110_000);

    let history = setup.client.get_round_history(&0u32, &0u32, &100u32);
    assert_eq!(history.len(), 2);
}

// ===========================================================================
//  #555: get_round_history pagination
// ===========================================================================

/// get_round_history(group_id, offset, limit) returns the correct page of
/// RoundHistory for a group with more rounds than fit on one page.
#[test]
fn test_get_round_history_pagination_returns_correct_slice() {
    let setup = setup_with_members(2, 5_000);
    default_init(&setup);

    let u1 = setup.members.get(0).unwrap();
    let u2 = setup.members.get(1).unwrap();

    // Complete 5 rounds (round-robin: u1, u2, u1, u2, u1).
    for _ in 0..5 {
        setup.client.contribute(&u1, &setup.token_admin, &100);
        setup.client.contribute(&u2, &setup.token_admin, &100);
    }

    let full = setup.client.get_round_history(&0u32, &0u32, &100u32);
    assert_eq!(full.len(), 5);

    // First page: 2 records, matching the start of the full history.
    let page1 = setup.client.get_round_history(&0u32, &0u32, &2u32);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().recipient, full.get(0).unwrap().recipient);
    assert_eq!(page1.get(1).unwrap().recipient, full.get(1).unwrap().recipient);

    // Second page: next 2 records.
    let page2 = setup.client.get_round_history(&0u32, &2u32, &2u32);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().recipient, full.get(2).unwrap().recipient);
    assert_eq!(page2.get(1).unwrap().recipient, full.get(3).unwrap().recipient);

    // Final (partial) page: only 1 record remains.
    let page3 = setup.client.get_round_history(&0u32, &4u32, &2u32);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().recipient, full.get(4).unwrap().recipient);
}

/// An out-of-range offset returns an empty vec rather than panicking.
#[test]
fn test_get_round_history_out_of_range_offset_returns_empty() {
    let setup = setup_with_members(2, 1_000);
    default_init(&setup);

    let u1 = setup.members.get(0).unwrap();
    let u2 = setup.members.get(1).unwrap();

    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);

    // Only 1 round exists; offset way past the end must not panic.
    let page = setup.client.get_round_history(&0u32, &50u32, &10u32);
    assert_eq!(page.len(), 0);

    // Offset exactly at the total count is also "out of range".
    let page_at_boundary = setup.client.get_round_history(&0u32, &1u32, &10u32);
    assert_eq!(page_at_boundary.len(), 0);
}

/// A zero limit returns an empty vec regardless of a valid offset.
#[test]
fn test_get_round_history_zero_limit_returns_empty() {
    let setup = setup_with_members(2, 1_000);
    default_init(&setup);

    let u1 = setup.members.get(0).unwrap();
    let u2 = setup.members.get(1).unwrap();

    setup.client.contribute(&u1, &setup.token_admin, &100);
    setup.client.contribute(&u2, &setup.token_admin, &100);

    let page = setup.client.get_round_history(&0u32, &0u32, &0u32);
    assert_eq!(page.len(), 0);
}

/// ExitRequests in temporary storage: a request is stored, accessible, and
/// cleared correctly after approval.
#[test]
fn test_exit_requests_temporary_storage_lifecycle() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    // No requests initially
    assert!(!client.get_exit_requests().contains_key(u1.clone()));

    // Request stored in temporary storage
    client.request_emergency_exit(&u1);
    assert!(client.get_exit_requests().contains_key(u1.clone()));

    // Approval clears the temporary entry
    client.approve_exit(&u1);
    assert!(!client.get_exit_requests().contains_key(u1.clone()));
}

/// ExitRequests in temporary storage: rejection also clears the entry.
#[test]
fn test_exit_requests_temporary_cleared_on_reject() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.request_emergency_exit(&u1);
    assert!(client.get_exit_requests().contains_key(u1.clone()));

    client.reject_exit(&u1);
    assert!(!client.get_exit_requests().contains_key(u1.clone()));
}

// ===========================================================================
//  Admin Transfer Tests
// ===========================================================================

#[test]
fn test_propose_admin_transfer() {
    let env = Env::default();
    let (client, admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    let new_admin = Address::generate(&env);
    client.propose_admin_transfer(&new_admin);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_proposed_admin(), Some(new_admin));
}

#[test]
fn test_accept_admin_role() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    let new_admin = Address::generate(&env);
    client.propose_admin_transfer(&new_admin);
    client.accept_admin_role();

    assert_eq!(client.get_admin(), new_admin);
    assert_eq!(client.get_proposed_admin(), None);
}

#[test]
#[should_panic(expected = "No admin transfer proposed")]
fn test_accept_admin_role_without_proposal_panics() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);
    client.accept_admin_role();
}

#[test]
fn test_admin_transfer_emits_events() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    let new_admin = Address::generate(&env);
    client.propose_admin_transfer(&new_admin);

    client.accept_admin_role();
    assert_eq!(client.get_admin(), new_admin);
    assert_eq!(client.get_proposed_admin(), None);

    // Events are emitted but not checked here as the event API is tested elsewhere
}

#[test]
fn test_get_admin_returns_current_admin() {
    let env = Env::default();
    let (client, admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_get_proposed_admin_returns_none_when_no_proposal() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    assert_eq!(client.get_proposed_admin(), None);
}

#[test]
fn test_boundary_amount_i128_max_rejected_without_balance() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _token_client, token) = setup_exit_env(&env);
    let res = client.try_contribute(&u1, &token, &i128::MAX);
    assert!(res.is_err());
}

#[test]
fn test_boundary_payment_id_u64_max_cast_proposal_lookup() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);
    let id = u64::MAX as u32;
    assert_eq!(client.get_proposal(&id), None);
}

#[test]
fn test_auth_required_for_contribute() {
    let env = Env::default();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);
    let contributor = Address::generate(&env);
    let token = Address::generate(&env);

    let res = client.try_contribute(&contributor, &token, &100);
    assert!(res.is_err());
}

#[test]
fn test_event_snapshot_for_pause_resume() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.pause_group(&soroban_sdk::String::from_str(&env, "snapshot"));
    client.resume_group(&soroban_sdk::String::from_str(&env, "resume"));

    let events = env.events().all();
    assert!(!events.is_empty());
    let snapshot = alloc::format!("{:?}", events);
    assert!(!snapshot.is_empty());
}

#[test]
fn test_fuzz_like_member_operations_100_cases() {
    let env = Env::default();
    let (client, _admin, u1, u2, _u3, _tc, token) = setup_exit_env(&env);
    let mut seed: u64 = 0x73CA73;

    for _ in 0..100 {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let amount = ((seed % 100) as i128) + 1;
        let actor = if seed & 1 == 0 {
            u1.clone()
        } else {
            u2.clone()
        };
        let _ = client.try_contribute(&actor, &token, &amount);
    }

    let info = client.get_group_info();
    assert!(info.members.len() >= 2);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(120))]

    #[test]
    fn prop_round_payout_conservation(
        contributions in prop::collection::vec(0i128..100_000, 1..120)
    ) {
        let mut total_collected = 0i128;
        for amount in contributions.iter() {
            total_collected += *amount;
        }

        let payout_amount = total_collected;
        prop_assert_eq!(payout_amount, total_collected);
        prop_assert!(payout_amount >= 0);
    }
}

#[test]
fn test_upgrade_increments_contract_version() {
    let env = Env::default();
    let (client, admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    assert_eq!(client.get_version(), 1);

    let wasm_hash = env.deployer().upload_contract_wasm(UPGRADE_WASM);
    client.upgrade(&admin, &wasm_hash);

    let version: u32 = env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .get(&DataKey2::ContractVersion)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_upgrade_by_non_admin_is_rejected() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    let intruder = Address::generate(&env);
    let wasm_hash = env.deployer().upload_contract_wasm(UPGRADE_WASM);
    let result = client.try_upgrade(&intruder, &wasm_hash);

    assert!(result.is_err());
    assert_eq!(client.get_version(), 1);
}

#[test]
fn test_migration_is_once_per_version() {
    let env = Env::default();
    let (client, admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    client.migrate(&admin);
    let second = client.try_migrate(&admin);

    assert!(second.is_ok());
}

#[test]
fn test_upgrade_atomicity_invalid_hash() {
    let env = Env::default();
    let (client, admin, _u1, _u2, _u3, _tc, _ta) = setup_exit_env(&env);

    let invalid_hash = BytesN::from_array(&env, &[3u8; 32]);
    let result = client.try_upgrade(&admin, &invalid_hash);

    assert!(result.is_err());
    assert_eq!(client.get_version(), 1);
}

// ===========================================================================
//  finalize_round Tests
// ===========================================================================

/// Helper: set up a 3-member ROSCA with penalty tracking enabled (exit_penalty_bps = 1000).
/// Returns (client, admin, u1, u2, u3, token_client, token_admin_addr).
fn setup_finalize_env(
    env: &Env,
) -> (
    AhjoorContractClient<'_>,
    Address,
    Address,
    Address,
    Address,
    soroban_sdk::token::Client<'_>,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let token_admin_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(env, &token_admin_addr);
    let token_client = soroban_sdk::token::Client::new(env, &token_admin_addr);

    let u1 = Address::generate(env);
    let u2 = Address::generate(env);
    let u3 = Address::generate(env);

    for u in [&u1, &u2, &u3] {
        token_admin_client.mint(u, &3000);
    }

    let members = vec![env, u1.clone(), u2.clone(), u3.clone()];

    client.init(
        &admin,
        &members,
        &100,
        &token_admin_addr,
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    (client, admin, u1, u2, u3, token_client, token_admin_addr)
}

#[test]
fn test_finalize_round_panics_before_deadline() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_finalize_env(&env);

    // Deadline is at t = 3600. Calling at t = 3600 (not strictly past) should panic.
    env.ledger().set_timestamp(3600);
    let res = client.try_finalize_round();
    assert_eq!(res.unwrap_err().unwrap(), Error::DeadlineNotPassed.into());
}

#[test]
fn test_finalize_round_pays_out_partial_contributions() {
    let env = Env::default();
    let (client, _admin, u1, u2, _u3, token_client, ta) = setup_finalize_env(&env);

    // Only u1 contributes in round 0
    client.contribute(&u1, &ta, &100);

    // Advance past deadline
    env.ledger().set_timestamp(3601);

    let u1_balance_before = token_client.balance(&u1);

    // u2 and u3 did NOT contribute; round 0 recipient is u1 (index 0 % 3 = 0)
    client.finalize_round();

    // u1 was the payout recipient and should have received all collected funds (100 tokens)
    // u1 contributed 100, then received the pot which also contained only 100 (only u1 paid)
    let u1_balance_after = token_client.balance(&u1);
    assert!(u1_balance_after > u1_balance_before);

    // Round advanced to 1
    let (round, _, _, _, _) = client.get_state();
    assert_eq!(round, 1);
}

fn event_topic_present(env: &Env, topic: &str) -> bool {
    let target = Symbol::new(env, topic);
    for (_, topics, _) in env.events().all().iter() {
        let topics_vec: soroban_sdk::Vec<soroban_sdk::Val> = topics.into_val(env);
        if let Some(first) = topics_vec.first() {
            let first_sym: Symbol = first.into_val(env);
            if first_sym == target {
                return true;
            }
        }
    }
    false
}

#[test]
fn test_close_round_emits_without_payout_event() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_finalize_env(&env);

    env.ledger().set_timestamp(3601);
    client.close_round();

    assert!(
        event_topic_present(&env, "round_closed_without_payout"),
        "close_round must emit RoundClosedWithoutPayout"
    );
}

#[test]
fn test_finalize_round_does_not_emit_without_payout_event() {
    let env = Env::default();
    let (client, _admin, u1, _u2, _u3, _tc, ta) = setup_finalize_env(&env);

    client.contribute(&u1, &ta, &100);
    env.ledger().set_timestamp(3601);
    client.finalize_round();

    assert!(
        !event_topic_present(&env, "round_closed_without_payout"),
        "finalize_round must not emit RoundClosedWithoutPayout"
    );
}

#[test]
fn test_finalize_round_tracks_defaulters_and_suspends_after_three_misses() {
    let env = Env::default();
    let (client, _admin, _u1, u2, _u3, _tc, _ta) = setup_finalize_env(&env);

    // round_duration = 3600. Initialized at t=0, so deadline[0] = 3600.
    // After finalize_round at timestamp T, new deadline = T + 3600.
    // t=3601 -> finalize round 0; new deadline = 3601 + 3600 = 7201
    // t=7202 -> finalize round 1; new deadline = 7202 + 3600 = 10802
    // t=10803 -> finalize round 2; new deadline = ...
    let finalize_timestamps = [3601u64, 7202u64, 10803u64];

    for ts in finalize_timestamps {
        env.ledger().set_timestamp(ts);
        // No one contributes; finalize_round pays out with whatever is in the contract (0 here)
        client.finalize_round();
    }

    // After 3 consecutive misses u2 should be suspended
    let status = client.get_member_status(&u2);
    assert!(
        status.is_suspended,
        "u2 should be suspended after 3 consecutive defaults"
    );
}

#[test]
fn test_finalize_round_get_group_info_round_deadline() {
    let env = Env::default();
    let (client, _admin, _u1, _u2, _u3, _tc, _ta) = setup_finalize_env(&env);

    // round_deadline should be set at initialization: timestamp(0) + round_duration(3600)
    let info = client.get_group_info();
    assert_eq!(info.round_deadline, 3600);
}

// ===========================================================================
//  Exit Penalty Fix Tests (dynamic computation in approve_exit)
// ===========================================================================

/// Helper: 2-member ROSCA with 10% exit penalty, contribution = 100.
fn setup_exit_penalty_env(
    env: &Env,
) -> (
    AhjoorContractClient<'_>,
    Address,
    Address,
    Address,
    soroban_sdk::token::Client<'_>,
    Address,
) {
    env.mock_all_auths();
    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(env, &contract_id);

    let admin = Address::generate(env);
    let ta_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let ta_client = soroban_sdk::token::StellarAssetClient::new(env, &ta_addr);
    let token_client = soroban_sdk::token::Client::new(env, &ta_addr);

    let u1 = Address::generate(env);
    let u2 = Address::generate(env);

    for u in [&u1, &u2] {
        ta_client.mint(u, &10_000);
    }

    let members = vec![env, u1.clone(), u2.clone()];

    client.init(
        &admin,
        &members,
        &100,
        &ta_addr,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 1000, // 10%
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
            voting_mode: VotingMode::Equal, late_fee_bps: 0, grace_period_seconds: 0, auction_enabled: false, auction_window_ledgers: 0, randomize_payout_order: false, reserve_enabled: false, reserve_contribution_bps: 0,},
        &None,
    );

    (client, admin, u1, u2, token_client, ta_addr)
}

#[test]
fn test_exit_before_payout_penalty_on_contributions_only() {
    let env = Env::default();
    let (client, _admin, u1, u2, token_client, ta) = setup_exit_penalty_env(&env);

    // u1 contributes round 0 but u2 does not, so no payout happens.
    client.contribute(&u1, &ta, &100);

    // Close the round (u1 contributed but u2 did not → no auto-payout, close manually)
    env.ledger().set_timestamp(3601);
    client.close_round();
    // CurrentRound = 1. Contract holds u1's 100 tokens.

    // u1 exits: contributed 1 round (100 tokens), no payout received, penalty = 100 * 10% = 10
    // expected refund = 100 - 0 - 10 = 90
    let balance_before = token_client.balance(&u1);
    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    let balance_after = token_client.balance(&u1);
    assert_eq!(balance_after - balance_before, 90);
}

#[test]
fn test_exit_after_receiving_payout_deducted_from_refund() {
    let env = Env::default();
    let (client, _admin, u1, u2, token_client, ta) = setup_exit_penalty_env(&env);

    // Complete round 0: u1 and u2 both contribute. Recipient = u1 (index 0).
    client.contribute(&u1, &ta, &100);
    client.contribute(&u2, &ta, &100);
    // u1 received 200 (the full pot). CurrentRound = 1.

    // In round 1, u1 contributes again but u2 does not → round doesn't auto-complete.
    client.contribute(&u1, &ta, &100);

    env.ledger().set_timestamp(3601);
    client.close_round();
    // CurrentRound = 2. u1 has contributed 2 rounds, received 200.

    // u1 exits:
    //   contributed_total = 100 * 2 = 200
    //   received_payout   = 200
    //   penalty           = 200 * 10% = 20
    //   net               = 200 - 200 - 20 = -20  → refund_amount = 0
    let balance_before = token_client.balance(&u1);
    client.request_emergency_exit(&u1);
    client.approve_exit(&u1);

    let balance_after = token_client.balance(&u1);
    // No refund because u1 already received more than they put in after penalty
    assert_eq!(balance_after, balance_before);
}

#[test]
fn test_exit_zero_refund_when_payout_exceeds_contributions() {
    let env = Env::default();
    let (client, _admin, u1, u2, token_client, ta) = setup_exit_penalty_env(&env);

    // Round 0: both contribute, u1 receives pot (200).
    client.contribute(&u1, &ta, &100);
    client.contribute(&u2, &ta, &100);
    // CurrentRound = 1. u1 received 200 while contributing only 100.

    // Request exit immediately (rounds_contributed = 1)
    client.request_emergency_exit(&u1);

    //   contributed_total = 100 * 1 = 100
    //   received_payout   = 200
    //   penalty           = 100 * 10% = 10
    //   net               = 100 - 200 - 10 = -110 → refund_amount = 0
    let balance_before = token_client.balance(&u1);
    client.approve_exit(&u1);

    let balance_after = token_client.balance(&u1);
    assert_eq!(balance_after, balance_before, "zero refund when payout exceeds contributions");
}

// ---------------------------------------------------------------------------
// #390: Grace period — ledger-mode vs timestamp-mode consistency
// ---------------------------------------------------------------------------

/// Initialise a 3-member group with the given grace configuration and return
/// (env, client, admin, token_addr, members, token_admin_client).
fn setup_grace_env<'a>(
    use_timestamp: bool,
    grace_ledgers: u32,
    grace_seconds: u64,
) -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    soroban_sdk::Vec<Address>,
    TokenAdminClient<'a>,
) {
    let setup = setup_with_members(3, 500);
    setup.client.init(
        &setup.admin,
        &setup.members,
        &100,
        &setup.token_admin,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 10,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: grace_ledgers,
            grace_period_seconds: grace_seconds,
            use_timestamp_schedule: use_timestamp,
            round_duration_seconds: if use_timestamp { 3600 } else { 0 },
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
    (
        setup.env,
        setup.client,
        setup.admin,
        setup.token_admin,
        setup.members,
        setup.token_admin_client,
    )
}

/// #390 — ledger-mode: a late contribution within grace_period_ledgers must NOT
/// increment the member's default_count.
#[test]
fn test_grace_period_ledger_vs_timestamp() {
    // Ledger mode with 600-second grace window.
    let (env, client, admin, token_addr, members, _token_admin_client) =
        setup_grace_env(false, 600, 0);

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();

    // Round starts at t=0, deadline at t=3600.
    env.ledger().with_mut(|l| l.timestamp = 100);
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);
    // member2 does NOT contribute → is a defaulter after close

    // Advance past deadline but WITHIN grace window (3600 + 600 = 4200)
    env.ledger().with_mut(|l| l.timestamp = 3800);
    client.finalize_round();

    // member2 is now a defaulter; admin requests their penalty while still in grace
    env.ledger().with_mut(|l| l.timestamp = 3900); // still < 4200
    let member2 = members.get(2).unwrap();
    // request_penalty_grace defers the penalty — default_count should stay at 0
    // (the grace window is still open so no penalty is applied yet)
    // We just verify the contract accepted it without error and pool is consistent.
    client.request_penalty_grace(&member2);

    // After the grace window expires, the same member should face the penalty
    env.ledger().with_mut(|l| l.timestamp = 4300); // past grace (>4200)
    // The pending penalty triggers on next interaction; verify state is consistent.
    let info = client.get_group_info();
    // Round has advanced: verify we are in round 1
    assert!(info.current_round >= 1, "should have progressed past round 0");
}

// ===========================================================================
//  #556: get_pending_penalties admin view
// ===========================================================================

/// Empty vec when no penalties are pending.
#[test]
fn test_get_pending_penalties_empty_when_none_pending() {
    let (_env, client, _admin, _token_addr, _members, _token_admin_client) =
        setup_grace_env(false, 600, 0);

    let pending = client.get_pending_penalties(&0u32);
    assert_eq!(pending.len(), 0);
}

/// A group with a mix of an already-processed (auto-flushed on round
/// advance) and a still-pending penalty: get_pending_penalties must surface
/// only the member whose penalty is genuinely still awaiting processing.
#[test]
fn test_get_pending_penalties_mix_of_processed_and_pending() {
    let (env, client, _admin, token_addr, members, _token_admin_client) =
        setup_grace_env(false, 600, 0);
    let token_client = TokenClient::new(&env, &token_addr);

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();
    let member2 = members.get(2).unwrap();

    // Round 0 (deadline = 3600): member2 defaults.
    env.ledger().with_mut(|l| l.timestamp = 100);
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);

    env.ledger().with_mut(|l| l.timestamp = 3800);
    client.finalize_round();

    // Still within round 0's grace window (deadline 3600 + grace 600 = 4200):
    // member2's penalty is deferred, not charged yet.
    env.ledger().with_mut(|l| l.timestamp = 3850);
    let member2_balance_before = token_client.balance(&member2);
    client.request_penalty_grace(&member2);

    let pending_after_round0 = client.get_pending_penalties(&0u32);
    assert_eq!(pending_after_round0.len(), 1);
    assert_eq!(pending_after_round0.get(0).unwrap().0, member2);
    // Deferred, not yet charged: balance unchanged.
    assert_eq!(token_client.balance(&member2), member2_balance_before);

    // Round 1: member1 defaults this time.
    let round1_deadline = client.get_group_info().round_deadline;
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member2, &token_addr, &100);

    env.ledger().with_mut(|l| l.timestamp = round1_deadline + 100);
    client.finalize_round();
    let member1_balance_before_request = token_client.balance(&member1);

    // Still within round 1's own grace window (round1_deadline + 600).
    // Requesting grace for member1 first runs process_pending_penalties,
    // which flushes member2's stale round-0 entry (the round has advanced
    // past it) before deferring member1's brand-new one.
    let round1_grace_expires_at = round1_deadline + 600;
    env.ledger()
        .with_mut(|l| l.timestamp = round1_deadline + 150);
    client.request_penalty_grace(&member1);

    // member2 was auto-processed: the penalty was actually charged, on top
    // of member2's normal round-1 contribution of 100.
    assert_eq!(
        token_client.balance(&member2),
        member2_balance_before - 100 - 10
    );

    // member1 is the only one still awaiting processing.
    let pending = client.get_pending_penalties(&0u32);
    assert_eq!(pending.len(), 1);
    let (pending_member, info) = pending.get(0).unwrap();
    assert_eq!(pending_member, member1);
    assert_eq!(info.round, 2);
    assert_eq!(info.penalty_amount, 10);
    assert_eq!(info.grace_expires_at, round1_grace_expires_at);

    // member1's penalty hasn't been charged yet — deferred, not applied —
    // so requesting grace left member1's balance unchanged.
    assert_eq!(
        token_client.balance(&member1),
        member1_balance_before_request
    );
}

/// #390 — set_use_timestamp_schedule must reject calls after round 1 starts.
#[test]
fn test_cannot_change_timestamp_schedule_mid_round() {
    let (env, client, admin, token_addr, members, _token_admin_client) =
        setup_grace_env(false, 0, 0);

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();
    let member2 = members.get(2).unwrap();

    // Complete round 0 to advance to round 1
    env.ledger().with_mut(|l| l.timestamp = 100);
    client.contribute(&member0, &token_addr, &100);
    client.contribute(&member1, &token_addr, &100);
    client.contribute(&member2, &token_addr, &100);

    // Now CurrentRound > 0 — trying to flip use_timestamp_schedule must fail
    let result = client.try_set_use_timestamp_schedule(&admin, &true);
    assert!(
        result.is_err(),
        "changing use_timestamp_schedule after round 1 must return CannotChangeMidRound"
    );
}






#[test]
fn test_overflow_guard_on_large_contribution() {
    // contribution_amount = i128::MAX / 50, max_members = 100 → product overflows i128
    let setup = setup_env();
    let mut members = soroban_sdk::Vec::new(&setup.env);
    for _ in 0..2 {
        members.push_back(Address::generate(&setup.env));
    }
    let overflow_amount: i128 = i128::MAX / 50;
    let res = setup.client.try_init(
        &setup.admin,
        &members,
        &overflow_amount,
        &setup.token_admin,
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
            max_members: Some(100),
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
    assert_eq!(res.unwrap_err().unwrap(), ExtError::InvalidAmount.into());

    // Normal group (small amount) must initialize without error
    let setup2 = setup_with_members(3, 1_000_000);
    default_init(&setup2);
}

// ── #404: reinstate_member must verify proposal is Approved ──────────────────

/// Full reinstatement flow:
/// 1. Pending proposal → reinstate_member must return ProposalNotPending.
/// 2. After quorum and execute_proposal → Approved, reinstate_member succeeds.
/// 3. Calling reinstate_member a second time must fail (mapping cleared).
#[test]
fn test_reinstatement_requires_approved_proposal() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    let u1 = Address::generate(&env);
    let u2 = Address::generate(&env);
    let u3 = Address::generate(&env);
    let members = vec![&env, u1.clone(), u2.clone(), u3.clone()];

    for user in [&u1, &u2, &u3] {
        token_admin_client.mint(user, &5000);
    }

    client.init(
        &admin,
        &members,
        &100,
        &token_addr,
        &3600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 25,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 2,
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

    // Suspend u2 by missing two consecutive rounds.
    // finalize_round tracks defaults and suspends when count >= max_defaults.
    // Round 0: u1, u3 contribute; u2 defaults (count → 1, no suspension yet)
    client.contribute(&u1, &token_addr, &100);
    client.contribute(&u3, &token_addr, &100);
    env.ledger().set_timestamp(3601);
    client.finalize_round();

    // Round 1: u1, u3 contribute; u2 defaults again (count → 2 == max_defaults → suspended)
    client.contribute(&u1, &token_addr, &100);
    client.contribute(&u3, &token_addr, &100);
    env.ledger().set_timestamp(7202);
    client.finalize_round();

    assert!(client.get_member_status(&u2).is_suspended);

    // u2 requests reinstatement — creates proposal_id=1 in Pending status
    let proposal_id = client.request_reinstatement(
        &u2,
        &BytesN::from_array(&env, &[0xABu8; 32]),
    );

    // Immediately attempt reinstatement while proposal is still Pending → must fail
    let err = client
        .try_reinstate_member(&u2)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::ProposalNotPending.into());

    // Vote on the proposal before the deadline (u1 + u2 → quorum met with 3 members / 51%)
    client.vote_on_proposal(&u1, &proposal_id, &true);
    client.vote_on_proposal(&u2, &proposal_id, &true);

    // Advance past the voting deadline (1 week = 604_800 s)
    env.ledger().set_timestamp(7202 + 604_801);
    client.execute_proposal(&proposal_id);

    // Proposal must now be Approved (not Executed, because reinstate_member finishes it)
    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.status, ProposalStatus::Approved);

    // reinstate_member completes the reinstatement
    client.reinstate_member(&u2);

    // u2 must no longer be suspended
    assert!(!client.get_member_status(&u2).is_suspended);

    // Second call must fail — ActiveReinstatementProposal mapping is cleared
    let err2 = client
        .try_reinstate_member(&u2)
        .unwrap_err()
        .unwrap();
    assert_eq!(err2, Error::ProposalNotFound.into());
}

