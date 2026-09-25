#![cfg(test)]
use super::*;
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{Address, Env};

fn base_config() -> RoscaConfig {
    RoscaConfig {
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
    }
}

fn setup_with_members<'a>(
    n: usize,
    auction_enabled: bool,
    auction_window_ledgers: u64,
) -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    soroban_sdk::Vec<Address>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        let addr = Address::generate(&env);
        token_admin_client.mint(&addr, &10_000);
        members.push_back(addr);
    }

    let mut config = base_config();
    config.auction_enabled = auction_enabled;
    config.auction_window_ledgers = auction_window_ledgers;

    client.init(&admin, &members, &100, &token_admin, &3600, &config, &None);

    (env, client, admin, token_admin, members)
}

// ─── #748: get_slot_swap ───────────────────────────────────────────────────

#[test]
fn test_get_slot_swap_tracks_status_transitions() {
    let (env, client, admin, token_addr, members) = setup_with_members(3, false, 0);

    // PayoutOrder for RoundRobin is the member list in order, so round_a/round_b
    // must be indices whose payout_order entry matches initiator/counterparty.
    let initiator = members.get(1).unwrap();
    let counterparty = members.get(2).unwrap();

    env.ledger().set_timestamp(100);

    let swap_id = client.request_slot_swap(&initiator, &1, &2, &counterparty);
    let swap = client.get_slot_swap(&swap_id);
    assert_eq!(swap.status, SlotSwapStatus::Pending);
    assert_eq!(swap.initiator, initiator);
    assert_eq!(swap.counterparty, counterparty);

    client.accept_slot_swap(&counterparty, &swap_id);
    let swap = client.get_slot_swap(&swap_id);
    // requires_admin defaults to false, so accept executes the swap directly.
    assert_eq!(swap.status, SlotSwapStatus::Executed);

    let _ = &admin;
    let _ = &token_addr;
}

#[test]
fn test_get_slot_swap_admin_required_flow() {
    let (env, client, admin, _token_addr, members) = setup_with_members(3, false, 0);

    let initiator = members.get(1).unwrap();
    let counterparty = members.get(2).unwrap();

    client.set_slot_swap_config(&admin, &true, &86_400);

    env.ledger().set_timestamp(100);
    let swap_id = client.request_slot_swap(&initiator, &1, &2, &counterparty);
    assert_eq!(
        client.get_slot_swap(&swap_id).status,
        SlotSwapStatus::Pending
    );

    client.accept_slot_swap(&counterparty, &swap_id);
    assert_eq!(
        client.get_slot_swap(&swap_id).status,
        SlotSwapStatus::Accepted
    );

    client.approve_slot_swap(&admin, &swap_id);
    assert_eq!(
        client.get_slot_swap(&swap_id).status,
        SlotSwapStatus::Executed
    );
}

#[test]
#[should_panic]
fn test_get_slot_swap_missing_id_panics() {
    let (_env, client, _admin, _token_addr, _members) = setup_with_members(3, false, 0);
    client.get_slot_swap(&999);
}

// ─── #749: get_auction_status ──────────────────────────────────────────────

#[test]
fn test_get_auction_status_disabled_by_default() {
    let (_env, client, _admin, _token_addr, _members) = setup_with_members(3, false, 0);
    let (enabled, window, open_until, round) = client.get_auction_status();
    assert_eq!(enabled, false);
    assert_eq!(window, 0);
    assert_eq!(open_until, 0);
    assert_eq!(round, 0);
}

#[test]
fn test_get_auction_status_reflects_open_window() {
    let (env, client, _admin, token_addr, members) = setup_with_members(3, true, 500);

    env.ledger().set_timestamp(100);
    for i in 0..3 {
        client.contribute(&members.get(i).unwrap(), &token_addr, &100);
    }
    env.ledger().set_timestamp(200);
    for i in 0..3 {
        client.contribute(&members.get(i).unwrap(), &token_addr, &100);
    }
    // Completing round 2 triggers reset to round 3 (cycle start for 3 members),
    // which opens the auction window.
    env.ledger().set_timestamp(300);
    for i in 0..3 {
        client.contribute(&members.get(i).unwrap(), &token_addr, &100);
    }

    let (enabled, window, open_until, round) = client.get_auction_status();
    assert_eq!(enabled, true);
    assert_eq!(window, 500);
    assert_eq!(open_until, 300 + 500);
    assert_eq!(round, 3);
}

// ─── #750: get_insurance_coverage_mode ─────────────────────────────────────

#[test]
fn test_get_insurance_coverage_mode_defaults_to_none() {
    let (_env, client, _admin, _token_addr, _members) = setup_with_members(3, false, 0);
    assert_eq!(
        client.get_insurance_coverage_mode(),
        InsuranceCoverageMode::None
    );
}

#[test]
fn test_get_insurance_coverage_mode_reflects_set_value() {
    let (_env, client, admin, _token_addr, _members) = setup_with_members(3, false, 0);

    client.set_insurance_coverage_mode(&admin, &InsuranceCoverageMode::Partial);
    assert_eq!(
        client.get_insurance_coverage_mode(),
        InsuranceCoverageMode::Partial
    );

    client.set_insurance_coverage_mode(&admin, &InsuranceCoverageMode::Full);
    assert_eq!(
        client.get_insurance_coverage_mode(),
        InsuranceCoverageMode::Full
    );
}

// ─── #751: get_quorum_for_type ─────────────────────────────────────────────

#[test]
fn test_get_quorum_for_type_custom_and_defaults() {
    let (_env, client, admin, _token_addr, _members) = setup_with_members(3, false, 0);

    // Global default quorum is 51% -> 5100 bps for types without an override.
    assert_eq!(client.get_quorum_for_type(&ProposalType::RuleChange), 5100);
    assert_eq!(
        client.get_quorum_for_type(&ProposalType::MemberRemoval),
        5100
    );

    client.set_quorum_per_type(&admin, &ProposalType::RuleChange, &7500);

    assert_eq!(client.get_quorum_for_type(&ProposalType::RuleChange), 7500);
    // Other types remain at the default.
    assert_eq!(
        client.get_quorum_for_type(&ProposalType::MemberRemoval),
        5100
    );
    assert_eq!(
        client.get_quorum_for_type(&ProposalType::PenaltyAppeal),
        5100
    );
}

// ─── #747: get_co_signer_window ────────────────────────────────────────────

#[test]
fn test_get_co_signer_window_defaults_and_reflects_set_value() {
    let (_env, client, admin, _token_addr, _members) = setup_with_members(3, false, 0);

    // No window configured yet -> falls back to the same default used
    // internally (0).
    assert_eq!(client.get_co_signer_window(), 0);

    client.set_co_signer_window(&admin, &500u32);
    assert_eq!(client.get_co_signer_window(), 500);
}

// ─── #752: get_round_duration_bounds / get_pending_round_duration ─────────

#[test]
fn test_round_duration_getters_before_and_after_set() {
    let (_env, client, admin, _token_addr, _members) = setup_with_members(3, false, 0);

    // Before any configuration: defaults used internally by
    // update_round_duration, and no pending change.
    assert_eq!(client.get_round_duration_bounds(), (60, u64::MAX));
    assert_eq!(client.get_pending_round_duration(), None);

    client.set_round_duration_bounds(&admin, &3600u64, &86400u64);
    assert_eq!(client.get_round_duration_bounds(), (3600, 86400));
    assert_eq!(client.get_pending_round_duration(), None);

    client.update_round_duration(&admin, &7200u64);
    assert_eq!(client.get_round_duration_bounds(), (3600, 86400));
    assert_eq!(client.get_pending_round_duration(), Some(7200));
}

// ─── #753: get_treasury_config ─────────────────────────────────────────────

#[test]
fn test_get_treasury_config_none_until_enabled() {
    let (_env, client, admin, _token_addr, _members) = setup_with_members(3, false, 0);

    assert_eq!(client.get_treasury_config(), None);

    let treasury_admin = admin.clone();
    client.enable_group_treasury(&admin, &treasury_admin);

    let config = client.get_treasury_config().unwrap();
    assert_eq!(config.treasury_admin, treasury_admin);
    assert_eq!(config.enabled, true);
}

// ─── #899: get_auto_close_enabled ──────────────────────────────────────────

#[test]
fn test_get_auto_close_enabled_default_and_after_set() {
    let (_env, client, _admin, _token_addr, _members) = setup_with_members(3, false, 0);

    // Default applies before configuration
    assert_eq!(client.get_auto_close_enabled(), false);

    client.set_auto_close_enabled(&true);
    assert_eq!(client.get_auto_close_enabled(), true);

    client.set_auto_close_enabled(&false);
    assert_eq!(client.get_auto_close_enabled(), false);
}

// ─── #904: get_reward_dist_params ──────────────────────────────────────────

#[test]
fn test_get_reward_dist_params_default_and_set() {
    let (env, client, _admin, _token, members) = setup_with_members(3, false, 0);

    // Default before configuration.
    assert_eq!(client.get_reward_dist_params(), (DistributionType::Equal, None));

    // Type only, no weights.
    client.set_reward_dist_params(&DistributionType::Proportional, &None);
    assert_eq!(
        client.get_reward_dist_params(),
        (DistributionType::Proportional, None)
    );

    // Type with weights.
    let mut weights = Map::new(&env);
    weights.set(members.get(0).unwrap(), 3u32);
    weights.set(members.get(1).unwrap(), 1u32);
    client.set_reward_dist_params(&DistributionType::Weighted, &Some(weights.clone()));
    assert_eq!(
        client.get_reward_dist_params(),
        (DistributionType::Weighted, Some(weights))
    );
}
