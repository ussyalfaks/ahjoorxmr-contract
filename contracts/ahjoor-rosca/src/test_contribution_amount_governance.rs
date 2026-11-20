#![cfg(test)]
use super::*;
use crate::errors::{Error, ExtError3};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, Address, Env, IntoVal, String, Symbol, Vec,
};

struct Setup<'a> {
    env: Env,
    client: AhjoorContractClient<'a>,
    admin: Address,
    token: Address,
    members: Vec<Address>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let token = token_contract.address();
    let token_admin = token::StellarAssetClient::new(&env, &token);

    let mut members = Vec::new(&env);
    for _ in 0..2 {
        let member = Address::generate(&env);
        token_admin.mint(&member, &1_000);
        members.push_back(member);
    }

    client.init(
        &admin,
        &members,
        &100,
        &token,
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

    Setup {
        env,
        client,
        admin,
        token,
        members,
    }
}

fn has_event(env: &Env, topic: &str) -> bool {
    let expected = (Symbol::new(env, topic),).into_val(env);
    env.events().all().iter().any(|event| event.1 == expected)
}

fn approve_and_execute(s: &Setup, proposal_id: u32) {
    for member in s.members.iter() {
        s.client.vote_on_proposal(&member, &proposal_id, &true);
    }
    let timestamp = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(timestamp + 1);
    s.client.execute_proposal(&proposal_id);
}

#[test]
fn test_change_contribution_amount_applies_next_cycle() {
    let s = setup();
    let proposer = s.members.get(0).unwrap();

    let proposal_id = s.client.propose_contribution_change(&proposer, &150, &0);
    approve_and_execute(&s, proposal_id);

    assert!(has_event(&s.env, "contribution_amount_scheduled"));
    assert_eq!(s.client.get_contribution_amount(), 100);

    let pending = s.client.get_pending_contribution_change().unwrap();
    assert_eq!(pending.amount, 150);
    assert_eq!(pending.effective_cycle, 1);
    assert_eq!(pending.proposal_id, proposal_id);
    assert_eq!(
        s.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Executed
    );

    // Round 0 is still paid at the old amount and does not cross a cycle.
    for member in s.members.iter() {
        s.client.contribute(&member, &s.token, &100);
    }
    assert_eq!(s.client.get_contribution_amount(), 100);

    // Round 1 closes the cycle, so the pending amount applies.
    for member in s.members.iter() {
        s.client.contribute(&member, &s.token, &100);
    }

    assert!(has_event(&s.env, "contribution_amount_changed"));
    assert_eq!(s.client.get_contribution_amount(), 150);
    assert!(s.client.get_pending_contribution_change().is_none());
}

#[test]
fn test_change_contribution_amount_via_generic_proposal() {
    let s = setup();
    let proposer = s.members.get(0).unwrap();

    s.client.create_proposal(
        &proposer,
        &ProposalType::ChangeContributionAmount,
        &String::from_str(&s.env, "raise contribution"),
        &proposer,
        &0,
        &Some(125),
    );
    let proposal_id = s.client.get_proposal_counter() - 1;
    approve_and_execute(&s, proposal_id);

    assert_eq!(s.client.get_contribution_amount(), 100);
    assert_eq!(
        s.client.get_pending_contribution_change().unwrap().amount,
        125
    );
}

#[test]
fn test_change_contribution_amount_rejects_non_member_proposer() {
    let s = setup();
    let outsider = Address::generate(&s.env);

    let err = s
        .client
        .try_propose_contribution_change(&outsider, &150, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::OnlyMembersAllowed.into());
}

#[test]
fn test_change_contribution_amount_rejects_zero_amount() {
    let s = setup();
    let proposer = s.members.get(0).unwrap();

    let err = s
        .client
        .try_propose_contribution_change(&proposer, &0, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidContributionAmount.into());
}

#[test]
fn test_change_contribution_amount_rejects_token_limit_breach() {
    let s = setup();
    let proposer = s.members.get(0).unwrap();
    s.client.set_token_limit(&s.token, &120);

    let err = s
        .client
        .try_propose_contribution_change(&proposer, &150, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::ExceedsTokenLimit.into());
}

#[test]
fn test_change_contribution_amount_rejects_overflow() {
    let s = setup();
    let proposer = s.members.get(0).unwrap();

    let err = s
        .client
        .try_propose_contribution_change(&proposer, &i128::MAX, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::ContributionAmountOverflow.into());
}

#[test]
fn test_change_contribution_amount_rejects_missing_execution_data() {
    let s = setup();
    let proposer = s.members.get(0).unwrap();

    let err = s
        .client
        .try_create_proposal(
            &proposer,
            &ProposalType::ChangeContributionAmount,
            &String::from_str(&s.env, "missing amount"),
            &proposer,
            &0,
            &None,
        )
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidContributionAmount.into());
}

#[test]
fn test_change_contribution_amount_rejects_second_pending_change() {
    let s = setup();
    let proposer = s.members.get(0).unwrap();

    let first = s.client.propose_contribution_change(&proposer, &150, &0);
    approve_and_execute(&s, first);

    let second = s.client.propose_contribution_change(&proposer, &175, &0);
    for member in s.members.iter() {
        s.client.vote_on_proposal(&member, &second, &true);
    }
    let timestamp = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(timestamp + 1);

    let err = s.client.try_execute_proposal(&second).unwrap_err().unwrap();
    assert_eq!(err, ExtError3::ContributionAmountChangePending.into());
    assert_eq!(
        s.client.get_pending_contribution_change().unwrap().amount,
        150
    );
}
