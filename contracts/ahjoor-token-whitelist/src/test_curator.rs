#![cfg(test)]
use crate::{ProposalStatus, TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};

fn setup<'a>() -> (Env, Address, TokenWhitelistContractClient<'a>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    let gov_token = env.register_stellar_asset_contract_v2(admin.clone()).address();
    client.set_governance_token(&admin, &gov_token);
    client.set_min_proposal_stake(&admin, &100i128);
    client.set_voting_window_ledgers(&admin, &100u32);
    client.set_enactment_delay_ledgers(&admin, &50u32);
    client.set_quorum_bps(&admin, &5_000u32);

    (env, admin, client, gov_token)
}

fn rationale(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

fn advance(env: &Env, ledgers: u32) {
    env.ledger().with_mut(|l| l.sequence_number += ledgers);
}

// ── Happy paths ───────────────────────────────────────────────────────────────

#[test]
fn test_add_and_remove_curator() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);

    assert!(!client.is_curator(&curator));
    client.add_curator(&admin, &curator);
    assert!(client.is_curator(&curator));
    client.remove_curator(&admin, &curator);
    assert!(!client.is_curator(&curator));
}

#[test]
fn test_curator_proposes_without_stake() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);

    // Curator holds zero governance tokens, below the 100 min stake.
    let id = client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));

    let proposal = client.get_listing_proposal(&id);
    assert_eq!(proposal.proposer, curator);
    assert_eq!(proposal.status, ProposalStatus::Active);
    assert_eq!(client.get_curator_open_proposals(&curator), 1);
}

#[test]
fn test_curator_proposal_still_requires_voting() {
    let (env, admin, client, gov_token) = setup();
    let curator = Address::generate(&env);
    let voter = Address::generate(&env);
    let token = Address::generate(&env);
    client.add_curator(&admin, &curator);
    TokenAdminClient::new(&env, &gov_token).mint(&voter, &500);

    let id = client.propose_token_listing(&curator, &token, &rationale(&env));
    client.vote_listing(&voter, &id, &true, &500i128);

    advance(&env, 101);
    client.finalise_listing_proposal(&id);
    assert_eq!(client.get_listing_proposal(&id).status, ProposalStatus::PendingEnactment);
    // PendingEnactment still counts as open.
    assert_eq!(client.get_curator_open_proposals(&curator), 1);

    advance(&env, 51);
    client.enact_listing(&id);
    assert_eq!(client.get_listing_proposal(&id).status, ProposalStatus::Enacted);
    assert!(client.is_whitelisted(&token));
    assert_eq!(client.get_curator_open_proposals(&curator), 0);
}

#[test]
fn test_curator_proposal_without_votes_fails_and_frees_slot() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    let token = Address::generate(&env);
    client.add_curator(&admin, &curator);

    let id = client.propose_token_listing(&curator, &token, &rationale(&env));
    advance(&env, 101);
    client.finalise_listing_proposal(&id);

    assert_eq!(client.get_listing_proposal(&id).status, ProposalStatus::Failed);
    assert!(!client.is_whitelisted(&token));
    assert_eq!(client.get_curator_open_proposals(&curator), 0);
}

#[test]
fn test_veto_frees_curator_slot() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);

    let id = client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    client.veto_listing_proposal(&admin, &id, &rationale(&env));

    assert_eq!(client.get_listing_proposal(&id).status, ProposalStatus::Vetoed);
    assert_eq!(client.get_curator_open_proposals(&curator), 0);
}

#[test]
fn test_veto_after_failure_does_not_double_release() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);

    let failed = client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    let open = client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    assert_eq!(client.get_curator_open_proposals(&curator), 2);

    // Finalise only the first proposal: move past its deadline, then veto the
    // already-failed proposal. The second one must still count as open.
    advance(&env, 101);
    client.finalise_listing_proposal(&failed);
    assert_eq!(client.get_curator_open_proposals(&curator), 1);
    client.veto_listing_proposal(&admin, &failed, &rationale(&env));
    assert_eq!(client.get_curator_open_proposals(&curator), 1);
    assert_eq!(client.get_listing_proposal(&open).status, ProposalStatus::Active);
}

#[test]
fn test_default_and_custom_curator_limit() {
    let (_, admin, client, _) = setup();
    assert_eq!(client.get_curator_max_open_proposals(), 3);
    client.set_curator_max_open_proposals(&admin, &5u32);
    assert_eq!(client.get_curator_max_open_proposals(), 5);
}

#[test]
fn test_curator_can_propose_again_after_slot_freed() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);
    client.set_curator_max_open_proposals(&admin, &1u32);

    let id = client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    client.veto_listing_proposal(&admin, &id, &rationale(&env));

    client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    assert_eq!(client.get_curator_open_proposals(&curator), 1);
}

#[test]
fn test_staked_proposals_do_not_count_toward_curator_limit() {
    let (env, _, client, gov_token) = setup();
    let proposer = Address::generate(&env);
    TokenAdminClient::new(&env, &gov_token).mint(&proposer, &100);

    let id = client.propose_token_listing(&proposer, &Address::generate(&env), &rationale(&env));
    assert_eq!(client.get_curator_open_proposals(&proposer), 0);
    advance(&env, 101);
    client.finalise_listing_proposal(&id);
    assert_eq!(client.get_curator_open_proposals(&proposer), 0);
}

#[test]
fn test_removed_curator_open_proposal_keeps_running() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);

    let id = client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    client.remove_curator(&admin, &curator);

    assert_eq!(client.get_listing_proposal(&id).status, ProposalStatus::Active);
    advance(&env, 101);
    client.finalise_listing_proposal(&id);
    assert_eq!(client.get_curator_open_proposals(&curator), 0);
}

// ── Error branches ────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #21)")]
fn test_add_curator_twice_fails() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);
    client.add_curator(&admin, &curator);
}

#[test]
#[should_panic(expected = "Error(Contract, #22)")]
fn test_remove_unknown_curator_fails() {
    let (env, admin, client, _) = setup();
    client.remove_curator(&admin, &Address::generate(&env));
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_curator_open_proposal_limit_enforced() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);
    client.set_curator_max_open_proposals(&admin, &2u32);

    client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_pending_enactment_counts_toward_limit() {
    let (env, admin, client, gov_token) = setup();
    let curator = Address::generate(&env);
    let voter = Address::generate(&env);
    client.add_curator(&admin, &curator);
    client.set_curator_max_open_proposals(&admin, &1u32);
    TokenAdminClient::new(&env, &gov_token).mint(&voter, &500);

    let id = client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
    client.vote_listing(&voter, &id, &true, &500i128);
    advance(&env, 101);
    client.finalise_listing_proposal(&id);

    client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_zero_curator_limit_rejected() {
    let (_, admin, client, _) = setup();
    client.set_curator_max_open_proposals(&admin, &0u32);
}

#[test]
#[should_panic(expected = "InsufficientProposerStake")]
fn test_removed_curator_needs_stake() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);
    client.remove_curator(&admin, &curator);
    client.propose_token_listing(&curator, &Address::generate(&env), &rationale(&env));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_admin_cannot_add_curator() {
    let (env, _, client, _) = setup();
    let stranger = Address::generate(&env);
    client.add_curator(&stranger, &Address::generate(&env));
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_admin_cannot_remove_curator() {
    let (env, admin, client, _) = setup();
    let curator = Address::generate(&env);
    client.add_curator(&admin, &curator);
    client.remove_curator(&Address::generate(&env), &curator);
}

#[test]
#[should_panic(expected = "Unauthorized")]
fn test_non_admin_cannot_set_curator_limit() {
    let (env, _, client, _) = setup();
    client.set_curator_max_open_proposals(&Address::generate(&env), &5u32);
}
