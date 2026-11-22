#![cfg(test)]
use super::*;
use crate::errors::{Error, ExtError, ExtError2, ExtError3};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
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

#[test]
fn test_set_get_and_clear_payout_escrow_target() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 10_000;

    assert_eq!(client.get_payout_escrow_target(&member2), None);

    client.set_payout_escrow_target(&0, &member2, &seller, &arbiter, &deadline);
    let target = client.get_payout_escrow_target(&member2).unwrap();
    assert_eq!(target.seller, seller);
    assert_eq!(target.arbiter, arbiter);
    assert_eq!(target.deadline, deadline);

    client.clear_payout_escrow_target(&member2);
    assert_eq!(client.get_payout_escrow_target(&member2), None);
}

#[test]
fn test_set_escrow_contract_admin_only() {
    let (env, client, admin, _token, _members) = setup_with_members(2);
    let escrow = Address::generate(&env);

    client.set_escrow_contract(&admin, &escrow);
    assert_eq!(client.get_escrow_contract(), Some(escrow.clone()));

    // The alias configures the same value.
    let escrow2 = Address::generate(&env);
    client.set_payout_escrow_contract(&admin, &escrow2);
    assert_eq!(client.get_escrow_contract(), Some(escrow2));

    let outsider = Address::generate(&env);
    let err = client
        .try_set_escrow_contract(&outsider, &escrow)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError::OnlyAdminAllowed.into());
}

#[test]
fn test_payout_escrow_target_rejects_non_member() {
    let (env, client, _admin, _token, _members) = setup_with_members(2);
    let outsider = Address::generate(&env);
    let deadline = env.ledger().timestamp() + 10_000;

    let err = client
        .try_set_payout_escrow_target(
            &0,
            &outsider,
            &Address::generate(&env),
            &Address::generate(&env),
            &deadline,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::NotAMember.into());
}

#[test]
fn test_payout_escrow_target_locked_during_payout_round() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member1 = members.get(0).unwrap();
    let deadline = env.ledger().timestamp() + 10_000;

    let err = client
        .try_set_payout_escrow_target(
            &0,
            &member1,
            &Address::generate(&env),
            &Address::generate(&env),
            &deadline,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::BeneficiaryLocked.into());

    let err = client
        .try_clear_payout_escrow_target(&member1)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::BeneficiaryLocked.into());
}

#[test]
fn test_payout_escrow_target_rejects_past_deadline() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let deadline = env.ledger().timestamp();

    let err = client
        .try_set_payout_escrow_target(
            &0,
            &member2,
            &Address::generate(&env),
            &Address::generate(&env),
            &deadline,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutEscrowTarget.into());
}

#[test]
fn test_payout_escrow_target_rejects_contract_as_party() {
    let (env, client, _admin, _token, members) = setup_with_members(2);
    let member2 = members.get(1).unwrap();
    let deadline = env.ledger().timestamp() + 10_000;

    let err = client
        .try_set_payout_escrow_target(
            &0,
            &member2,
            &client.address,
            &Address::generate(&env),
            &deadline,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutEscrowTarget.into());

    let err = client
        .try_set_payout_escrow_target(
            &0,
            &member2,
            &Address::generate(&env),
            &client.address,
            &deadline,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidPayoutEscrowTarget.into());
}
