#![cfg(test)]
use super::*;
use crate::errors::ExtError3;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

const CONTRIBUTION: i128 = 100;
const POT: i128 = 2 * CONTRIBUTION;
const ROUND_SECONDS: u64 = 3_600;
const START_BALANCE: i128 = 1_000;

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
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin = token::StellarAssetClient::new(&env, &token);

    let mut members = Vec::new(&env);
    for _ in 0..2 {
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

    Setup { env, client, admin, token, members }
}

impl<'a> Setup<'a> {
    fn member(&self, i: u32) -> Address {
        self.members.get(i).unwrap()
    }

    fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(who)
    }

    fn full_round(&self) {
        for m in self.members.iter() {
            self.client.contribute(&m, &self.token, &CONTRIBUTION);
        }
    }

    fn advance_ledgers(&self, n: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number += n);
    }

    fn pass_deadline(&self) {
        let deadline = self.client.get_group_info().round_deadline;
        self.env.ledger().set_timestamp(deadline + 1);
    }
}

#[test]
fn test_disabled_vesting_pays_lump_sum() {
    let s = setup();
    let m0 = s.member(0);
    assert_eq!(s.client.get_payout_vesting_ledgers(), 0);

    s.full_round();

    assert_eq!(s.balance(&m0), START_BALANCE - CONTRIBUTION + POT);
    assert_eq!(s.client.get_vesting_record(&m0), None);
    let res = s.client.try_claim_vested(&m0);
    assert_eq!(res.unwrap_err().unwrap(), ExtError3::NoVestingRecord.into());
}

#[test]
fn test_claim_at_zero_half_and_full() {
    let s = setup();
    let m0 = s.member(0);
    assert_eq!(s.client.set_payout_vesting_ledgers(&s.admin, &100), None);
    assert_eq!(s.client.get_payout_vesting_ledgers(), 100);

    s.full_round();

    // Payout is locked, not paid.
    assert_eq!(s.balance(&m0), START_BALANCE - CONTRIBUTION);
    let record = s.client.get_vesting_record(&m0).unwrap();
    assert_eq!(record.total, POT);
    assert_eq!(record.released, 0);
    assert_eq!(record.duration, 100);

    // 0%: nothing to claim.
    assert_eq!(s.client.get_claimable_vested(&m0), 0);
    let res = s.client.try_claim_vested(&m0);
    assert_eq!(res.unwrap_err().unwrap(), ExtError3::NothingVested.into());

    // 50%.
    s.advance_ledgers(50);
    assert_eq!(s.client.get_claimable_vested(&m0), POT / 2);
    assert_eq!(s.client.claim_vested(&m0), POT / 2);
    assert_eq!(s.balance(&m0), START_BALANCE - CONTRIBUTION + POT / 2);

    // 100%, and well past it: never more than the total.
    s.advance_ledgers(500);
    assert_eq!(s.client.get_claimable_vested(&m0), POT / 2);
    assert_eq!(s.client.claim_vested(&m0), POT / 2);
    assert_eq!(s.balance(&m0), START_BALANCE - CONTRIBUTION + POT);
    let record = s.client.get_vesting_record(&m0).unwrap();
    assert_eq!(record.released, record.total);
    assert_eq!(s.client.get_claimable_vested(&m0), 0);
}

#[test]
fn test_claimable_grows_linearly() {
    let s = setup();
    let m0 = s.member(0);
    s.client.set_payout_vesting_ledgers(&s.admin, &200);
    s.full_round();

    let mut previous = 0;
    for step in 1..=4 {
        s.advance_ledgers(50);
        let claimable = s.client.get_claimable_vested(&m0);
        assert_eq!(claimable, POT * step / 4);
        assert!(claimable >= previous);
        previous = claimable;
    }
}

#[test]
fn test_locked_funds_excluded_from_next_pot() {
    let s = setup();
    let (m0, m1) = (s.member(0), s.member(1));
    s.client.set_payout_vesting_ledgers(&s.admin, &100);

    s.full_round();
    s.full_round();

    // m1's pot is its own round's contributions, not m0's locked payout too.
    assert_eq!(s.client.get_vesting_record(&m1).unwrap().total, POT);
    assert_eq!(s.client.get_vesting_record(&m0).unwrap().total, POT);

    s.advance_ledgers(100);
    assert_eq!(s.client.claim_vested(&m0), POT);
    assert_eq!(s.client.claim_vested(&m1), POT);
}

#[test]
fn test_payout_to_beneficiary_on_claim() {
    let s = setup();
    let m1 = s.member(1);
    let beneficiary = Address::generate(&s.env);
    s.client.set_payout_vesting_ledgers(&s.admin, &100);
    s.client.set_payout_beneficiary(&m1, &beneficiary);

    s.full_round();
    s.full_round();
    s.advance_ledgers(100);
    s.client.claim_vested(&m1);

    assert_eq!(s.balance(&beneficiary), POT);
}

#[test]
fn test_default_fully_covered_by_unvested_balance() {
    let s = setup();
    let (m0, m1) = (s.member(0), s.member(1));
    s.client.set_payout_vesting_ledgers(&s.admin, &1_000);

    s.full_round();

    // Round 1: m0 (already paid out) stops contributing.
    s.client.contribute(&m1, &s.token, &CONTRIBUTION);
    s.pass_deadline();
    s.client.finalize_round();

    // Unvested payout covered the missed contribution: no default recorded.
    let record = s.client.get_vesting_record(&m0).unwrap();
    assert_eq!(record.covered, CONTRIBUTION);
    assert_eq!(s.client.get_member_status(&m0).default_count, 0);

    // m1 received a full pot.
    assert_eq!(s.client.get_vesting_record(&m1).unwrap().total, POT);

    // m0 can only ever claim what is left after the cover.
    s.advance_ledgers(1_000);
    assert_eq!(s.client.claim_vested(&m0), POT - CONTRIBUTION);
    assert_eq!(s.client.get_claimable_vested(&m0), 0);
}

#[test]
fn test_default_partially_covered_still_defaults() {
    let s = setup();
    let (m0, m1) = (s.member(0), s.member(1));
    s.client.set_payout_vesting_ledgers(&s.admin, &100);

    s.full_round();

    // 70% vested: only 60 left unvested, less than the 100 owed.
    s.advance_ledgers(70);
    s.client.contribute(&m1, &s.token, &CONTRIBUTION);
    s.pass_deadline();
    s.client.finalize_round();

    let record = s.client.get_vesting_record(&m0).unwrap();
    assert_eq!(record.covered, 60);
    assert_eq!(s.client.get_member_status(&m0).default_count, 1);
    // m1's pot includes m0's partial cover.
    assert_eq!(s.client.get_vesting_record(&m1).unwrap().total, CONTRIBUTION + 60);

    s.advance_ledgers(100);
    assert_eq!(s.client.claim_vested(&m0), POT - 60);
}

#[test]
fn test_no_cover_without_vesting_record() {
    let s = setup();
    let m1 = s.member(1);
    s.client.set_payout_vesting_ledgers(&s.admin, &100);

    s.full_round();

    // m1 has not been paid yet, so there is nothing to cover with.
    s.client.contribute(&s.member(0), &s.token, &CONTRIBUTION);
    s.pass_deadline();
    s.client.finalize_round();

    assert_eq!(s.client.get_member_status(&m1).default_count, 1);
}

#[test]
fn test_change_after_activation_requires_governance() {
    let s = setup();
    s.env.ledger().set_timestamp(100);
    s.client.contribute(&s.member(0), &s.token, &CONTRIBUTION);

    let proposal_id = s
        .client
        .set_payout_vesting_ledgers(&s.admin, &500)
        .unwrap();
    assert_eq!(s.client.get_payout_vesting_ledgers(), 0);
    let proposal = s.client.get_proposal(&proposal_id).unwrap();
    assert_eq!(proposal.proposal_type, ProposalType::PayoutVestingUpdate);

    for m in s.members.iter() {
        s.client.vote_on_proposal(&m, &proposal_id, &true);
    }
    s.env
        .ledger()
        .set_timestamp(100 + charter::CHARTER_VOTING_WINDOW_SECONDS + 1);
    s.client.execute_proposal(&proposal_id);

    assert_eq!(s.client.get_payout_vesting_ledgers(), 500);
}
