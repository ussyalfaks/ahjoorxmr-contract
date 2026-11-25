#![cfg(test)]
//! Tests for multi-token contributions: `set_exchange_rate`,
//! `get_exchange_rates`, `set_token_limit` and `get_token_limits`.
//!
//! Rates use 10^7 precision: a rate of `20_000_000` means one unit of the
//! alternate token is worth two units of the base token, so a base-valued
//! contribution of `amount` transfers `amount * 10^7 / rate` alternate tokens.

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, MockAuth, MockAuthInvoke},
    Address, Env, IntoVal, Vec,
};

use crate::{AhjoorContract, AhjoorContractClient, PayoutStrategy, RoscaConfig, VotingMode};

const CONTRIBUTION: i128 = 1_000;
const RATE_PRECISION: i128 = 10_000_000;

fn make_config() -> RoscaConfig {
    RoscaConfig {
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
        max_members: Some(10),
        skip_fee: 0,
        max_skips_per_cycle: 5,
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

struct Setup<'a> {
    env: Env,
    client: AhjoorContractClient<'a>,
    contract_id: Address,
    admin: Address,
    base_token: Address,
    base_client: TokenClient<'a>,
    alt_token: Address,
    alt_client: TokenClient<'a>,
    members: Vec<Address>,
}

/// Three-member group with a base token and an approved (but un-rated)
/// alternate token. Every member holds 10_000 of each.
fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let base_token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let alt_token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let base_client = TokenClient::new(&env, &base_token);
    let alt_client = TokenClient::new(&env, &alt_token);
    let base_admin = TokenAdminClient::new(&env, &base_token);
    let alt_admin = TokenAdminClient::new(&env, &alt_token);

    let mut members = Vec::new(&env);
    for _ in 0..3 {
        let m = Address::generate(&env);
        base_admin.mint(&m, &10_000);
        alt_admin.mint(&m, &10_000);
        members.push_back(m);
    }

    client.init(
        &admin,
        &members,
        &CONTRIBUTION,
        &base_token,
        &1000u64,
        &make_config(),
        &None,
    );
    client.add_approved_token(&alt_token);

    Setup {
        env,
        client,
        contract_id,
        admin,
        base_token,
        base_client,
        alt_token,
        alt_client,
        members,
    }
}

// ── Exchange rates: storage and getters ────────────────────────────────────

#[test]
fn test_exchange_rates_empty_by_default() {
    let s = setup();
    assert_eq!(s.client.get_exchange_rates().len(), 0);
    assert_eq!(s.client.get_token_limits().len(), 0);
}

#[test]
fn test_set_and_get_exchange_rate() {
    let s = setup();
    s.client.set_exchange_rate(&s.alt_token, &(2 * RATE_PRECISION));

    let rates = s.client.get_exchange_rates();
    assert_eq!(rates.len(), 1);
    assert_eq!(rates.get(s.alt_token.clone()), Some(2 * RATE_PRECISION));
}

#[test]
fn test_set_exchange_rate_overwrites_previous_value() {
    let s = setup();
    s.client.set_exchange_rate(&s.alt_token, &(2 * RATE_PRECISION));
    s.client.set_exchange_rate(&s.alt_token, &(5 * RATE_PRECISION));

    let rates = s.client.get_exchange_rates();
    assert_eq!(rates.len(), 1);
    assert_eq!(rates.get(s.alt_token.clone()), Some(5 * RATE_PRECISION));
}

// ── Contributing in a non-base token ───────────────────────────────────────

#[test]
fn test_contribute_in_alt_token_at_set_rate() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    // 1 alt = 2 base.
    s.client.set_exchange_rate(&s.alt_token, &(2 * RATE_PRECISION));

    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);

    // 1_000 base-valued → 500 alt tokens transferred.
    assert_eq!(s.alt_client.balance(&member), 10_000 - 500);
    assert_eq!(s.alt_client.balance(&s.contract_id), 500);
    // Base token untouched.
    assert_eq!(s.base_client.balance(&member), 10_000);
    assert_eq!(s.base_client.balance(&s.contract_id), 0);

    // Obligation is tracked in base-token value.
    let (contributed, remaining) = s.client.get_member_contribution_status(&member);
    assert_eq!(contributed, CONTRIBUTION);
    assert_eq!(remaining, 0);
    assert!(s.client.get_state().1.contains(&member));
}

#[test]
fn test_contribute_in_alt_token_below_parity_rate() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    // 1 alt = 0.5 base → twice as many alt tokens are needed.
    s.client.set_exchange_rate(&s.alt_token, &(RATE_PRECISION / 2));

    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);

    assert_eq!(s.alt_client.balance(&member), 10_000 - 2_000);
    assert_eq!(s.alt_client.balance(&s.contract_id), 2_000);
}

#[test]
fn test_rate_change_applies_to_later_contributions() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    s.client.set_exchange_rate(&s.alt_token, &(2 * RATE_PRECISION));
    s.client.contribute(&a, &s.alt_token, &CONTRIBUTION);

    s.client.set_exchange_rate(&s.alt_token, &(4 * RATE_PRECISION));
    s.client.contribute(&b, &s.alt_token, &CONTRIBUTION);

    assert_eq!(s.alt_client.balance(&a), 10_000 - 500);
    assert_eq!(s.alt_client.balance(&b), 10_000 - 250);
}

#[test]
fn test_base_token_ignores_exchange_rate() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    // A rate on the base token must not rescale base-token contributions.
    s.client.set_exchange_rate(&s.base_token, &(2 * RATE_PRECISION));

    s.client.contribute(&member, &s.base_token, &CONTRIBUTION);
    assert_eq!(s.base_client.balance(&member), 10_000 - CONTRIBUTION);
}

#[test]
#[should_panic(expected = "Exchange rate not set")]
fn test_contribute_in_alt_token_without_rate_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_contribute_with_zero_rate_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_exchange_rate(&s.alt_token, &0);
    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);
}

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_contribute_with_negative_rate_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_exchange_rate(&s.alt_token, &-RATE_PRECISION);
    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_contribute_in_rated_but_unapproved_token_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let unapproved = s
        .env
        .register_stellar_asset_contract_v2(s.admin.clone())
        .address();

    s.client.set_exchange_rate(&unapproved, &RATE_PRECISION);
    s.client.contribute(&member, &unapproved, &CONTRIBUTION);
}

// ── Token limits ───────────────────────────────────────────────────────────

#[test]
fn test_set_and_get_token_limit() {
    let s = setup();
    s.client.set_token_limit(&s.alt_token, &400);
    s.client.set_token_limit(&s.base_token, &900);

    let limits = s.client.get_token_limits();
    assert_eq!(limits.len(), 2);
    assert_eq!(limits.get(s.alt_token.clone()), Some(400));
    assert_eq!(limits.get(s.base_token.clone()), Some(900));

    s.client.set_token_limit(&s.alt_token, &700);
    assert_eq!(s.client.get_token_limits().get(s.alt_token.clone()), Some(700));
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_alt_token_transfer_above_limit_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_exchange_rate(&s.alt_token, &(2 * RATE_PRECISION));
    // 1_000 base → 500 alt, which exceeds the 499 limit.
    s.client.set_token_limit(&s.alt_token, &499);

    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);
}

#[test]
fn test_alt_token_transfer_at_limit_allowed() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_exchange_rate(&s.alt_token, &(2 * RATE_PRECISION));
    // Limit is checked against the converted alt-token amount (500).
    s.client.set_token_limit(&s.alt_token, &500);

    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);
    assert_eq!(s.alt_client.balance(&s.contract_id), 500);
}

#[test]
fn test_limit_allows_partial_contributions_under_cap() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_exchange_rate(&s.alt_token, &(2 * RATE_PRECISION));
    s.client.set_token_limit(&s.alt_token, &300);

    // Each 500 base-valued payment converts to 250 alt, under the 300 cap.
    s.client.contribute(&member, &s.alt_token, &500);
    s.client.contribute(&member, &s.alt_token, &500);

    assert_eq!(s.alt_client.balance(&s.contract_id), 500);
    assert_eq!(s.client.get_member_contribution_status(&member).0, CONTRIBUTION);
}

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_base_token_limit_enforced() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_token_limit(&s.base_token, &(CONTRIBUTION - 1));
    s.client.contribute(&member, &s.base_token, &CONTRIBUTION);
}

#[test]
fn test_limit_on_one_token_does_not_affect_another() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_token_limit(&s.alt_token, &1);

    s.client.contribute(&member, &s.base_token, &CONTRIBUTION);
    assert_eq!(s.base_client.balance(&s.contract_id), CONTRIBUTION);
}

// ── Unauthorized callers ───────────────────────────────────────────────────

#[test]
fn test_set_exchange_rate_by_non_admin_rejected() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    let rate = 2 * RATE_PRECISION;

    s.env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.contract_id,
            fn_name: "set_exchange_rate",
            args: (s.alt_token.clone(), rate).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);

    assert!(s.client.try_set_exchange_rate(&s.alt_token, &rate).is_err());
    assert_eq!(s.client.get_exchange_rates().len(), 0);
}

#[test]
fn test_set_token_limit_by_non_admin_rejected() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    let limit: i128 = 100;

    s.env.mock_auths(&[MockAuth {
        address: &stranger,
        invoke: &MockAuthInvoke {
            contract: &s.contract_id,
            fn_name: "set_token_limit",
            args: (s.alt_token.clone(), limit).into_val(&s.env),
            sub_invokes: &[],
        },
    }]);

    assert!(s.client.try_set_token_limit(&s.alt_token, &limit).is_err());
    assert_eq!(s.client.get_token_limits().len(), 0);
}

#[test]
fn test_set_exchange_rate_without_any_auth_rejected() {
    let s = setup();
    s.env.mock_auths(&[]);

    assert!(s
        .client
        .try_set_exchange_rate(&s.alt_token, &RATE_PRECISION)
        .is_err());
    assert!(s.client.try_set_token_limit(&s.alt_token, &100).is_err());
}

// ── Overflow on large rates ────────────────────────────────────────────────

#[test]
fn test_max_rate_is_stored_without_truncation() {
    let s = setup();
    s.client.set_exchange_rate(&s.alt_token, &i128::MAX);
    assert_eq!(
        s.client.get_exchange_rates().get(s.alt_token.clone()),
        Some(i128::MAX)
    );

    s.client.set_token_limit(&s.alt_token, &i128::MAX);
    assert_eq!(
        s.client.get_token_limits().get(s.alt_token.clone()),
        Some(i128::MAX)
    );
}

#[test]
fn test_rate_conversion_overflow_fails_instead_of_wrapping() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.set_exchange_rate(&s.alt_token, &i128::MAX);

    // `amount * 10^7` overflows i128 for this amount; the call must abort
    // rather than wrap into a bogus transfer amount.
    let huge = i128::MAX / RATE_PRECISION + 1;
    assert!(s
        .client
        .try_contribute(&member, &s.alt_token, &huge)
        .is_err());

    assert_eq!(s.alt_client.balance(&member), 10_000);
    assert_eq!(s.alt_client.balance(&s.contract_id), 0);
    assert_eq!(s.client.get_member_contribution_status(&member).0, 0);
    assert!(!s.client.get_state().1.contains(&member));
}

#[test]
fn test_large_rate_contribution_respects_member_obligation() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    // A very large (but non-overflowing) rate makes each alt token worth a lot
    // of base value; the base-valued obligation still caps the contribution.
    s.client.set_exchange_rate(&s.alt_token, &(1_000 * RATE_PRECISION));

    assert!(s
        .client
        .try_contribute(&member, &s.alt_token, &(CONTRIBUTION + 1))
        .is_err());

    s.client.contribute(&member, &s.alt_token, &CONTRIBUTION);
    // 1_000 base-valued at 1 alt = 1_000 base → 1 alt token.
    assert_eq!(s.alt_client.balance(&member), 10_000 - 1);
    assert_eq!(s.client.get_member_contribution_status(&member).0, CONTRIBUTION);
}
