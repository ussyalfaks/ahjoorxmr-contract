#![cfg(test)]
use super::*;
use crate::errors::{Error, ExtError3};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, IntoVal, Symbol, Vec,
};

struct Setup<'a> {
    env: Env,
    client: AhjoorContractClient<'a>,
    admin: Address,
    members: Vec<Address>,
}

fn setup<'a>(member_count: u32) -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    // The admin is also a member so the "outgoing admin cannot vote" rule can
    // be exercised directly.
    let mut members = Vec::new(&env);
    members.push_back(admin.clone());
    for _ in 1..member_count {
        members.push_back(Address::generate(&env));
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
            max_members: Some(member_count),
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
        members,
    }
}

fn has_event(env: &Env, topic: &str) -> bool {
    let expected = (Symbol::new(env, topic),).into_val(env);
    env.events().all().iter().any(|e| e.1 == expected)
}

#[test]
fn test_no_confidence_happy_path_replaces_admin() {
    let s = setup(10);
    let proposer = s.members.get(2).unwrap();
    let nominee = s.members.get(1).unwrap();

    let proposal_id = s
        .client
        .propose_no_confidence(&proposer, &nominee, &3_600);

    let proposal = s.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.proposal_type, ProposalType::NoConfidence);
    assert_eq!(proposal.required_quorum, 6_700);
    assert_eq!(proposal.status, ProposalStatus::Pending);

    // The outgoing admin cannot vote, even though it is a member.
    let err = s
        .client
        .try_vote_on_proposal(&s.admin, &proposal_id, &true)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::AdminCannotVoteOnNoConfidence.into());

    // 6/10 is below the 67% supermajority.
    for i in 1..7 {
        let voter = s.members.get(i).unwrap();
        s.client.vote_on_proposal(&voter, &proposal_id, &true);
    }
    s.env.ledger().set_timestamp(4_000);
    s.client.execute_proposal(&proposal_id);

    assert_eq!(s.client.get_admin(), s.admin);
    assert_eq!(
        s.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Rejected
    );

    // A fresh proposal with 7/10 votes passes and swaps the admin.
    let proposal_id = s
        .client
        .propose_no_confidence(&proposer, &nominee, &3_600);
    for i in 1..8 {
        let voter = s.members.get(i).unwrap();
        s.client.vote_on_proposal(&voter, &proposal_id, &true);
    }
    s.env.ledger().set_timestamp(8_000);
    s.client.execute_proposal(&proposal_id);

    assert!(has_event(&s.env, "no_confidence_executed"));
    assert!(has_event(&s.env, "admin_transferred"));

    assert_eq!(s.client.get_admin(), nominee);
    assert_eq!(
        s.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Executed
    );
}

#[test]
fn test_no_confidence_uses_configured_supermajority() {
    let s = setup(10);
    let proposer = s.members.get(1).unwrap();
    let nominee = s.members.get(2).unwrap();

    s.client
        .set_quorum_per_type(&s.admin, &ProposalType::NoConfidence, &8_000);
    assert_eq!(
        s.client.get_quorum_for_type(&ProposalType::NoConfidence),
        8_000
    );

    let proposal_id = s
        .client
        .propose_no_confidence(&proposer, &nominee, &3_600);
    assert_eq!(
        s.client.get_proposal(&proposal_id).unwrap().required_quorum,
        8_000
    );

    // 7/10 is below the configured 80% quorum.
    for i in 1..8 {
        let voter = s.members.get(i).unwrap();
        s.client.vote_on_proposal(&voter, &proposal_id, &true);
    }
    s.env.ledger().set_timestamp(4_000);
    s.client.execute_proposal(&proposal_id);
    assert_eq!(
        s.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Rejected
    );
    assert_eq!(s.client.get_admin(), s.admin);

    // 8/10 meets the configured quorum.
    let proposal_id = s
        .client
        .propose_no_confidence(&proposer, &nominee, &3_600);
    for i in 1..9 {
        let voter = s.members.get(i).unwrap();
        s.client.vote_on_proposal(&voter, &proposal_id, &true);
    }
    s.env.ledger().set_timestamp(8_000);
    s.client.execute_proposal(&proposal_id);
    assert_eq!(s.client.get_admin(), nominee);
    assert_eq!(
        s.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Executed
    );
}

#[test]
fn test_no_confidence_rejects_non_member_proposer() {
    let s = setup(10);
    let outsider = Address::generate(&s.env);
    let nominee = s.members.get(1).unwrap();

    let err = s
        .client
        .try_propose_no_confidence(&outsider, &nominee, &3_600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::OnlyMembersAllowed.into());
}

#[test]
fn test_no_confidence_rejects_non_member_nominee() {
    let s = setup(10);
    let proposer = s.members.get(1).unwrap();
    let outsider = Address::generate(&s.env);

    let err = s
        .client
        .try_propose_no_confidence(&proposer, &outsider, &3_600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidNoConfidenceNominee.into());
}

#[test]
fn test_no_confidence_rejects_current_admin_nominee() {
    let s = setup(10);
    let proposer = s.members.get(1).unwrap();

    let err = s
        .client
        .try_propose_no_confidence(&proposer, &s.admin, &3_600)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidNoConfidenceNominee.into());
}

#[test]
fn test_no_confidence_ignores_outgoing_admin_delegated_vote() {
    let s = setup(10);
    let proposer = s.members.get(2).unwrap();
    let nominee = s.members.get(1).unwrap();

    let proposal_id = s
        .client
        .propose_no_confidence(&proposer, &nominee, &3_600);

    // The admin delegates to a member; the admin's weight must still not count.
    s.client.delegate_vote(&s.admin, &nominee);
    s.client.vote_on_proposal(&nominee, &proposal_id, &true);

    assert_eq!(s.client.get_proposal(&proposal_id).unwrap().votes_for, 1);
}

#[test]
fn test_no_confidence_rejects_removed_nominee_at_execution() {
    let s = setup(10);
    let proposer = s.members.get(2).unwrap();
    let nominee = s.members.get(1).unwrap();

    let proposal_id = s
        .client
        .propose_no_confidence(&proposer, &nominee, &3_600);

    // Remove the nominee after the proposal is opened but before it executes.
    s.client.remove_member(&nominee);

    for i in 2..9 {
        let voter = s.members.get(i).unwrap();
        s.client.vote_on_proposal(&voter, &proposal_id, &true);
    }
    s.env.ledger().set_timestamp(4_000);

    let err = s
        .client
        .try_execute_proposal(&proposal_id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError3::InvalidNoConfidenceNominee.into());
    assert_eq!(s.client.get_admin(), s.admin);
}

#[test]
fn test_no_confidence_rejected_when_votes_against_win() {
    let s = setup(10);
    let proposer = s.members.get(1).unwrap();
    let nominee = s.members.get(2).unwrap();

    let proposal_id = s
        .client
        .propose_no_confidence(&proposer, &nominee, &3_600);

    // Four for, four against: quorum is met but the proposal fails.
    for i in 1..5 {
        let voter = s.members.get(i).unwrap();
        s.client.vote_on_proposal(&voter, &proposal_id, &true);
    }
    for i in 5..9 {
        let voter = s.members.get(i).unwrap();
        s.client.vote_on_proposal(&voter, &proposal_id, &false);
    }
    s.env.ledger().set_timestamp(4_000);
    s.client.execute_proposal(&proposal_id);

    assert_eq!(
        s.client.get_proposal(&proposal_id).unwrap().status,
        ProposalStatus::Rejected
    );
    assert_eq!(s.client.get_admin(), s.admin);
}
