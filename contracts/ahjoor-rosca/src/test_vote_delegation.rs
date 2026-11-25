#![cfg(test)]
//! Tests for governance vote delegation and proxy voting:
//! `delegate_vote`, `revoke_delegation`, `get_vote_delegation`,
//! `vote_proposal_via_proxy` and `revoke_contrib_vote_delegation`.

use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, Address, Env, String, Vec};

use crate::{
    AhjoorContract, AhjoorContractClient, PayoutStrategy, ProposalType, RoscaConfig, VotingMode,
};

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
    admin: Address,
    members: Vec<Address>,
}

/// Four-member group (Equal voting: every member has weight 1) with one open
/// `RuleChange` proposal, id 0.
fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin = TokenAdminClient::new(&env, &token);

    let mut members = Vec::new(&env);
    for _ in 0..4 {
        let m = Address::generate(&env);
        token_admin.mint(&m, &10_000);
        members.push_back(m);
    }

    client.init(&admin, &members, &1_000i128, &token, &1000u64, &make_config(), &None);

    client.create_proposal(
        &members.get(0).unwrap(),
        &ProposalType::RuleChange,
        &String::from_str(&env, "rule change"),
        &admin,
        &86_400,
        &None,
    );

    Setup {
        env,
        client,
        admin,
        members,
    }
}

const PROPOSAL: u32 = 0;

fn tally(s: &Setup) -> (i128, i128) {
    let p = s.client.get_proposal(&PROPOSAL).unwrap();
    (p.votes_for, p.votes_against)
}

fn future_expiry(s: &Setup) -> u64 {
    s.env.ledger().sequence() as u64 + 1_000
}

// ── delegate_vote / get_vote_delegation ────────────────────────────────────

#[test]
fn test_delegate_vote_records_delegation() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    assert_eq!(s.client.get_vote_delegation(&a), None);
    s.client.delegate_vote(&a, &b);
    assert_eq!(s.client.get_vote_delegation(&a), Some(b.clone()));
    // Delegation is one-directional.
    assert_eq!(s.client.get_vote_delegation(&b), None);
}

#[test]
fn test_redelegate_replaces_previous_delegate() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let c = s.members.get(2).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.delegate_vote(&a, &c);
    assert_eq!(s.client.get_vote_delegation(&a), Some(c));
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_delegate_vote_by_non_member_rejected() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    s.client.delegate_vote(&outsider, &s.members.get(1).unwrap());
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_delegate_vote_to_non_member_rejected() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    s.client.delegate_vote(&s.members.get(0).unwrap(), &outsider);
}

#[test]
#[should_panic(expected = "Error(Contract, #41)")]
fn test_delegate_vote_to_delegating_member_rejected() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let c = s.members.get(2).unwrap();

    // b already delegates to c, so a cannot chain through b.
    s.client.delegate_vote(&b, &c);
    s.client.delegate_vote(&a, &b);
}

// ── Delegate votes with combined weight ────────────────────────────────────

#[test]
fn test_delegate_votes_with_combined_weight() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();
    let c = s.members.get(2).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.delegate_vote(&c, &b);

    // b's own weight + a's + c's.
    s.client.vote_on_proposal(&b, &PROPOSAL, &true);
    assert_eq!(tally(&s), (3, 0));

    // The remaining, non-delegating member votes with only their own weight.
    s.client.vote_on_proposal(&s.members.get(3).unwrap(), &PROPOSAL, &false);
    assert_eq!(tally(&s), (3, 1));
}

#[test]
fn test_delegate_against_vote_carries_delegator_weight() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.vote_on_proposal(&b, &PROPOSAL, &false);
    assert_eq!(tally(&s), (0, 2));
}

#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_delegator_cannot_vote_directly() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.vote_on_proposal(&a, &PROPOSAL, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_delegator_cannot_vote_directly_after_delegate_voted() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.vote_on_proposal(&b, &PROPOSAL, &true);
    s.client.vote_on_proposal(&a, &PROPOSAL, &false);
}

// ── revoke_delegation ──────────────────────────────────────────────────────

#[test]
fn test_revoke_before_vote_restores_direct_voting() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.revoke_delegation(&a);
    assert_eq!(s.client.get_vote_delegation(&a), None);

    // b now only carries their own weight.
    s.client.vote_on_proposal(&b, &PROPOSAL, &true);
    assert_eq!(tally(&s), (1, 0));

    // a votes for themselves.
    s.client.vote_on_proposal(&a, &PROPOSAL, &false);
    assert_eq!(tally(&s), (1, 1));
}

#[test]
fn test_revoke_after_delegate_voted_does_not_allow_double_vote() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.vote_on_proposal(&b, &PROPOSAL, &true);
    assert_eq!(tally(&s), (2, 0));

    s.client.revoke_delegation(&a);
    assert_eq!(s.client.get_vote_delegation(&a), None);

    // a's weight was already cast through b on this proposal.
    assert!(s.client.try_vote_on_proposal(&a, &PROPOSAL, &false).is_err());
    assert_eq!(tally(&s), (2, 0));
}

#[test]
fn test_revoke_after_vote_applies_to_next_proposal() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    let b = s.members.get(1).unwrap();

    s.client.delegate_vote(&a, &b);
    s.client.vote_on_proposal(&b, &PROPOSAL, &true);
    s.client.revoke_delegation(&a);

    s.client.create_proposal(
        &a,
        &ProposalType::RuleChange,
        &String::from_str(&s.env, "second"),
        &s.admin,
        &86_400,
        &None,
    );
    let next: u32 = PROPOSAL + 1;

    s.client.vote_on_proposal(&b, &next, &true);
    s.client.vote_on_proposal(&a, &next, &false);
    let p = s.client.get_proposal(&next).unwrap();
    assert_eq!((p.votes_for, p.votes_against), (1, 1));
}

#[test]
#[should_panic(expected = "Error(Contract, #39)")]
fn test_revoke_without_delegation_rejected() {
    let s = setup();
    s.client.revoke_delegation(&s.members.get(0).unwrap());
}

#[test]
#[should_panic(expected = "Error(Contract, #39)")]
fn test_revoke_twice_rejected() {
    let s = setup();
    let a = s.members.get(0).unwrap();
    s.client.delegate_vote(&a, &s.members.get(1).unwrap());
    s.client.revoke_delegation(&a);
    s.client.revoke_delegation(&a);
}

// ── Contribution-weight delegation: proxy voting ───────────────────────────

#[test]
fn test_vote_proposal_via_proxy_records_member_vote() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let proxy = s.members.get(1).unwrap();

    s.client
        .delegate_contribution_vote(&member, &proxy, &future_expiry(&s));
    s.client
        .vote_proposal_via_proxy(&proxy, &member, &PROPOSAL, &true);
    assert_eq!(tally(&s), (1, 0));

    // The proxy still holds their own vote.
    s.client.vote_on_proposal(&proxy, &PROPOSAL, &false);
    assert_eq!(tally(&s), (1, 1));
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_proxy_cannot_vote_twice_for_same_member() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let proxy = s.members.get(1).unwrap();

    s.client
        .delegate_contribution_vote(&member, &proxy, &future_expiry(&s));
    s.client
        .vote_proposal_via_proxy(&proxy, &member, &PROPOSAL, &true);
    s.client
        .vote_proposal_via_proxy(&proxy, &member, &PROPOSAL, &false);
}

#[test]
#[should_panic(expected = "Error(Contract, #86)")]
fn test_vote_via_proxy_by_wrong_proxy_rejected() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let proxy = s.members.get(1).unwrap();
    let impostor = s.members.get(2).unwrap();

    s.client
        .delegate_contribution_vote(&member, &proxy, &future_expiry(&s));
    s.client
        .vote_proposal_via_proxy(&impostor, &member, &PROPOSAL, &true);
}

#[test]
#[should_panic(expected = "Error(Contract, #39)")]
fn test_vote_via_proxy_without_delegation_rejected() {
    let s = setup();
    s.client.vote_proposal_via_proxy(
        &s.members.get(1).unwrap(),
        &s.members.get(0).unwrap(),
        &PROPOSAL,
        &true,
    );
}

#[test]
#[should_panic(expected = "Error(Contract, #40)")]
fn test_contrib_delegator_cannot_vote_directly() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let proxy = s.members.get(1).unwrap();

    s.client
        .delegate_contribution_vote(&member, &proxy, &future_expiry(&s));
    s.client.vote_on_proposal(&member, &PROPOSAL, &true);
}

#[test]
fn test_contrib_delegate_direct_vote_carries_delegator_weight() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let proxy = s.members.get(1).unwrap();

    s.client
        .delegate_contribution_vote(&member, &proxy, &future_expiry(&s));
    s.client.vote_on_proposal(&proxy, &PROPOSAL, &true);
    assert_eq!(tally(&s), (2, 0));

    // The delegator's vote is already counted, so the proxy path is closed too.
    assert!(s
        .client
        .try_vote_proposal_via_proxy(&proxy, &member, &PROPOSAL, &false)
        .is_err());
}

// ── revoke_contrib_vote_delegation ─────────────────────────────────────────

#[test]
fn test_revoke_contrib_delegation_before_vote() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let proxy = s.members.get(1).unwrap();

    s.client
        .delegate_contribution_vote(&member, &proxy, &future_expiry(&s));
    s.client.revoke_contrib_vote_delegation(&member);

    // The proxy can no longer vote for the member...
    assert!(s
        .client
        .try_vote_proposal_via_proxy(&proxy, &member, &PROPOSAL, &true)
        .is_err());
    assert_eq!(tally(&s), (0, 0));

    // ...and the member votes directly instead.
    s.client.vote_on_proposal(&member, &PROPOSAL, &false);
    assert_eq!(tally(&s), (0, 1));
}

#[test]
fn test_revoke_contrib_delegation_after_proxy_vote() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let proxy = s.members.get(1).unwrap();

    s.client
        .delegate_contribution_vote(&member, &proxy, &future_expiry(&s));
    s.client
        .vote_proposal_via_proxy(&proxy, &member, &PROPOSAL, &true);
    s.client.revoke_contrib_vote_delegation(&member);

    // The proxy's vote stands and the member cannot vote a second time.
    assert!(s.client.try_vote_on_proposal(&member, &PROPOSAL, &false).is_err());
    assert_eq!(tally(&s), (1, 0));
}

#[test]
#[should_panic(expected = "Error(Contract, #39)")]
fn test_revoke_contrib_delegation_without_delegation_rejected() {
    let s = setup();
    s.client
        .revoke_contrib_vote_delegation(&s.members.get(0).unwrap());
}
