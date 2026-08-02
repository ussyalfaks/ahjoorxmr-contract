#![cfg(test)]

//! #376: Tests for bounty board milestone gating with a verifier sign-off chain.
//!
//! Covers the acceptance criteria:
//! - Milestones must be completed in order (index N+1 cannot be submitted
//!   before N is verified).
//! - Each milestone has its own designated verifier address.
//! - Unverified milestones block subsequent milestone submissions.
//! - The bounty creator can replace a verifier before the milestone is submitted.
//! - Sequential verification, out-of-order rejection, and verifier replacement.

use crate::{
    AhjoorEscrowContract, AhjoorEscrowContractClient, BountyMilestoneInput,
    BountyMilestoneStatus, EscrowErrorExt3, EscrowStatus,
};
use soroban_sdk::{testutils::Address as _, token, vec, Address, BytesN, Env, Vec};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> token::StellarAssetClient<'a> {
    token::StellarAssetClient::new(
        e,
        &e.register_stellar_asset_contract_v2(admin.clone()).address(),
    )
}

/// Convenience: a deterministic 32-byte hash from a single byte.
fn h(e: &Env, b: u8) -> BytesN<32> {
    BytesN::from_array(e, &[b; 32])
}

struct Harness<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    token: token::Client<'a>,
    token_addr: Address,
    contract_id: Address,
    buyer: Address,
    solver: Address,
    verifier0: Address,
    verifier1: Address,
}

fn setup<'a>() -> Harness<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let solver = Address::generate(&env);
    let verifier0 = Address::generate(&env);
    let verifier1 = Address::generate(&env);
    let token_admin = Address::generate(&env);

    let token_sac = create_token_contract(&env, &token_admin);
    token_sac.mint(&buyer, &1000);
    let token_addr = token_sac.address.clone();
    let token = token::Client::new(&env, &token_addr);

    let contract_id = env.register_contract(None, AhjoorEscrowContract);
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);
    client.initialize(&admin);

    Harness {
        env,
        client,
        token,
        token_addr,
        contract_id,
        buyer,
        solver,
        verifier0,
        verifier1,
    }
}

/// Build a 2-milestone bounty (300 + 200) and return its escrow id.
fn create_two_milestone_bounty(hx: &Harness) -> u32 {
    let now = hx.env.ledger().timestamp();
    let milestones: Vec<BountyMilestoneInput> = vec![
        &hx.env,
        BountyMilestoneInput {
            description_hash: h(&hx.env, 1),
            verifier: hx.verifier0.clone(),
            amount: 300,
        },
        BountyMilestoneInput {
            description_hash: h(&hx.env, 2),
            verifier: hx.verifier1.clone(),
            amount: 200,
        },
    ];
    hx.client.create_milestone_bounty(
        &hx.buyer,
        &hx.token_addr,
        &milestones,
        &(now + 86_400),
        &(now + 172_800),
    )
}

#[test]
fn test_create_escrows_full_total_and_stores_milestones() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);

    // Full sum of milestone amounts is escrowed up front.
    assert_eq!(hx.token.balance(&hx.buyer), 1000 - 500);
    assert_eq!(hx.token.balance(&hx.contract_id), 500);

    let escrow = hx.client.get_escrow(&id);
    assert_eq!(escrow.amount, 500);
    assert_eq!(escrow.status, EscrowStatus::BountyUnclaimed);

    let milestones = hx.client.get_bounty_milestones(&id);
    assert_eq!(milestones.len(), 2);
    // Each milestone has its own designated verifier.
    assert_eq!(milestones.get(0).unwrap().verifier, hx.verifier0);
    assert_eq!(milestones.get(1).unwrap().verifier, hx.verifier1);
    assert_eq!(milestones.get(0).unwrap().status, BountyMilestoneStatus::Pending);
    assert_eq!(milestones.get(1).unwrap().status, BountyMilestoneStatus::Pending);
}

#[test]
fn test_sequential_verification_releases_tranches_then_settles() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);

    hx.client.claim_bounty(&hx.solver, &id);
    assert_eq!(hx.client.get_escrow(&id).status, EscrowStatus::BountyClaimed);

    // ── Milestone 0 ──
    hx.client.submit_bounty_milestone(&hx.solver, &id, &0, &h(&hx.env, 11));
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().status,
        BountyMilestoneStatus::Submitted
    );

    hx.client.verify_bounty_milestone(&id, &0);
    assert_eq!(hx.token.balance(&hx.solver), 300);
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().status,
        BountyMilestoneStatus::Paid
    );
    // Not all milestones paid yet → bounty still claimed.
    assert_eq!(hx.client.get_escrow(&id).status, EscrowStatus::BountyClaimed);

    // ── Milestone 1 ──
    hx.client.submit_bounty_milestone(&hx.solver, &id, &1, &h(&hx.env, 12));
    hx.client.verify_bounty_milestone(&id, &1);
    assert_eq!(hx.token.balance(&hx.solver), 500);

    // All tranches released → escrow settled and empty.
    assert_eq!(hx.client.get_escrow(&id).status, EscrowStatus::Released);
    assert_eq!(hx.token.balance(&hx.contract_id), 0);
}

#[test]
fn test_out_of_order_submission_is_rejected() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);
    hx.client.claim_bounty(&hx.solver, &id);

    // Attempt to submit milestone 1 before milestone 0 is verified.
    let __typed_err_result = hx.client.try_submit_bounty_milestone(&hx.solver, &id, &1, &h(&hx.env, 12));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::PreviousMilestoneNotYetVerified.into());
}

#[test]
fn test_verify_before_submit_is_rejected() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);
    hx.client.claim_bounty(&hx.solver, &id);

    // Milestone 0 has not been submitted yet.
    let __typed_err_result = hx.client.try_verify_bounty_milestone(&id, &0);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::MilestoneIsNotAwaitingVerification.into());
}

#[test]
fn test_replace_verifier_before_submission_then_flow_works() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);

    let new_verifier = Address::generate(&hx.env);
    hx.client
        .replace_bounty_verifier(&hx.buyer, &id, &0, &new_verifier);

    // The stored verifier for milestone 0 is updated.
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().verifier,
        new_verifier
    );

    // Downstream flow continues to work with the replaced verifier.
    hx.client.claim_bounty(&hx.solver, &id);
    hx.client.submit_bounty_milestone(&hx.solver, &id, &0, &h(&hx.env, 11));
    hx.client.verify_bounty_milestone(&id, &0);
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().status,
        BountyMilestoneStatus::Paid
    );
    assert_eq!(hx.token.balance(&hx.solver), 300);
}

#[test]
fn test_replace_verifier_after_submission_is_rejected() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);
    hx.client.claim_bounty(&hx.solver, &id);
    hx.client.submit_bounty_milestone(&hx.solver, &id, &0, &h(&hx.env, 11));

    let new_verifier = Address::generate(&hx.env);
    // Too late — milestone 0 is already Submitted.
    let __typed_err_result = hx.client
        .try_replace_bounty_verifier(&hx.buyer, &id, &0, &new_verifier);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::VerifierCanOnlyBeReplacedBeforeMilestoneIsSubmitted.into());
}

#[test]
fn test_non_buyer_cannot_replace_verifier() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);

    let new_verifier = Address::generate(&hx.env);
    let unauthorized = Address::generate(&hx.env);
    
    // Unauthorized address tries to replace verifier
    let __typed_err_result = hx.client
        .try_replace_bounty_verifier(&unauthorized, &id, &0, &new_verifier);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::OnlyBountyCreatorCanReplaceVerifier.into());
}

#[test]
fn test_non_solver_cannot_submit_milestone() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);
    hx.client.claim_bounty(&hx.solver, &id);

    let unauthorized = Address::generate(&hx.env);
    
    // Unauthorized address tries to submit milestone
    let __typed_err_result = hx.client.try_submit_bounty_milestone(&unauthorized, &id, &0, &h(&hx.env, 11));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::OnlySolverCanSubmitMilestones.into());
}

#[test]
fn test_submit_milestone_before_claim_is_rejected() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);

    // Try to submit milestone without claiming the bounty first
    let __typed_err_result = hx.client.try_submit_bounty_milestone(&hx.solver, &id, &0, &h(&hx.env, 11));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::BountyMustBeClaimedBeforeSubmittingMilestones.into());
}

#[test]
fn test_multiple_verifier_replacements_before_submission() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);

    // Replace verifier multiple times before submission
    let verifier_v2 = Address::generate(&hx.env);
    let verifier_v3 = Address::generate(&hx.env);
    let verifier_v4 = Address::generate(&hx.env);

    hx.client
        .replace_bounty_verifier(&hx.buyer, &id, &0, &verifier_v2);
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().verifier,
        verifier_v2
    );

    hx.client
        .replace_bounty_verifier(&hx.buyer, &id, &0, &verifier_v3);
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().verifier,
        verifier_v3
    );

    hx.client
        .replace_bounty_verifier(&hx.buyer, &id, &0, &verifier_v4);
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().verifier,
        verifier_v4
    );

    // Verify the final verifier is used for verification
    hx.client.claim_bounty(&hx.solver, &id);
    hx.client.submit_bounty_milestone(&hx.solver, &id, &0, &h(&hx.env, 11));
    hx.client.verify_bounty_milestone(&id, &0);
    
    assert_eq!(hx.token.balance(&hx.solver), 300);
    assert_eq!(
        hx.client.get_bounty_milestones(&id).get(0).unwrap().status,
        BountyMilestoneStatus::Paid
    );
}

#[test]
fn test_all_milestones_can_be_verified_independently_by_different_verifiers() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);

    hx.client.claim_bounty(&hx.solver, &id);

    // Submit and verify milestone 0 (verifier0)
    hx.client.submit_bounty_milestone(&hx.solver, &id, &0, &h(&hx.env, 11));
    hx.client.verify_bounty_milestone(&id, &0);
    
    let milestones_after_first = hx.client.get_bounty_milestones(&id);
    assert_eq!(
        milestones_after_first.get(0).unwrap().status,
        BountyMilestoneStatus::Paid
    );
    assert_eq!(
        milestones_after_first.get(0).unwrap().verifier,
        hx.verifier0
    );

    // Submit and verify milestone 1 (verifier1)
    hx.client.submit_bounty_milestone(&hx.solver, &id, &1, &h(&hx.env, 12));
    hx.client.verify_bounty_milestone(&id, &1);
    
    let milestones_after_second = hx.client.get_bounty_milestones(&id);
    assert_eq!(
        milestones_after_second.get(1).unwrap().status,
        BountyMilestoneStatus::Paid
    );
    assert_eq!(
        milestones_after_second.get(1).unwrap().verifier,
        hx.verifier1
    );

    // Both milestones paid by different verifiers
    assert_eq!(hx.token.balance(&hx.solver), 500);
    assert_eq!(hx.client.get_escrow(&id).status, EscrowStatus::Released);
}

/// #419: Out-of-order submission must be rejected before any prior milestone is verified.
#[test]
fn test_out_of_order_submission_rejected() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);
    hx.client.claim_bounty(&hx.solver, &id);

    // Milestone 0 is Pending (not yet verified). Submitting milestone 1 must fail.
    let __typed_err_result = hx.client.try_submit_bounty_milestone(&hx.solver, &id, &1, &h(&hx.env, 12));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::PreviousMilestoneNotYetVerified.into());
}

#[test]
fn test_submit_invalid_milestone_index() {
    let hx = setup();
    let id = create_two_milestone_bounty(&hx);
    hx.client.claim_bounty(&hx.solver, &id);

    // Try to submit milestone with index 2 (only 0 and 1 exist)
    let __typed_err_result = hx.client.try_submit_bounty_milestone(&hx.solver, &id, &2, &h(&hx.env, 13));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt3::MilestoneIndexOutBounds.into());
}
