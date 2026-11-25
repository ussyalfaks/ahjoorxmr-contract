#![cfg(test)]
//! Catch-up contributions (`pay_catch_up_contribution`, `get_catch_up_debt`)
//! and penalty-based slot demotion for late contributors
//! (`configure_late_demotion`, `get_late_contrib_threshold`).
use super::*;
use crate::errors::{Error, ExtError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, Map, Vec,
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
            max_defaults: 5,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
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
    env.ledger().set_timestamp(100);

    Setup {
        env,
        client,
        admin,
        token,
        members,
    }
}

impl<'a> Setup<'a> {
    fn member(&self, i: u32) -> Address {
        self.members.get(i).unwrap()
    }

    fn balance(&self, who: &Address) -> i128 {
        token::Client::new(&self.env, &self.token).balance(who)
    }

    fn late_count(&self, who: &Address) -> u32 {
        self.client
            .get_late_contribution_counts()
            .get(who.clone())
            .unwrap_or(0)
    }

    /// Moves past the current round's soft deadline (into the grace period).
    fn pass_deadline(&self) {
        let deadline = self.client.get_group_info().round_deadline;
        self.env.ledger().set_timestamp(deadline + 1);
    }

    /// Everyone except `late` pays on time; `late` pays during the grace
    /// period, which completes the round.
    fn round_with_late(&self, late: &Address) {
        for m in self.members.iter() {
            if m != *late {
                self.client.contribute(&m, &self.token, &CONTRIBUTION);
            }
        }
        self.pass_deadline();
        self.client.contribute(late, &self.token, &CONTRIBUTION);
    }

    /// Everyone except `missing` pays on time, then the round is finalized.
    fn round_with_missing(&self, missing: &Address) {
        for m in self.members.iter() {
            if m != *missing {
                self.client.contribute(&m, &self.token, &CONTRIBUTION);
            }
        }
        self.pass_deadline();
        self.client.finalize_round();
    }

    /// Writes catch-up debt directly into contract storage. Debt is normally
    /// carried in by a succession claim; seeding it keeps these tests focused
    /// on the catch-up payment itself.
    fn seed_debts(&self, entries: &[(&Address, i128)]) {
        self.env.as_contract(&self.client.address, || {
            let mut debts: Map<Address, i128> = self
                .env
                .storage()
                .instance()
                .get(&DataKey2::CatchUpDebt)
                .unwrap_or(Map::new(&self.env));
            for (who, amount) in entries.iter() {
                debts.set((*who).clone(), *amount);
            }
            self.env
                .storage()
                .instance()
                .set(&DataKey2::CatchUpDebt, &debts);
        });
    }
}

// ── Catch-up debt accrual ────────────────────────────────────────────────────

#[test]
fn test_no_catch_up_debt_by_default() {
    let s = setup();
    for m in s.members.iter() {
        assert_eq!(s.client.get_catch_up_debt(&m), 0);
    }
}

#[test]
fn test_catch_up_debt_is_tracked_per_member() {
    let s = setup();
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));
    s.seed_debts(&[(&m0, 200), (&m1, 50)]);

    assert_eq!(s.client.get_catch_up_debt(&m0), 200);
    assert_eq!(s.client.get_catch_up_debt(&m1), 50);
    assert_eq!(s.client.get_catch_up_debt(&m2), 0);
}

#[test]
fn test_catch_up_debt_accrues_onto_successor() {
    let s = setup();
    let member = s.member(2);
    let successor = Address::generate(&s.env);

    s.client.set_succession_trigger_rounds(&s.admin, &2);
    s.client.designate_successor(&member, &successor);
    s.client.accept_succession(&successor, &member);
    s.round_with_missing(&member);
    s.round_with_missing(&member);

    // Inactive member owes 200; the successor already owes 50.
    s.seed_debts(&[(&member, 200), (&successor, 50)]);

    s.client.claim_succession(&successor, &member);

    // Debts add up on the successor and the inactive member's entry is cleared.
    assert_eq!(s.client.get_catch_up_debt(&successor), 250);
    assert_eq!(s.client.get_catch_up_debt(&member), 0);
}

// ── Full and partial catch-up ────────────────────────────────────────────────

#[test]
fn test_full_catch_up_clears_debt_and_moves_funds() {
    let s = setup();
    let m0 = s.member(0);
    s.seed_debts(&[(&m0, 300)]);

    let member_before = s.balance(&m0);
    let contract_before = s.balance(&s.client.address);

    s.client.pay_catch_up_contribution(&m0);

    assert_eq!(s.client.get_catch_up_debt(&m0), 0);
    assert_eq!(s.balance(&m0), member_before - 300);
    assert_eq!(s.balance(&s.client.address), contract_before + 300);
}

#[test]
fn test_catch_up_only_clears_the_payers_debt() {
    let s = setup();
    let (m0, m1) = (s.member(0), s.member(1));
    s.seed_debts(&[(&m0, 300), (&m1, 120)]);

    s.client.pay_catch_up_contribution(&m0);

    assert_eq!(s.client.get_catch_up_debt(&m0), 0);
    assert_eq!(s.client.get_catch_up_debt(&m1), 120);
}

#[test]
fn test_catch_up_rejected_when_nothing_owed() {
    let s = setup();
    let m0 = s.member(0);
    let before = s.balance(&m0);

    assert!(s.client.try_pay_catch_up_contribution(&m0).is_err());
    assert_eq!(s.balance(&m0), before);
}

#[test]
fn test_catch_up_cannot_be_paid_twice() {
    let s = setup();
    let m0 = s.member(0);
    s.seed_debts(&[(&m0, 300)]);

    s.client.pay_catch_up_contribution(&m0);
    let after_first = s.balance(&m0);

    assert!(s.client.try_pay_catch_up_contribution(&m0).is_err());
    assert_eq!(s.balance(&m0), after_first);
}

#[test]
fn test_partial_catch_up_is_not_possible_with_insufficient_balance() {
    let s = setup();
    let newcomer = Address::generate(&s.env);
    token::StellarAssetClient::new(&s.env, &s.token).mint(&newcomer, &150);
    s.seed_debts(&[(&newcomer, 300)]);

    // The whole debt is collected in one transfer: a balance that only covers
    // part of it fails the call and nothing is taken.
    assert!(s.client.try_pay_catch_up_contribution(&newcomer).is_err());
    assert_eq!(s.client.get_catch_up_debt(&newcomer), 300);
    assert_eq!(s.balance(&newcomer), 150);

    // Once the balance covers the full debt, the catch-up succeeds.
    token::StellarAssetClient::new(&s.env, &s.token).mint(&newcomer, &150);
    s.client.pay_catch_up_contribution(&newcomer);
    assert_eq!(s.client.get_catch_up_debt(&newcomer), 0);
    assert_eq!(s.balance(&newcomer), 0);
}

#[test]
fn test_partial_late_contribution_only_counts_once_complete() {
    let s = setup();
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));

    s.client.contribute(&m0, &s.token, &CONTRIBUTION);
    s.client.contribute(&m1, &s.token, &CONTRIBUTION);
    s.pass_deadline();

    // A partial payment in the grace period does not yet count as late.
    s.client.contribute(&m2, &s.token, &(CONTRIBUTION / 2));
    assert_eq!(s.late_count(&m2), 0);

    // Completing the contribution in the grace period records one late cycle.
    s.client.contribute(&m2, &s.token, &(CONTRIBUTION / 2));
    assert_eq!(s.late_count(&m2), 1);
}

// ── Late demotion configuration ──────────────────────────────────────────────

#[test]
fn test_late_contrib_threshold_defaults_to_three() {
    let s = setup();
    assert_eq!(s.client.get_late_contrib_threshold(), 3);
}

#[test]
fn test_configure_late_demotion_updates_threshold_and_grace() {
    let s = setup();
    s.client.configure_late_demotion(&s.admin, &2, &500);
    assert_eq!(s.client.get_late_contrib_threshold(), 2);
    assert_eq!(s.client.get_grace_period_seconds(), 500);
}

#[test]
fn test_configure_late_demotion_rejects_non_admin() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    let err = s
        .client
        .try_configure_late_demotion(&outsider, &2, &500)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError::OnlyAdminAllowed.into());
    assert_eq!(s.client.get_late_contrib_threshold(), 3);
}

#[test]
fn test_configure_late_demotion_rejects_zero_threshold() {
    let s = setup();
    let err = s
        .client
        .try_configure_late_demotion(&s.admin, &0, &500)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::InvalidMaxDefaults.into());
}

// ── Late tracking and demotion ───────────────────────────────────────────────

#[test]
fn test_late_contributions_accumulate_and_reset_on_time() {
    let s = setup();
    let m0 = s.member(0);

    s.round_with_late(&m0);
    assert_eq!(s.late_count(&m0), 1);
    s.round_with_late(&m0);
    assert_eq!(s.late_count(&m0), 2);

    // An on-time full contribution resets the consecutive count.
    for m in s.members.iter() {
        s.client.contribute(&m, &s.token, &CONTRIBUTION);
    }
    assert_eq!(s.late_count(&m0), 0);
}

#[test]
fn test_no_demotion_below_threshold() {
    let s = setup();
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));
    s.client.configure_late_demotion(&s.admin, &3, &GRACE_SECONDS);

    s.round_with_late(&m0);
    s.round_with_late(&m0);
    s.round_with_missing(&m0);

    assert_eq!(s.late_count(&m0), 2);
    let order = s.client.get_payout_order();
    assert_eq!(order.get(0).unwrap(), m0);
    assert_eq!(order.get(1).unwrap(), m1);
    assert_eq!(order.get(2).unwrap(), m2);
}

#[test]
fn test_demotion_once_threshold_crossed() {
    let s = setup();
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));
    s.client.configure_late_demotion(&s.admin, &2, &GRACE_SECONDS);

    s.round_with_late(&m0);
    s.round_with_late(&m0);
    assert_eq!(s.late_count(&m0), 2);

    // Demotion is applied when the next round is finalized.
    s.round_with_missing(&m0);

    let order = s.client.get_payout_order();
    assert_eq!(order.len(), 3);
    assert_eq!(order.get(0).unwrap(), m1);
    assert_eq!(order.get(1).unwrap(), m2);
    assert_eq!(order.get(2).unwrap(), m0);
}

#[test]
fn test_on_time_members_are_not_demoted() {
    let s = setup();
    let (m0, m1) = (s.member(0), s.member(1));
    s.client.configure_late_demotion(&s.admin, &2, &GRACE_SECONDS);

    s.round_with_late(&m0);
    s.round_with_late(&m0);
    s.round_with_missing(&m0);

    assert_eq!(s.late_count(&m1), 0);
    let order = s.client.get_payout_order();
    assert_eq!(order.first_index_of(&m1), Some(0));
}

#[test]
fn test_late_contribution_rejected_after_grace_period() {
    let s = setup();
    let (m0, m1, m2) = (s.member(0), s.member(1), s.member(2));

    s.client.contribute(&m0, &s.token, &CONTRIBUTION);
    s.client.contribute(&m1, &s.token, &CONTRIBUTION);
    let deadline = s.client.get_group_info().round_deadline;
    s.env.ledger().set_timestamp(deadline + GRACE_SECONDS + 1);

    let err = s
        .client
        .try_contribute(&m2, &s.token, &CONTRIBUTION)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, Error::ContributionWindowClosed.into());
    assert_eq!(s.late_count(&m2), 0);
}
