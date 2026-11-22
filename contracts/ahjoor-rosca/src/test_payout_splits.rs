#![cfg(test)]
use super::*;
use crate::errors::{Error, ExtError2, ExtError3};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

fn setup_with_members<'a>(
    n: usize,
) -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    Vec<Address>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let mut members = Vec::new(&env);
    for _ in 0..n {
        members.push_back(Address::generate(&env));
    }

    client.init(
        &admin,
        &members,
        &100,
        &token_addr,
        &3_600,
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

    (env, client, admin, token_addr, members)
}

fn split_pair(env: &Env, a: &Address, a_bps: u32, b: &Address, b_bps: u32) -> Vec<(Address, u32)> {
    let mut splits = Vec::new(env);
    splits.push_back((a.clone(), a_bps));
    splits.push_back((b.clone(), b_bps));
    splits
}

#[test]
fn test_set_and_clear_payout_splits() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let wallet = Address::generate(&env);
    let family = Address::generate(&env);
    let splits = split_pair(&env, &wallet, 6_000, &family, 4_000);

    assert_eq!(client.get_payout_splits(&member2), None);

    client.set_payout_splits(&0, &member2, &splits);
    assert_eq!(client.get_payout_splits(&member2), Some(splits));

    client.clear_payout_splits(&member2);
    assert_eq!(client.get_payout_splits(&member2), None);
}

#[test]
fn test_payout_splits_honoured_with_dust_to_primary() {
    let (env, client, _admin, token_addr, members) = setup_with_members(2);
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin = token::StellarAssetClient::new(&env, &token_addr);

    let member1 = members.get(0).unwrap();
    let member2 = members.get(1).unwrap();
    let wallet = Address::generate(&env);
    let family = Address::generate(&env);

    token_admin.mint(&member1, &1_000);
    token_admin.mint(&member2, &1_000);

    // Member2 is paid in round 1; split 200 into 33.33% / 66.67%.
    let splits = split_pair(&env, &wallet, 3_333, &family, 6_667);
    client.set_payout_splits(&0, &member2, &splits);

    // Round 0: member1 is paid directly.
    client.contribute(&member1, &token_addr, &100);
    client.contribute(&member2, &token_addr, &100);

    // Round 1: member2's payout is split; 200 * 3333 / 10000 = 66,
    // 200 * 6667 / 10000 = 133, leaving 1 unit of dust for member2.
    env.ledger().with_mut(|li| li.sequence_number += 1);
    client.contribute(&member1, &token_addr, &100);
    client.contribute(&member2, &token_addr, &100);

    assert_eq!(token_client.balance(&wallet), 66);
    assert_eq!(token_client.balance(&family), 133);
    assert_eq!(token_client.balance(&member2), 801);
}

#[test]
fn test_payout_splits_reject_non_member() {
    let (env, client, _admin, _token, _members) = setup_with_members(2);
    let outsider = Address::generate(&env);
    let recipient = Address::generate(&env);
    let splits = split_pair(&env, &recipient, 5_000, &Address::generate(&env), 5_000);

    let err = client
        .try_set_payout_splits(&0, &outsider, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::NotAMember.into());
}

#[test]
fn test_payout_splits_locked_during_payout_round() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member1 = members.get(0).unwrap();
    let recipient = Address::generate(&env);
    let splits = split_pair(&env, &recipient, 5_000, &Address::generate(&env), 5_000);

    let err = client
        .try_set_payout_splits(&0, &member1, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::BeneficiaryLocked.into());

    let err = client
        .try_clear_payout_splits(&member1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::BeneficiaryLocked.into());
}

#[test]
fn test_payout_splits_reject_empty() {
    let (_env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let splits: Vec<(Address, u32)> = Vec::new(&_env);

    let err = client
        .try_set_payout_splits(&0, &member2, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutSplit.into());
}

#[test]
fn test_payout_splits_reject_too_many_recipients() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();

    let mut splits: Vec<(Address, u32)> = Vec::new(&env);
    for _ in 0..11 {
        splits.push_back((Address::generate(&env), 1_000));
    }

    let err = client
        .try_set_payout_splits(&0, &member2, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutSplit.into());
}

#[test]
fn test_payout_splits_reject_bps_not_summing_to_10000() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let splits = split_pair(
        &env,
        &Address::generate(&env),
        5_000,
        &Address::generate(&env),
        4_999,
    );

    let err = client
        .try_set_payout_splits(&0, &member2, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutSplit.into());
}

#[test]
fn test_payout_splits_reject_zero_bps() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let splits = split_pair(
        &env,
        &Address::generate(&env),
        10_000,
        &Address::generate(&env),
        0,
    );

    let err = client
        .try_set_payout_splits(&0, &member2, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutSplit.into());
}

#[test]
fn test_payout_splits_reject_contract_recipient() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let splits = split_pair(
        &env,
        &client.address,
        5_000,
        &Address::generate(&env),
        5_000,
    );

    let err = client
        .try_set_payout_splits(&0, &member2, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutSplit.into());
}

#[test]
fn test_payout_splits_reject_duplicate_recipient() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let recipient = Address::generate(&env);
    let splits = split_pair(&env, &recipient, 5_000, &recipient, 5_000);

    let err = client
        .try_set_payout_splits(&0, &member2, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutSplit.into());
}

#[test]
fn test_splits_and_beneficiary_conflict_both_ways() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let beneficiary = Address::generate(&env);
    let splits = split_pair(
        &env,
        &Address::generate(&env),
        5_000,
        &Address::generate(&env),
        5_000,
    );

    // Splits active → single beneficiary rejected.
    client.set_payout_splits(&0, &member2, &splits);
    let err = client
        .try_set_payout_beneficiary(&member2, &beneficiary)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::PayoutSplitsBeneficiaryConflict.into());

    // Beneficiary active → splits rejected.
    client.clear_payout_splits(&member2);
    client.set_payout_beneficiary(&member2, &beneficiary);
    let err = client
        .try_set_payout_splits(&0, &member2, &splits)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::PayoutSplitsBeneficiaryConflict.into());
}
