#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

const CONTRIBUTION: i128 = 100;
const MEMBER_COUNT: u32 = 3;
const POT: i128 = CONTRIBUTION * MEMBER_COUNT as i128;
const GROUP_ID: u32 = 0;

struct Setup<'a> {
    env: Env,
    client: AhjoorContractClient<'a>,
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
    for _ in 0..MEMBER_COUNT {
        let m = Address::generate(&env);
        token_admin.mint(&m, &10_000);
        members.push_back(m);
    }

    client.init(
        &admin,
        &members,
        &CONTRIBUTION,
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

    Setup { env, client, token, members }
}

impl<'a> Setup<'a> {
    fn member(&self, i: u32) -> Address {
        self.members.get(i).unwrap()
    }

    fn full_round(&self) {
        for m in self.members.iter() {
            self.client.contribute(&m, &self.token, &CONTRIBUTION);
        }
    }

    fn history(&self, member: &Address, start: u32, limit: u32) -> Vec<MemberPayoutEntry> {
        self.client
            .get_member_payout_history(&GROUP_ID, member, &start, &limit)
    }
}

#[test]
fn test_unpaid_member_has_empty_history() {
    let s = setup();
    for m in s.members.iter() {
        assert_eq!(s.history(&m, 0, 10).len(), 0);
    }

    // After round 0 only member 0 has been paid.
    s.full_round();
    assert_eq!(s.history(&s.member(0), 0, 10).len(), 1);
    assert_eq!(s.history(&s.member(1), 0, 10).len(), 0);
    assert_eq!(s.history(&s.member(2), 0, 10).len(), 0);
}

#[test]
fn test_payout_entry_records_cycle_round_amount_token_and_ledger() {
    let s = setup();
    s.env.ledger().with_mut(|l| l.sequence_number = 4_242);
    s.full_round();

    let page = s.history(&s.member(0), 0, 10);
    assert_eq!(page.len(), 1);
    assert_eq!(
        page.get(0).unwrap(),
        MemberPayoutEntry {
            cycle: 0,
            round: 0,
            amount: POT,
            token: s.token.clone(),
            ledger: 4_242,
        }
    );
}

#[test]
fn test_multi_cycle_history_lists_every_payout_in_order() {
    let s = setup();
    // Three full cycles: each member is paid once per cycle.
    for _ in 0..(MEMBER_COUNT * 3) {
        s.full_round();
    }

    for i in 0..MEMBER_COUNT {
        let page = s.history(&s.member(i), 0, 10);
        assert_eq!(page.len(), 3);
        for cycle in 0..3u32 {
            let entry = page.get(cycle).unwrap();
            assert_eq!(entry.cycle, cycle);
            assert_eq!(entry.round, cycle * MEMBER_COUNT + i);
            assert_eq!(entry.amount, POT);
            assert_eq!(entry.token, s.token);
        }
    }
}

#[test]
fn test_pagination_returns_stable_non_overlapping_pages() {
    let s = setup();
    for _ in 0..(MEMBER_COUNT * 3) {
        s.full_round();
    }
    let m0 = s.member(0);
    let full = s.history(&m0, 0, 10);
    assert_eq!(full.len(), 3);

    let first = s.history(&m0, 0, 2);
    let second = s.history(&m0, 2, 2);
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 1);
    assert_eq!(first.get(0).unwrap(), full.get(0).unwrap());
    assert_eq!(first.get(1).unwrap(), full.get(1).unwrap());
    assert_eq!(second.get(0).unwrap(), full.get(2).unwrap());
    assert_ne!(first.get(1).unwrap().round, second.get(0).unwrap().round);

    // Re-reading a page returns the same entries.
    assert_eq!(s.history(&m0, 0, 2), first);

    // Later payouts append without shifting earlier pages.
    for _ in 0..MEMBER_COUNT {
        s.full_round();
    }
    assert_eq!(s.history(&m0, 0, 2), first);
    assert_eq!(s.history(&m0, 2, 2).get(0).unwrap(), second.get(0).unwrap());
    assert_eq!(s.history(&m0, 2, 2).len(), 2);
}

#[test]
fn test_out_of_range_start_returns_empty() {
    let s = setup();
    s.full_round();
    let m0 = s.member(0);
    assert_eq!(s.history(&m0, 1, 10).len(), 0);
    assert_eq!(s.history(&m0, 50, 10).len(), 0);
    assert_eq!(s.history(&m0, u32::MAX, u32::MAX).len(), 0);
}

#[test]
fn test_zero_limit_returns_empty() {
    let s = setup();
    s.full_round();
    assert_eq!(s.history(&s.member(0), 0, 0).len(), 0);
}

#[test]
fn test_limit_is_capped_at_max_page_size() {
    let s = setup();
    let m0 = s.member(0);
    let total = audit_trail::MAX_CONTRIBUTION_HISTORY_RANGE + 20;

    // Seed more entries than one page can hold.
    s.env.as_contract(&s.client.address, || {
        for i in 0..total {
            internals::record_member_payout(&s.env, &m0, i * MEMBER_COUNT, &s.token, POT);
        }
    });

    let page = s.history(&m0, 0, u32::MAX);
    assert_eq!(page.len(), audit_trail::MAX_CONTRIBUTION_HISTORY_RANGE);
    let rest = s.history(&m0, audit_trail::MAX_CONTRIBUTION_HISTORY_RANGE, u32::MAX);
    assert_eq!(rest.len(), 20);
    assert_eq!(
        rest.get(0).unwrap().round,
        audit_trail::MAX_CONTRIBUTION_HISTORY_RANGE * MEMBER_COUNT
    );
}

#[test]
fn test_payout_to_beneficiary_is_recorded_for_member() {
    let s = setup();
    let m0 = s.member(0);
    let beneficiary = Address::generate(&s.env);
    // Round 0 pays member 0, so nominate during round 0 only for member 1.
    let m1 = s.member(1);
    s.client.set_payout_beneficiary(&m1, &beneficiary);

    s.full_round();
    s.full_round();

    assert_eq!(token::Client::new(&s.env, &s.token).balance(&beneficiary), POT);
    assert_eq!(s.history(&m1, 0, 10).len(), 1);
    assert_eq!(s.history(&m1, 0, 10).get(0).unwrap().round, 1);
    assert_eq!(s.history(&beneficiary, 0, 10).len(), 0);
    assert_eq!(s.history(&m0, 0, 10).len(), 1);
}
