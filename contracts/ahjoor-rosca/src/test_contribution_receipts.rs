#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

/// Helper to set up a test environment with `n` members.
fn setup_with_members<'a>(
    n: usize,
    mint_amount: i128,
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

    (
        env,
        client,
        admin,
        token_admin,
        token_client,
        token_admin_client,
        members,
    )
}

fn make_default_config() -> RoscaConfig {
    RoscaConfig {
        strategy: PayoutStrategy::RoundRobin,
        custom_order: None,
        penalty_amount: 10,
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

#[test]
fn test_contribution_receipt_data_structure() {
    let (_env, client, admin, token_admin, _token_client, _token_admin_client, members) =
        setup_with_members(3, 1000);

    assert_eq!(client.get_contribution_receipt_count(), 0);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &make_default_config(),
        &None,
    );

    assert_eq!(client.get_contribution_receipt_count(), 0);
}

#[test]
fn test_contribution_receipts_minted_on_finalize_round() {
    let (env, client, admin, token_admin, _token_client, _token_admin_client, members) =
        setup_with_members(3, 1000);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &make_default_config(),
        &None,
    );

    let m0 = members.get(0).unwrap();
    let m1 = members.get(1).unwrap();

    client.contribute(&m0, &token_admin, &100);
    client.contribute(&m1, &token_admin, &100);

    env.ledger().set_timestamp(3601);
    client.finalize_round();

    assert_eq!(client.get_contribution_receipt_count(), 2);
}

#[test]
fn test_get_contribution_receipt_and_lookup_by_member() {
    let (env, client, admin, token_admin, _token_client, _token_admin_client, members) =
        setup_with_members(3, 1000);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &make_default_config(),
        &None,
    );

    let m0 = members.get(0).unwrap();
    let m1 = members.get(1).unwrap();
    let m2 = members.get(2).unwrap();

    client.contribute(&m0, &token_admin, &100);
    client.contribute(&m1, &token_admin, &100);

    env.ledger().set_timestamp(3601);
    client.finalize_round();

    assert_eq!(client.get_contribution_receipt_count(), 2);

    let receipt0 = client.get_contribution_receipt(&0);
    assert_eq!(receipt0.receipt_id, 0);
    assert_eq!(receipt0.member, m0);
    assert_eq!(receipt0.round, 0);
    assert_eq!(receipt0.amount_contributed, 100);
    assert_eq!(receipt0.token, token_admin);
    assert_eq!(receipt0.minted_at, 3601);

    let receipt1 = client.get_contribution_receipt(&1);
    assert_eq!(receipt1.receipt_id, 1);
    assert_eq!(receipt1.member, m1);

    let m0_receipt_ids = client.get_member_receipt_ids(&m0);
    let m1_receipt_ids = client.get_member_receipt_ids(&m1);
    let m2_receipt_ids = client.get_member_receipt_ids(&m2);

    assert_eq!(m0_receipt_ids.len(), 1);
    assert_eq!(m0_receipt_ids.get(0).unwrap(), 0);

    assert_eq!(m1_receipt_ids.len(), 1);
    assert_eq!(m1_receipt_ids.get(0).unwrap(), 1);

    assert_eq!(m2_receipt_ids.len(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #116)")]
fn test_get_contribution_receipt_not_found() {
    let (_env, client, admin, token_admin, _token_client, _token_admin_client, members) =
        setup_with_members(3, 1000);

    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &make_default_config(),
        &None,
    );

    client.get_contribution_receipt(&999);
}
