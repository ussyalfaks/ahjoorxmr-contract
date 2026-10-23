#![cfg(test)]
use super::*;
use crate::errors::ExtError3;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

const CONTRIBUTION: i128 = 100;
const REDUCED: i128 = 40;
const SHORTFALL: i128 = CONTRIBUTION - REDUCED;
const MEMBER_COUNT: u32 = 3;
const POT: i128 = CONTRIBUTION * MEMBER_COUNT as i128;
const START_BALANCE: i128 = 10_000;
// Rounds outlast the 3-day governance voting window.
const ROUND_SECONDS: u64 = 30 * 86_400;
const GROUP_ID: u32 = 0;

struct Setup<'a> {
    env: Env,
    client: AhjoorContractClient<'a>,
    token: Address,
    members: Vec<Address>,
}

/// Group of three with the emergency reserve enabled and funded
/// contract-internally (there is no public setter for `ReserveEnabled`).
fn setup<'a>(reserve: i128) -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin = token::StellarAssetClient::new(&env, &token);

    let mut members = Vec::new(&env);
    for _ in 0..MEMBER_COUNT {
        let m = Address::generate(&env);
        token_admin.mint(&m, &START_BALANCE);
        members.push_back(m);
    }

    client.init(
        &admin,
        &members,
        &CONTRIBUTION,
        &token,
        &ROUND_SECONDS,
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

    token_admin.mint(&contract_id, &reserve);
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&DataKey3::ReserveEnabled, &true);
        env.storage()
            .persistent()
            .set(&DataKey3::EmergencyReserveBalance, &reserve);
    });

    Setup { env, client, token, members }
}

impl<'a> Setup<'a> {
    fn member(&self, i: u32) -> Address {
        self.members.get(i).unwrap()
    }

    fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(who)
    }

    fn request(&self, member: &Address, round: u32) -> u32 {
        self.client
            .request_hardship_reduction(&GROUP_ID, member, &round, &REDUCED)
    }

    /// Every member votes `vote_for`, then the voting window closes and the
    /// proposal is executed.
    fn vote_and_execute(&self, proposal_id: u32, vote_for: bool) {
        for m in self.members.iter() {
            self.client.vote_on_proposal(&m, &proposal_id, &vote_for);
        }
        self.close_voting(proposal_id);
        self.client.execute_proposal(&proposal_id);
    }

    fn vote_all(&self, proposal_id: u32) {
        for m in self.members.iter() {
            self.client.vote_on_proposal(&m, &proposal_id, &true);
        }
    }

    fn close_voting(&self, proposal_id: u32) {
        let deadline = self.client.get_proposal(&proposal_id).unwrap().deadline;
        self.env.ledger().set_timestamp(deadline + 1);
    }

    fn approve(&self, member: &Address, round: u32) -> u32 {
        let id = self.request(member, round);
        self.vote_and_execute(id, true);
        id
    }

    fn contribute(&self, member: &Address, amount: i128) {
        self.client.contribute(member, &self.token, &amount);
    }

    fn full_round(&self) {
        for m in self.members.iter() {
            self.contribute(&m, CONTRIBUTION);
        }
    }
}

#[test]
fn test_request_opens_hardship_proposal() {
    let s = setup(1_000);
    let m1 = s.member(1);
    let id = s.request(&m1, 0);

    let proposal = s.client.get_proposal(&id).unwrap();
    assert_eq!(proposal.proposal_type, ProposalType::HardshipReduction);
    assert_eq!(proposal.status, ProposalStatus::Pending);

    let request = s.client.get_hardship_request(&id).unwrap();
    assert_eq!(request.member, m1);
    assert_eq!(request.round, 0);
    assert_eq!(request.cycle, 0);
    assert_eq!(request.reduced_amount, REDUCED);
    assert_eq!(request.shortfall, SHORTFALL);
    assert!(!request.approved);

    // Nothing is drawn or applied before approval.
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000);
    assert_eq!(s.client.get_hardship_reduction(&m1, &0), None);
}

#[test]
fn test_approved_reduction_lowers_contribution_and_draws_reserve() {
    let s = setup(1_000);
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));
    let id = s.approve(&m1, 0);

    assert_eq!(s.client.get_proposal(&id).unwrap().status, ProposalStatus::Executed);
    assert!(s.client.get_hardship_request(&id).unwrap().approved);
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000 - SHORTFALL);

    s.contribute(&m0, CONTRIBUTION);
    s.contribute(&m1, REDUCED);
    s.contribute(&m2, CONTRIBUTION);

    // The round completed with the full pot: the shortfall came from the reserve.
    assert_eq!(s.client.get_group_info().current_round, 1);
    assert_eq!(s.balance(&m0), START_BALANCE - CONTRIBUTION + POT);
    assert_eq!(s.balance(&m1), START_BALANCE - REDUCED);
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000 - SHORTFALL);
    assert_eq!(s.balance(&s.client.address), 1_000 - SHORTFALL);
}

#[test]
fn test_reduction_applies_only_to_the_specified_round() {
    let s = setup(1_000);
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));
    s.approve(&m1, 0);

    s.contribute(&m0, CONTRIBUTION);
    s.contribute(&m1, REDUCED);
    s.contribute(&m2, CONTRIBUTION);

    // Round 1: the reduced amount no longer settles m1's obligation.
    assert_eq!(s.client.get_hardship_reduction(&m1, &1), None);
    s.contribute(&m0, CONTRIBUTION);
    s.contribute(&m1, REDUCED);
    s.contribute(&m2, CONTRIBUTION);
    assert_eq!(s.client.get_group_info().current_round, 1);

    s.contribute(&m1, SHORTFALL);
    assert_eq!(s.client.get_group_info().current_round, 2);
    assert_eq!(s.balance(&m1), START_BALANCE - REDUCED - CONTRIBUTION + POT);
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000 - SHORTFALL);
}

#[test]
fn test_full_contribution_rejected_in_reduced_round() {
    let s = setup(1_000);
    let m1 = s.member(1);
    s.approve(&m1, 0);

    let result = s.client.try_contribute(&m1, &s.token, &CONTRIBUTION);
    assert_eq!(result, Err(Ok(Error::ExceedsRemainingContribution.into())));
}

#[test]
fn test_future_round_shortfall_is_held_out_of_earlier_pots() {
    let s = setup(1_000);
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));
    s.approve(&m2, 1);
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000 - SHORTFALL);

    // Round 0 pays exactly the contributions; the earmark is not swept in.
    s.full_round();
    assert_eq!(s.balance(&m0), START_BALANCE - CONTRIBUTION + POT);

    // Round 1 uses the reduction and still pays a full pot.
    s.contribute(&m0, CONTRIBUTION);
    s.contribute(&m1, CONTRIBUTION);
    s.contribute(&m2, REDUCED);
    assert_eq!(s.client.get_group_info().current_round, 2);
    assert_eq!(s.balance(&m1), START_BALANCE - 2 * CONTRIBUTION + POT);
    assert_eq!(s.balance(&s.client.address), 1_000 - SHORTFALL);
}

#[test]
fn test_unpaid_reduced_round_returns_shortfall_to_reserve() {
    let s = setup(1_000);
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));
    s.approve(&m1, 0);
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000 - SHORTFALL);

    s.contribute(&m0, CONTRIBUTION);
    s.contribute(&m2, CONTRIBUTION);
    let deadline = s.client.get_group_info().round_deadline;
    s.env.ledger().set_timestamp(deadline + 1);
    s.client.finalize_round();

    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000);
    assert_eq!(s.balance(&m0), START_BALANCE - CONTRIBUTION + 2 * CONTRIBUTION);
    assert_eq!(s.balance(&s.client.address), 1_000);
}

#[test]
#[should_panic(expected = "Error(Contract, #182)")]
fn test_request_fails_when_reserve_cannot_cover_shortfall() {
    let s = setup(SHORTFALL - 1);
    s.request(&s.member(1), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #182)")]
fn test_request_fails_when_reserve_disabled() {
    let s = setup(1_000);
    s.env.as_contract(&s.client.address, || {
        s.env
            .storage()
            .instance()
            .set(&DataKey3::ReserveEnabled, &false);
    });
    s.request(&s.member(1), 0);
}

#[test]
fn test_execution_fails_cleanly_when_reserve_drained() {
    let s = setup(100);
    let (m0, m1) = (s.member(0), s.member(1));
    let id = s.request(&m1, 0);
    s.vote_all(id);

    // Another member's emergency loan leaves less than the shortfall.
    s.client.request_emergency_loan(&m0, &50, &1_000);
    assert_eq!(s.client.get_emergency_reserve_balance(), 50);

    s.close_voting(id);
    let result = s.client.try_execute_proposal(&id);
    assert_eq!(
        result,
        Err(Ok(ExtError3::InsufficientReserveForHardship.into()))
    );

    // Nothing changed: proposal still pending, reserve intact, no reduction.
    assert_eq!(s.client.get_proposal(&id).unwrap().status, ProposalStatus::Pending);
    assert_eq!(s.client.get_emergency_reserve_balance(), 50);
    assert_eq!(s.client.get_hardship_reduction(&m1, &0), None);
    assert!(!s.client.get_hardship_request(&id).unwrap().approved);
}

#[test]
fn test_second_request_in_same_cycle_rejected() {
    let s = setup(1_000);
    let m1 = s.member(1);
    s.approve(&m1, 0);

    // Rounds 0..=2 form cycle 0 for a three-member group.
    let result = s
        .client
        .try_request_hardship_reduction(&GROUP_ID, &m1, &2, &REDUCED);
    assert_eq!(
        result,
        Err(Ok(ExtError3::HardshipAlreadyUsedThisCycle.into()))
    );
}

#[test]
fn test_second_request_while_first_pending_rejected() {
    let s = setup(1_000);
    let m1 = s.member(1);
    s.request(&m1, 0);

    let result = s
        .client
        .try_request_hardship_reduction(&GROUP_ID, &m1, &1, &REDUCED);
    assert_eq!(
        result,
        Err(Ok(ExtError3::HardshipAlreadyUsedThisCycle.into()))
    );
}

#[test]
fn test_rejected_request_does_not_use_up_the_cycle() {
    let s = setup(1_000);
    let m1 = s.member(1);
    let first = s.request(&m1, 0);
    s.vote_and_execute(first, false);
    assert_eq!(s.client.get_proposal(&first).unwrap().status, ProposalStatus::Rejected);
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000);

    let second = s.request(&m1, 0);
    assert_ne!(first, second);
}

#[test]
fn test_request_allowed_again_in_next_cycle() {
    let s = setup(1_000);
    let m1 = s.member(1);
    s.approve(&m1, 0);

    let id = s.request(&m1, MEMBER_COUNT);
    assert_eq!(s.client.get_hardship_request(&id).unwrap().cycle, 1);
}

#[test]
fn test_other_members_unaffected_by_cycle_limit() {
    let s = setup(1_000);
    s.approve(&s.member(1), 0);
    s.approve(&s.member(2), 0);
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000 - 2 * SHORTFALL);
}

#[test]
fn test_invalid_reduced_amounts_rejected() {
    let s = setup(1_000);
    let m1 = s.member(1);
    for amount in [0, -5, CONTRIBUTION, CONTRIBUTION + 1] {
        let result = s
            .client
            .try_request_hardship_reduction(&GROUP_ID, &m1, &0, &amount);
        assert_eq!(result, Err(Ok(ExtError3::InvalidHardshipAmount.into())));
    }
}

#[test]
fn test_request_for_past_round_rejected() {
    let s = setup(1_000);
    s.full_round();
    let result = s
        .client
        .try_request_hardship_reduction(&GROUP_ID, &s.member(1), &0, &REDUCED);
    assert_eq!(result, Err(Ok(ExtError3::HardshipRoundPassed.into())));
}

#[test]
fn test_non_member_request_rejected() {
    let s = setup(1_000);
    let outsider = Address::generate(&s.env);
    let result = s
        .client
        .try_request_hardship_reduction(&GROUP_ID, &outsider, &0, &REDUCED);
    assert_eq!(result, Err(Ok(Error::NotAMember.into())));
}

#[test]
fn test_execution_rejected_after_member_already_paid() {
    let s = setup(1_000);
    let m1 = s.member(1);
    let id = s.request(&m1, 0);
    s.vote_all(id);
    s.contribute(&m1, CONTRIBUTION);

    s.close_voting(id);
    let result = s.client.try_execute_proposal(&id);
    assert_eq!(result, Err(Ok(Error::AlreadyContributed.into())));
    assert_eq!(s.client.get_emergency_reserve_balance(), 1_000);
}
