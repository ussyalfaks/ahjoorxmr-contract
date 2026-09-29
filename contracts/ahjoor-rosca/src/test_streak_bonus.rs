#![cfg(test)]
use super::*;
use crate::errors::ExtError3;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Vec,
};

const CONTRIBUTION: i128 = 100;
const ROUND_SECONDS: u64 = 3_600;
const GRACE_SECONDS: u64 = 1_000;

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
    for _ in 0..3 {
        let m = Address::generate(&env);
        token_admin.mint(&m, &10_000);
        members.push_back(m);
    }
    token_admin.mint(&admin, &10_000);

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
            max_skips_per_cycle: 1,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: GRACE_SECONDS,
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

    /// Every member contributes on time, completing the round.
    fn full_round(&self) {
        for m in self.members.iter() {
            self.client.contribute(&m, &self.token, &CONTRIBUTION);
        }
    }

    /// Moves past the current round's deadline (into the grace period).
    fn pass_deadline(&self) {
        let deadline = self.client.get_group_info().round_deadline;
        self.env.ledger().set_timestamp(deadline + 1);
    }
}

#[test]
fn test_on_time_members_are_eligible_and_paid_equal_shares() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &900);
    s.client.set_streak_bonus_bps(&s.admin, &1_000);
    assert_eq!(s.client.get_streak_bonus_pool(), 900);

    // Round 0 payout must not sweep the streak pool into the pot.
    let m0 = s.member(0);
    let m0_before = s.balance(&m0);
    s.full_round();
    assert_eq!(s.balance(&m0), m0_before - CONTRIBUTION + 3 * CONTRIBUTION);

    s.full_round();
    let status = s.client.get_streak_status(&m0);
    assert_eq!(status.current_cycle, 0);
    assert!(status.on_track);
    assert_eq!(status.claimable_cycle, None);

    s.full_round(); // completes cycle 0

    // 10% of 900 per member, three members: 270 allocated, 90 each.
    let allocation = s.client.get_streak_allocation(&0).unwrap();
    assert_eq!(allocation.eligible.len(), 3);
    assert_eq!(allocation.per_member_amount, 90);
    assert_eq!(s.client.get_streak_bonus_pool(), 630);

    let status = s.client.get_streak_status(&m0);
    assert_eq!(status.current_cycle, 1);
    assert_eq!(status.claimable_cycle, Some(0));
    assert_eq!(status.claimable_amount, 90);

    for m in s.members.iter() {
        let before = s.balance(&m);
        assert_eq!(s.client.claim_streak_bonus(&m, &0), 90);
        assert_eq!(s.balance(&m), before + 90);
    }
    assert_eq!(s.client.get_streak_allocation(&0).unwrap().claimed_count, 3);
    assert_eq!(s.client.get_streak_status(&m0).claimable_cycle, None);

    // The next cycle's pots are unaffected by the remaining pool.
    let m0_before = s.balance(&m0);
    s.full_round();
    assert_eq!(s.balance(&m0), m0_before - CONTRIBUTION + 3 * CONTRIBUTION);
}

#[test]
fn test_pool_split_evenly_and_capped_at_pool() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &1_000);
    // 50% per member for 3 members would need 150% of the pool: capped.
    s.client.set_streak_bonus_bps(&s.admin, &5_000);

    for _ in 0..3 {
        s.full_round();
    }

    let allocation = s.client.get_streak_allocation(&0).unwrap();
    assert_eq!(allocation.per_member_amount, 333);
    // Only whole equal shares are allocated; the dust stays in the pool.
    assert_eq!(s.client.get_streak_bonus_pool(), 1);

    let mut total = 0;
    for m in s.members.iter() {
        total += s.client.claim_streak_bonus(&m, &0);
    }
    assert_eq!(total, 999);
}

#[test]
fn test_late_contribution_disqualifies() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &900);
    s.client.set_streak_bonus_bps(&s.admin, &1_000);
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));

    s.full_round();

    // Round 1: m1 pays during the grace period.
    s.client.contribute(&m0, &s.token, &CONTRIBUTION);
    s.client.contribute(&m2, &s.token, &CONTRIBUTION);
    s.pass_deadline();
    s.client.contribute(&m1, &s.token, &CONTRIBUTION);
    assert!(!s.client.get_streak_status(&m1).on_track);
    assert!(s.client.get_streak_status(&m0).on_track);

    s.full_round();

    let allocation = s.client.get_streak_allocation(&0).unwrap();
    assert_eq!(allocation.eligible.len(), 2);
    assert!(!allocation.eligible.contains(&m1));
    assert_eq!(allocation.per_member_amount, 90);

    let res = s.client.try_claim_streak_bonus(&m1, &0);
    assert_eq!(res.unwrap_err().unwrap(), ExtError3::StreakNotEligible.into());
    assert_eq!(s.client.claim_streak_bonus(&m0, &0), 90);
}

#[test]
fn test_skip_request_disqualifies() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &900);
    s.client.set_streak_bonus_bps(&s.admin, &1_000);
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));

    s.full_round();

    // Round 1: m2 skips; the round is finalized without them.
    s.client.request_skip(&m2, &1);
    assert!(!s.client.get_streak_status(&m2).on_track);
    s.client.contribute(&m0, &s.token, &CONTRIBUTION);
    s.client.contribute(&m1, &s.token, &CONTRIBUTION);
    s.env.ledger().set_timestamp(s.client.get_group_info().round_deadline + GRACE_SECONDS + 1);
    s.client.finalize_round();

    s.full_round();

    let allocation = s.client.get_streak_allocation(&0).unwrap();
    assert_eq!(allocation.eligible.len(), 2);
    assert!(!allocation.eligible.contains(&m2));
    let res = s.client.try_claim_streak_bonus(&m2, &0);
    assert_eq!(res.unwrap_err().unwrap(), ExtError3::StreakNotEligible.into());
}

#[test]
fn test_missed_contribution_disqualifies() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &900);
    s.client.set_streak_bonus_bps(&s.admin, &1_000);
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));

    s.full_round();

    // Round 1: m2 never pays and the round is finalized.
    s.client.contribute(&m0, &s.token, &CONTRIBUTION);
    s.client.contribute(&m1, &s.token, &CONTRIBUTION);
    s.env.ledger().set_timestamp(s.client.get_group_info().round_deadline + GRACE_SECONDS + 1);
    s.client.finalize_round();

    s.full_round();

    let allocation = s.client.get_streak_allocation(&0).unwrap();
    assert!(!allocation.eligible.contains(&m2));
    assert!(allocation.eligible.contains(&m0));
}

#[test]
fn test_emergency_exit_request_disqualifies() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &900);
    s.client.set_streak_bonus_bps(&s.admin, &1_000);
    let m1 = s.member(1);

    s.full_round();
    s.client.request_emergency_exit(&m1);
    s.client.reject_exit(&m1);
    s.full_round();
    s.full_round();

    let allocation = s.client.get_streak_allocation(&0).unwrap();
    assert_eq!(allocation.eligible.len(), 2);
    assert!(!allocation.eligible.contains(&m1));
}

#[test]
fn test_double_claim_rejected() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &900);
    s.client.set_streak_bonus_bps(&s.admin, &1_000);
    for _ in 0..3 {
        s.full_round();
    }
    let m0 = s.member(0);
    s.client.claim_streak_bonus(&m0, &0);

    let res = s.client.try_claim_streak_bonus(&m0, &0);
    assert_eq!(
        res.unwrap_err().unwrap(),
        ExtError3::StreakBonusAlreadyClaimed.into()
    );
}

#[test]
fn test_claim_before_cycle_completes_rejected() {
    let s = setup();
    s.client.fund_streak_bonus_pool(&s.admin, &900);
    s.client.set_streak_bonus_bps(&s.admin, &1_000);
    s.full_round();

    let res = s.client.try_claim_streak_bonus(&s.member(0), &0);
    assert_eq!(
        res.unwrap_err().unwrap(),
        ExtError3::StreakCycleNotCompleted.into()
    );
}

#[test]
fn test_invalid_bps_rejected() {
    let s = setup();
    let res = s.client.try_set_streak_bonus_bps(&s.admin, &10_001);
    assert_eq!(
        res.unwrap_err().unwrap(),
        ExtError3::InvalidStreakBonusBps.into()
    );
}
