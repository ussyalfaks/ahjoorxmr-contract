#![cfg(test)]
//! Tests for co-payer contribution splits:
//! `register_co_payer_splits`, `contribute_split`, `revoke_co_payer_splits`
//! and `get_co_payer_splits`.

use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

use crate::{AhjoorContract, AhjoorContractClient, CoPayerSplit, PayoutStrategy, RoscaConfig, VotingMode};

const CONTRIBUTION: i128 = 1_000;

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
    token: Address,
    token_client: TokenClient<'a>,
    token_admin: TokenAdminClient<'a>,
    members: Vec<Address>,
}

/// Three-member group so a single contribution never completes the round.
fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    // Co-payers authorize only the nested token transfer inside
    // `contribute_split`, so non-root auth must be mocked too.
    env.mock_all_auths_allowing_non_root_auth();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token);
    let token_admin = TokenAdminClient::new(&env, &token);

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
        &1000u64,
        &make_config(),
        &None,
    );

    Setup {
        env,
        client,
        contract_id,
        admin,
        token,
        token_client,
        token_admin,
        members,
    }
}

fn split(co_payer: &Address, amount: i128) -> CoPayerSplit {
    CoPayerSplit {
        co_payer: co_payer.clone(),
        amount,
    }
}

/// Generate a funded co-payer.
fn co_payer(s: &Setup, balance: i128) -> Address {
    let p = Address::generate(&s.env);
    if balance > 0 {
        s.token_admin.mint(&p, &balance);
    }
    p
}

fn is_paid(s: &Setup, member: &Address) -> bool {
    s.client.get_state().1.contains(member)
}

// ── Registration ────────────────────────────────────────────────────────────

#[test]
fn test_register_and_get_co_payer_splits() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);
    let y = co_payer(&s, 0);

    let splits = Vec::from_array(&s.env, [split(&x, 600), split(&y, 400)]);
    s.client.register_co_payer_splits(&member, &splits);

    let stored = s.client.get_co_payer_splits(&member);
    assert_eq!(stored, splits);
    assert_eq!(stored.len(), 2);
}

#[test]
fn test_get_co_payer_splits_empty_when_unregistered() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    assert_eq!(s.client.get_co_payer_splits(&member).len(), 0);
}

#[test]
fn test_single_co_payer_covering_full_amount() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);

    let splits = Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]);
    s.client.register_co_payer_splits(&member, &splits);
    assert_eq!(s.client.get_co_payer_splits(&member), splits);
}

#[test]
#[should_panic(expected = "Error(Contract, #115)")]
fn test_register_splits_summing_below_required_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);
    let y = co_payer(&s, 0);

    // 60% + 30% = 90% of the required contribution.
    let splits = Vec::from_array(&s.env, [split(&x, 600), split(&y, 300)]);
    s.client.register_co_payer_splits(&member, &splits);
}

#[test]
#[should_panic(expected = "Error(Contract, #115)")]
fn test_register_splits_summing_above_required_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);
    let y = co_payer(&s, 0);

    // 60% + 50% = 110% of the required contribution.
    let splits = Vec::from_array(&s.env, [split(&x, 600), split(&y, 500)]);
    s.client.register_co_payer_splits(&member, &splits);
}

#[test]
#[should_panic(expected = "Error(Contract, #115)")]
fn test_register_empty_splits_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client
        .register_co_payer_splits(&member, &Vec::<CoPayerSplit>::new(&s.env));
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_register_zero_amount_split_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);
    let y = co_payer(&s, 0);

    let splits = Vec::from_array(&s.env, [split(&x, CONTRIBUTION), split(&y, 0)]);
    s.client.register_co_payer_splits(&member, &splits);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_register_negative_amount_split_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);
    let y = co_payer(&s, 0);

    // Sums to the required amount, but a negative share is never valid.
    let splits = Vec::from_array(&s.env, [split(&x, 1_200), split(&y, -200)]);
    s.client.register_co_payer_splits(&member, &splits);
}

#[test]
fn test_register_splits_respects_member_tier() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);
    let y = co_payer(&s, 0);

    // 2x tier → required contribution is 2_000.
    s.client.set_member_tier(&s.admin, &member, &20_000);

    let base_only = Vec::from_array(&s.env, [split(&x, 600), split(&y, 400)]);
    assert!(s
        .client
        .try_register_co_payer_splits(&member, &base_only)
        .is_err());

    let tiered = Vec::from_array(&s.env, [split(&x, 1_500), split(&y, 500)]);
    s.client.register_co_payer_splits(&member, &tiered);
    assert_eq!(s.client.get_co_payer_splits(&member), tiered);
}

#[test]
#[should_panic(expected = "Error(Contract, #117)")]
fn test_register_twice_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);

    let splits = Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]);
    s.client.register_co_payer_splits(&member, &splits);
    s.client.register_co_payer_splits(&member, &splits);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_register_by_non_member_rejected() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    let x = co_payer(&s, 0);

    let splits = Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]);
    s.client.register_co_payer_splits(&outsider, &splits);
}

// ── Contributing via splits ────────────────────────────────────────────────

#[test]
fn test_contribute_split_transfers_from_each_co_payer() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 1_000);
    let y = co_payer(&s, 1_000);

    s.client.register_co_payer_splits(
        &member,
        &Vec::from_array(&s.env, [split(&x, 600), split(&y, 400)]),
    );

    let member_before = s.token_client.balance(&member);
    s.client.contribute_split(&member, &s.token);

    assert_eq!(s.token_client.balance(&x), 400);
    assert_eq!(s.token_client.balance(&y), 600);
    assert_eq!(s.token_client.balance(&s.contract_id), CONTRIBUTION);
    // The member's own balance is untouched.
    assert_eq!(s.token_client.balance(&member), member_before);

    assert!(is_paid(&s, &member));
    let (contributed, remaining) = s.client.get_member_contribution_status(&member);
    assert_eq!(contributed, CONTRIBUTION);
    assert_eq!(remaining, 0);
}

#[test]
fn test_partial_funding_reverts_whole_split_contribution() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 1_000);
    // y only holds 100 of its 400 share.
    let y = co_payer(&s, 100);

    s.client.register_co_payer_splits(
        &member,
        &Vec::from_array(&s.env, [split(&x, 600), split(&y, 400)]),
    );

    assert!(s.client.try_contribute_split(&member, &s.token).is_err());

    // Nothing moved: x's 600 transfer was rolled back with the failed call.
    assert_eq!(s.token_client.balance(&x), 1_000);
    assert_eq!(s.token_client.balance(&y), 100);
    assert_eq!(s.token_client.balance(&s.contract_id), 0);
    assert!(!is_paid(&s, &member));
    assert_eq!(s.client.get_member_contribution_status(&member).0, 0);

    // Once the co-payer is fully funded the contribution goes through.
    s.token_admin.mint(&y, &300);
    s.client.contribute_split(&member, &s.token);
    assert_eq!(s.token_client.balance(&x), 400);
    assert_eq!(s.token_client.balance(&y), 0);
    assert_eq!(s.token_client.balance(&s.contract_id), CONTRIBUTION);
    assert!(is_paid(&s, &member));
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_contribute_split_twice_in_same_round_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 5_000);

    s.client
        .register_co_payer_splits(&member, &Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]));
    s.client.contribute_split(&member, &s.token);
    s.client.contribute_split(&member, &s.token);
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_contribute_split_after_direct_contribution_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 5_000);

    s.client
        .register_co_payer_splits(&member, &Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]));
    s.client.contribute(&member, &s.token, &CONTRIBUTION);
    s.client.contribute_split(&member, &s.token);
}

#[test]
#[should_panic(expected = "Error(Contract, #114)")]
fn test_contribute_split_without_registration_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.contribute_split(&member, &s.token);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_contribute_split_with_unapproved_token_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 5_000);
    let other_token = s
        .env
        .register_stellar_asset_contract_v2(s.admin.clone())
        .address();

    s.client
        .register_co_payer_splits(&member, &Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]));
    s.client.contribute_split(&member, &other_token);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_contribute_split_by_non_member_rejected() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    s.client.contribute_split(&outsider, &s.token);
}

// ── Revocation ─────────────────────────────────────────────────────────────

#[test]
fn test_revoke_clears_splits() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 0);

    s.client
        .register_co_payer_splits(&member, &Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]));
    s.client.revoke_co_payer_splits(&member);

    assert_eq!(s.client.get_co_payer_splits(&member).len(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #114)")]
fn test_revoke_without_registration_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    s.client.revoke_co_payer_splits(&member);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_revoke_by_non_member_rejected() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    s.client.revoke_co_payer_splits(&outsider);
}

#[test]
fn test_reregister_after_revoke_uses_new_splits() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let x = co_payer(&s, 5_000);
    let y = co_payer(&s, 5_000);

    s.client
        .register_co_payer_splits(&member, &Vec::from_array(&s.env, [split(&x, CONTRIBUTION)]));
    s.client.revoke_co_payer_splits(&member);

    let new_splits = Vec::from_array(&s.env, [split(&y, CONTRIBUTION)]);
    s.client.register_co_payer_splits(&member, &new_splits);
    assert_eq!(s.client.get_co_payer_splits(&member), new_splits);

    s.client.contribute_split(&member, &s.token);
    // Only the newly registered co-payer was charged.
    assert_eq!(s.token_client.balance(&x), 5_000);
    assert_eq!(s.token_client.balance(&y), 4_000);
}

#[test]
fn test_revocation_mid_round() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let xa = co_payer(&s, 5_000);
    let xb = co_payer(&s, 5_000);

    s.client
        .register_co_payer_splits(&a, &Vec::from_array(&s.env, [split(&xa, CONTRIBUTION)]));
    s.client
        .register_co_payer_splits(&b, &Vec::from_array(&s.env, [split(&xb, CONTRIBUTION)]));

    // A pays through co-payers, then B revokes before paying.
    s.client.contribute_split(&a, &s.token);
    s.client.revoke_co_payer_splits(&b);

    // B can no longer pay through the revoked split, and its co-payer is untouched.
    assert!(s.client.try_contribute_split(&b, &s.token).is_err());
    assert_eq!(s.token_client.balance(&xb), 5_000);
    assert!(!is_paid(&s, &b));

    // B still meets the round obligation directly.
    let b_before = s.token_client.balance(&b);
    s.client.contribute(&b, &s.token, &CONTRIBUTION);
    assert!(is_paid(&s, &b));
    assert_eq!(s.token_client.balance(&b), b_before - CONTRIBUTION);

    // A revoking after paying does not undo A's contribution.
    s.client.revoke_co_payer_splits(&a);
    assert!(is_paid(&s, &a));
    assert_eq!(s.client.get_member_contribution_status(&a).0, CONTRIBUTION);
    assert_eq!(s.token_client.balance(&s.contract_id), 2 * CONTRIBUTION);
}
