#![cfg(test)]

//! #319: Tests for the bounty board open competitive work assignment feature.
//!
//! Covers the acceptance criteria:
//! - Buyer can create a bounty with description hash, claim deadline, and submission
//!   deadline; status is `BountyUnclaimed` and solver is `None`.
//! - Any solver can claim an unclaimed bounty first-come-first-served.
//! - The assigned solver can submit work (submission hash) before the deadline.
//! - Buyer can approve a submission, releasing funds to the solver.
//! - Buyer can reject a submission, re-opening the bounty for another solver.
//! - A configurable maximum rejection rounds (default 3) prevents infinite churn.
//! - Buyer can cancel an unclaimed bounty and receive a refund.
//! - Admin can configure the max bounty rejection rounds.
//! - Milestone-gated bounty variant with per-milestone verifiers.
//! - Cancellation after partial milestone payout refunds only remaining funds.

use crate::{
    AhjoorEscrowContract, AhjoorEscrowContractClient, BountyMilestoneInput, EscrowErrorExt3,
    EscrowStatus,
};
use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, vec, Address, BytesN, Env, String, Vec,
};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> token::StellarAssetClient<'a> {
    token::StellarAssetClient::new(e, &e.register_stellar_asset_contract_v2(admin.clone()).address())
}

fn advance_ledger(e: &Env, delta_secs: u64) {
    e.ledger().set(LedgerInfo {
        timestamp: e.ledger().timestamp().saturating_add(delta_secs),
        protocol_version: 23,
        sequence_number: e.ledger().sequence(),
        network_id: Default::default(),
        base_reserve: 10,
        min_temp_entry_ttl: 10,
        min_persistent_entry_ttl: 10,
        max_entry_ttl: 3110400,
    });
}

#[test]
fn test_list_bounty_submissions_is_paginated() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();
    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);
    client.submit_bounty_work(&solver, &escrow_id, &BytesN::from_array(&env, &[2u8; 32]));
    client.submit_bounty_work(&solver, &escrow_id, &BytesN::from_array(&env, &[3u8; 32]));

    let page1 = client.list_bounty_submissions(&escrow_id, &0, &1);
    assert_eq!(page1.len(), 1);
    assert_eq!(page1.get(0).unwrap().solver, solver);

    let page2 = client.list_bounty_submissions(&escrow_id, &1, &1);
    assert_eq!(page2.len(), 1);
    assert_eq!(page2.get(0).unwrap().submission_hash, BytesN::from_array(&env, &[3u8; 32]));
}

#[test]
fn test_create_bounty_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();
    let claim_deadline = current_time + 86400; // 1 day
    let submission_deadline = current_time + 172800; // 2 days

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &claim_deadline,
        &submission_deadline,
    );

    assert_eq!(escrow_id, 1);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.buyer, buyer);
    assert_eq!(escrow.amount, 500);
    assert_eq!(escrow.status, EscrowStatus::BountyUnclaimed);

    let bounty_data = client.get_bounty_data(&escrow_id).unwrap();
    assert_eq!(bounty_data.description_hash, description_hash);
    assert_eq!(bounty_data.claim_deadline_ledger, claim_deadline);
    assert_eq!(bounty_data.submission_deadline_ledger, submission_deadline);
    assert_eq!(bounty_data.solver, None);
    assert_eq!(bounty_data.rejection_count, 0);
}

#[test]
fn test_create_bounty_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();
    let __typed_err_result = client.try_create_bounty(
        &buyer,
        &token.address,
        &0,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::BountyAmountMustBePositive.into());
}

#[test]
fn test_create_bounty_past_claim_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    advance_ledger(&env, 1000); // ensure timestamp > 100 so subtraction doesn't overflow
    let current_time = env.ledger().timestamp();
    let __typed_err_result = client.try_create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time - 100),
        &(current_time + 172800),
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::ClaimDeadlineMustBeFuture.into());
}

#[test]
fn test_create_bounty_invalid_deadlines() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();
    let __typed_err_result = client.try_create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 172800),
        &(current_time + 86400),
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::SubmissionDeadlineMustBeAfterClaimDeadline.into());
}

#[test]
fn test_claim_bounty_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.seller, solver);
    assert_eq!(escrow.status, EscrowStatus::BountyClaimed);

    let bounty_data = client.get_bounty_data(&escrow_id).unwrap();
    assert_eq!(bounty_data.solver, Some(solver));
}

#[test]
fn test_claim_bounty_duplicate() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver1 = Address::generate(&env);
    let solver2 = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver1, &escrow_id);
    let __typed_err_result = client.try_claim_bounty(&solver2, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::BountyIsNotAvailableClaiming.into());
}

#[test]
fn test_claim_bounty_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    advance_ledger(&env, 86401); // Past claim deadline

    let __typed_err_result = client.try_claim_bounty(&solver, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::ClaimDeadlineHasPassed.into());
}

#[test]
fn test_submit_bounty_work_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);

    let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.submit_bounty_work(&solver, &escrow_id, &submission_hash);

    let bounty_data = client.get_bounty_data(&escrow_id).unwrap();
    assert_eq!(bounty_data.submission_hash, Some(submission_hash));
}

#[test]
fn test_submit_bounty_work_wrong_solver() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let wrong_solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);

    let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
    let __typed_err_result = client.try_submit_bounty_work(&wrong_solver, &escrow_id, &submission_hash);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::OnlyAssignedSolverCanSubmitWork.into());
}

#[test]
fn test_submit_bounty_work_after_deadline() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);

    advance_ledger(&env, 172801); // Past submission deadline

    let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
    let __typed_err_result = client.try_submit_bounty_work(&solver, &escrow_id, &submission_hash);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::SubmissionDeadlineHasPassed.into());
}

#[test]
fn test_approve_bounty_submission_success() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);

    let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.submit_bounty_work(&solver, &escrow_id, &submission_hash);

    let solver_balance_before = token.balance(&solver);
    client.approve_bounty_submission(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);

    let solver_balance_after = token.balance(&solver);
    assert_eq!(solver_balance_after - solver_balance_before, 500);
}

#[test]
fn test_approve_bounty_without_submission() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);
    let __typed_err_result = client.try_approve_bounty_submission(&buyer, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::NoSubmissionHasBeenMade.into());
}

#[test]
fn test_reject_bounty_submission_and_reclaim() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver1 = Address::generate(&env);
    let solver2 = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    // First solver claims and submits
    client.claim_bounty(&solver1, &escrow_id);
    let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.submit_bounty_work(&solver1, &escrow_id, &submission_hash);

    // Buyer rejects
    client.reject_bounty_submission(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::BountyUnclaimed);

    let bounty_data = client.get_bounty_data(&escrow_id).unwrap();
    assert_eq!(bounty_data.solver, None);
    assert_eq!(bounty_data.submission_hash, None);
    assert_eq!(bounty_data.rejection_count, 1);

    // Second solver can now claim
    client.claim_bounty(&solver2, &escrow_id);
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.seller, solver2);
    assert_eq!(escrow.status, EscrowStatus::BountyClaimed);
}

#[test]
fn test_reject_bounty_max_rejections() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    // Reject 3 times (default max)
    for _ in 0..3 {
        client.claim_bounty(&solver, &escrow_id);
        let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.submit_bounty_work(&solver, &escrow_id, &submission_hash);
        client.reject_bounty_submission(&buyer, &escrow_id);
    }

    // Fourth rejection should panic
    client.claim_bounty(&solver, &escrow_id);
    let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.submit_bounty_work(&solver, &escrow_id, &submission_hash);
    let __typed_err_result = client.try_reject_bounty_submission(&buyer, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::MaximumRejectionRoundsReached.into());
}

#[test]
fn test_cancel_bounty_unclaimed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    let buyer_balance_before = token.balance(&buyer);
    client.cancel_bounty(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);

    let buyer_balance_after = token.balance(&buyer);
    assert_eq!(buyer_balance_after - buyer_balance_before, 500);
}

#[test]
fn test_cancel_bounty_after_inspection_fee() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let inspector = Address::generate(&env);
    let verifier = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_sac = create_token_contract(&env, &token_admin);
    token_sac.mint(&buyer, &1000);
    let token_client = token::Client::new(&env, &token_sac.address);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let current_time = env.ledger().timestamp();
    let claim_deadline = current_time + 86400;
    let submission_deadline = current_time + 172800;
    let inspection_fee = 50;
    let bounty_remainder = 450;
    let milestones: Vec<BountyMilestoneInput> = vec![
        &env,
        BountyMilestoneInput {
            description_hash: BytesN::from_array(&env, &[1u8; 32]),
            verifier: inspector,
            amount: inspection_fee,
        },
        BountyMilestoneInput {
            description_hash: BytesN::from_array(&env, &[2u8; 32]),
            verifier,
            amount: bounty_remainder,
        },
    ];

    let escrow_id = client.create_milestone_bounty(
        &buyer,
        &token_sac.address,
        &milestones,
        &claim_deadline,
        &submission_deadline,
    );

    client.claim_bounty(&solver, &escrow_id);
    client.submit_bounty_milestone(
        &solver,
        &escrow_id,
        &0,
        &BytesN::from_array(&env, &[3u8; 32]),
    );
    client.verify_bounty_milestone(&escrow_id, &0);

    assert_eq!(token_client.balance(&contract_id), bounty_remainder);
    assert_eq!(token_client.balance(&solver), inspection_fee);

    advance_ledger(&env, 86401);

    let buyer_balance_before = token_client.balance(&buyer);
    client.cancel_bounty(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);

    let buyer_balance_after = token_client.balance(&buyer);
    assert_eq!(buyer_balance_after - buyer_balance_before, bounty_remainder);
    assert_eq!(token_client.balance(&contract_id), 0);
}

#[test]
fn test_cancel_bounty_claimed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    client.claim_bounty(&solver, &escrow_id);
    let __typed_err_result = client.try_cancel_bounty(&buyer, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::CannotCancelBountyCurrentState.into());
}

#[test]
fn test_full_bounty_award_flow() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    // 1. Create bounty
    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::BountyUnclaimed);

    // 2. Solver claims bounty
    client.claim_bounty(&solver, &escrow_id);
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::BountyClaimed);
    assert_eq!(escrow.seller, solver);

    // 3. Solver submits work
    let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
    client.submit_bounty_work(&solver, &escrow_id, &submission_hash);

    let bounty_data = client.get_bounty_data(&escrow_id).unwrap();
    assert_eq!(bounty_data.submission_hash, Some(submission_hash));

    // 4. Buyer approves and funds are released
    let solver_balance_before = token.balance(&solver);
    client.approve_bounty_submission(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);

    let solver_balance_after = token.balance(&solver);
    assert_eq!(solver_balance_after - solver_balance_before, 500);
}

#[test]
fn test_set_max_bounty_rejection_rounds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);

    // Set max rejection rounds to 5
    client.set_max_bounty_rejection_rounds(&admin, &5);

    // Verify by creating a bounty and rejecting it 5 times
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token = create_token_contract(&env, &token_admin);
    token.mint(&buyer, &1000);

    let description_hash = BytesN::from_array(&env, &[1u8; 32]);
    let current_time = env.ledger().timestamp();

    let escrow_id = client.create_bounty(
        &buyer,
        &token.address,
        &500,
        &description_hash,
        &(current_time + 86400),
        &(current_time + 172800),
    );

    // Should be able to reject 5 times
    for i in 0..5 {
        client.claim_bounty(&solver, &escrow_id);
        let submission_hash = BytesN::from_array(&env, &[2u8; 32]);
        client.submit_bounty_work(&solver, &escrow_id, &submission_hash);
        client.reject_bounty_submission(&buyer, &escrow_id);

        let bounty_data = client.get_bounty_data(&escrow_id).unwrap();
        assert_eq!(bounty_data.rejection_count, i + 1);
    }
}
