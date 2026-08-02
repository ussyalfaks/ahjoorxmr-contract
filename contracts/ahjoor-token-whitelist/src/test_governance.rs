#![cfg(test)]
use crate::{ProposalStatus, TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env,
};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;

fn setup_governance<'a>() -> (
    Env,
    Address,                          // admin
    TokenWhitelistContractClient<'a>, // whitelist client
    Address,                          // governance token address
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    // Deploy a stellar asset as the governance token
    let gov_token = env.register_stellar_asset_contract_v2(admin.clone()).address();

    // Configure governance
    client.set_governance_token(&admin, &gov_token);
    client.set_min_proposal_stake(&admin, &100i128);
    client.set_voting_window_ledgers(&admin, &100u32);
    client.set_enactment_delay_ledgers(&admin, &50u32);
    client.set_quorum_bps(&admin, &5_000u32); // 50%

    (env, admin, client, gov_token)
}

fn mint_gov_tokens(env: &Env, gov_token: &Address, admin: &Address, to: &Address, amount: i128) {
    TokenAdminClient::new(env, gov_token).mint(to, &amount);
}

#[test]
fn test_full_proposal_vote_enact_lifecycle() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter1 = Address::generate(&env);
    let voter2 = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter1, 600);
    mint_gov_tokens(&env, &gov_token, &admin, &voter2, 400);

    let rationale = BytesN::from_array(&env, &[1u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Vote: 600 approve, 400 reject → 60% approve → quorum met (>= 50%)
    client.vote_listing(&voter1, &proposal_id, &true, &600i128);
    client.vote_listing(&voter2, &proposal_id, &false, &400i128);

    // Close voting window
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);

    client.finalise_listing_proposal(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::PendingEnactment);

    // Advance past enactment delay
    env.ledger().set_sequence_number(env.ledger().sequence() + 51);

    client.enact_listing(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Enacted);

    // Token now in whitelist
    assert!(client.is_token_allowed(&new_token));
}

#[test]
fn test_quorum_failure_marks_proposal_failed() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 400);

    let rationale = BytesN::from_array(&env, &[2u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Only 40% approve → below 50% quorum
    client.vote_listing(&voter, &proposal_id, &true, &400i128);
    // No reject votes, total = 400, approve = 400 but we need another voter to reject
    // Actually 400/400 = 100% approve. Let's add a reject voter to bring below threshold
    let rejecter = Address::generate(&env);
    mint_gov_tokens(&env, &gov_token, &admin, &rejecter, 700);
    client.vote_listing(&rejecter, &proposal_id, &false, &700i128);
    // Now approve=400, total=1100, approve% = 36% < 50%

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Failed);
}

#[test]
fn test_admin_veto_blocks_enactment() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[3u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    client.vote_listing(&voter, &proposal_id, &true, &600i128);
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    // Admin vetoes before enactment
    let reason = BytesN::from_array(&env, &[0xFFu8; 32]);
    client.veto_listing_proposal(&admin, &proposal_id, &reason);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.status, ProposalStatus::Vetoed);

    // Token NOT in whitelist
    assert!(!client.is_token_allowed(&new_token));
}

#[test]
#[should_panic(expected = "InsufficientProposerStake")]
fn test_insufficient_proposer_stake_rejected() {
    let (env, _admin, client, gov_token) = setup_governance();

    let poor_proposer = Address::generate(&env);
    // Only mint 50, but minimum is 100
    mint_gov_tokens(&env, &gov_token, &_admin, &poor_proposer, 50);

    let new_token = Address::generate(&env);
    let rationale = BytesN::from_array(&env, &[4u8; 32]);
    client.propose_token_listing(&poor_proposer, &new_token, &rationale);
}

#[test]
fn test_duplicate_vote_overwrite_last_write_wins() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 800);

    let rationale = BytesN::from_array(&env, &[5u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // First vote: approve 800
    client.vote_listing(&voter, &proposal_id, &true, &800i128);

    let p = client.get_listing_proposal(&proposal_id);
    assert_eq!(p.approve_weight, 800);

    // Overwrite: now reject 800
    client.vote_listing(&voter, &proposal_id, &false, &800i128);

    let p = client.get_listing_proposal(&proposal_id);
    assert_eq!(p.approve_weight, 0);
    assert_eq!(p.reject_weight, 800);
}

#[test]
#[should_panic(expected = "VotingWindowClosed")]
fn test_cannot_vote_after_window_closes() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 500);

    let rationale = BytesN::from_array(&env, &[6u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Close voting window
    env.ledger().set_sequence_number(env.ledger().sequence() + 101);

    client.vote_listing(&voter, &proposal_id, &true, &500i128);
}

#[test]
#[should_panic(expected = "VoteWeightExceedsBalance")]
fn test_vote_weight_exceeds_balance_rejected() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 300);

    let rationale = BytesN::from_array(&env, &[7u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Try to vote with weight > balance
    client.vote_listing(&voter, &proposal_id, &true, &1000i128);
}

#[test]
#[should_panic(expected = "EnactmentDelayNotElapsed")]
fn test_cannot_enact_before_delay() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[8u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);
    client.vote_listing(&voter, &proposal_id, &true, &600i128);

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    // Enactment delay not elapsed yet
    client.enact_listing(&proposal_id);
}

// ─── #591: balance snapshot at first vote ────────────────────────────────────

#[test]
fn test_balance_change_after_vote_does_not_affect_recorded_weight() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 600);

    let rationale = BytesN::from_array(&env, &[9u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Voter casts a vote using their full balance at the time.
    client.vote_listing(&voter, &proposal_id, &true, &600i128);

    // Voter's balance changes after voting (e.g. they transfer tokens away).
    // A flash-loan style balance manipulation must not affect the already
    // recorded weight, since weight is fixed at first-vote time.
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 5_000);

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    // Recorded weight remains 600, not affected by the post-vote balance increase.
    assert_eq!(proposal.approve_weight, 600);
    assert_eq!(proposal.status, ProposalStatus::PendingEnactment);
}

#[test]
#[should_panic(expected = "VoteWeightExceedsBalance")]
fn test_vote_weight_capped_by_snapshot_not_live_balance_on_revote() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 300);

    let rationale = BytesN::from_array(&env, &[10u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // First vote establishes the snapshot at balance 300.
    client.vote_listing(&voter, &proposal_id, &true, &300i128);

    // Balance later increases, but a re-vote weight above the snapshot must
    // still be rejected — the snapshot, not the live balance, is the cap.
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 1_000);

    client.vote_listing(&voter, &proposal_id, &true, &1_000i128);
}

#[test]
fn test_vote_weight_uses_snapshot_not_live_balance() {
    let (env, admin, client, gov_token) = setup_governance();

    let proposer = Address::generate(&env);
    let voter = Address::generate(&env);
    let sink = Address::generate(&env);
    let new_token = Address::generate(&env);

    mint_gov_tokens(&env, &gov_token, &admin, &proposer, 200);
    // Voter flash-acquires a large balance right before voting.
    mint_gov_tokens(&env, &gov_token, &admin, &voter, 1_000);

    let rationale = BytesN::from_array(&env, &[11u8; 32]);
    let proposal_id = client.propose_token_listing(&proposer, &new_token, &rationale);

    // Vote weight is snapshotted at first vote.
    client.vote_listing(&voter, &proposal_id, &true, &1_000i128);

    // Voter moves the tokens elsewhere before finalisation — the recorded
    // weight must not be affected by this post-vote balance change.
    soroban_sdk::token::Client::new(&env, &gov_token).transfer(&voter, &sink, &1_000i128);
    assert_eq!(soroban_sdk::token::Client::new(&env, &gov_token).balance(&voter), 0);

    env.ledger().set_sequence_number(env.ledger().sequence() + 101);
    client.finalise_listing_proposal(&proposal_id);

    let proposal = client.get_listing_proposal(&proposal_id);
    assert_eq!(proposal.approve_weight, 1_000);
    assert_eq!(proposal.status, ProposalStatus::PendingEnactment);
}
