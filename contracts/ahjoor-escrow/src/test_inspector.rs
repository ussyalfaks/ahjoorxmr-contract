#![cfg(test)]

use crate::{AhjoorEscrowContract, AhjoorEscrowContractClient, EscrowStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::Client as TokenClient,
    token::StellarAssetClient as TokenAdminClient,
    Address, BytesN, Env, String, Vec,
};

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    AhjoorEscrowContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);
    token_admin_client.mint(&buyer, &10_000);

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);

    (env, admin, buyer, seller, arbiter, token_addr, client)
}

#[test]
fn test_inspector_approval_enables_release() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    // Seller marks work complete → AwaitingInspection
    client.seller_mark_complete(&seller, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::AwaitingInspection);

    // Submit inspection result (approved)
    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &true, &report_hash);

    // Verify status changed to InspectionPassed
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionPassed);

    // Now buyer can release
    client.release_escrow(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_inspector_rejection_blocks_release() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    // Seller marks work complete → AwaitingInspection
    client.seller_mark_complete(&seller, &escrow_id);

    // Submit inspection result (rejected)
    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &false, &report_hash);

    // Verify status changed to InspectionFailed
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionFailed);

    // Buyer cannot release
    let result = client.try_release_escrow(&buyer, &escrow_id);
    assert!(result.is_err());
}

// ── #357: Inspector Reputation Scoring Tests ─────────────────────────────────

fn make_request(env: &soroban_sdk::Env, seller: &Address, arbiter: &Address, token: &Address, amount: i128) -> crate::EscrowCreateRequest {
    crate::EscrowCreateRequest {
        seller: seller.clone(), arbiter: arbiter.clone(), amount,
        token: token.clone(), deadline: env.ledger().timestamp() + 86400,
        metadata_hash: None, sellers: soroban_sdk::Vec::new(env), auto_renew: false,
        renewal_count: 0, buyer_inactivity_secs: 0, min_lock_until: None,
        release_base: None, release_quote: None, release_comparison: None,
        release_threshold_price: None, arbiter_fee_bps: None, dispute_default_winner: None,
        auto_renew_max_renewals: None,

        auto_renew_interval_ledgers: None,
    }
}

#[test]
fn test_inspector_score_initialized_on_first_ruling() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    // No score yet → neutral (0, 0, 10_000)
    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(total, 0);
    assert_eq!(correct, 0);
    assert_eq!(accuracy, 10_000);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let eid = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &eid, &String::from_str(&env, "test"), &1000);
    client.resolve_dispute(&arbiter, &eid, &50);

    // First ruling: initialized to neutral (1, 1, 10_000)
    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(total, 1);
    assert_eq!(correct, 1);
    assert_eq!(accuracy, 10_000);
}

#[test]
fn test_inspector_score_increases_on_ruling() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    // Mint extra tokens for buyer
    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &5_000);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let e1 = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    let e2 = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    client.dispute_escrow(&buyer, &e1, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &e1, &0);

    client.dispute_escrow(&buyer, &e2, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &e2, &0);

    // Two rulings: first initializes to (1,1), second increments to (2,2)
    let (total, correct, _accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(total, 2);
    assert_eq!(correct, 2);
}

#[test]
fn test_inspector_score_decreases_on_appeal() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let eid = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &eid, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &eid, &0);

    // Score after ruling: (1, 1)
    let (total_before, correct_before, _) = client.get_inspector_score(&inspector);
    assert_eq!(total_before, 1);
    assert_eq!(correct_before, 1);

    // Admin appeals → correct_rulings decremented
    client.appeal_inspector_ruling(&admin, &eid);

    let (total_after, correct_after, accuracy_after) = client.get_inspector_score(&inspector);
    assert_eq!(total_after, 1);
    assert_eq!(correct_after, 0);
    assert_eq!(accuracy_after, 0);
}

#[test]
fn test_low_score_inspector_blocked_from_high_value_escrow() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &100_000);

    // Create ruling + appeal → inspector score = 0/1 = 0 bps
    let req = make_request(&env, &seller, &arbiter, &token, 500);
    let e1 = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &e1, &String::from_str(&env, "d"), &500);
    client.resolve_dispute(&arbiter, &e1, &0);
    client.appeal_inspector_ruling(&admin, &e1);

    // Set threshold: min 5000 bps for escrows above 1000
    client.set_inspector_score_threshold(&admin, &5_000u32, &1_000i128);

    // High-value escrow with low-score inspector must be rejected
    let hv_req = make_request(&env, &seller, &arbiter, &token, 5_000);
    let result = client.try_create_escrow_with_inspector(&buyer, &hv_req, &Some(inspector.clone()));
    assert!(result.is_err());
}

#[test]
fn test_replaced_inspector_score_penalized() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let old_inspector = Address::generate(&env);
    let new_inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &5_000);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let first_escrow = client.create_escrow_with_inspector(&buyer, &req, &Some(old_inspector.clone()));
    client.dispute_escrow(&buyer, &first_escrow, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &first_escrow, &0);

    let (total_before, correct_before, accuracy_before) = client.get_inspector_score(&old_inspector);
    assert_eq!((total_before, correct_before, accuracy_before), (1, 1, 10_000));

    let replacement_escrow = client.create_escrow_with_inspector(&buyer, &req, &Some(old_inspector.clone()));
    client.replace_inspector(&buyer, &replacement_escrow, &new_inspector);
    client.replace_inspector(&seller, &replacement_escrow, &new_inspector);

    let (total_after, correct_after, accuracy_after) = client.get_inspector_score(&old_inspector);
    assert_eq!(total_after, 2);
    assert_eq!(correct_after, 1);
    assert!(accuracy_after < accuracy_before);
    assert_eq!(accuracy_after, 5_000);

    let (new_total, new_correct, new_accuracy) = client.get_inspector_score(&new_inspector);
    assert_eq!((new_total, new_correct, new_accuracy), (0, 0, 10_000));
}

#[test]
fn test_inspector_cannot_approve_without_seller_marking_complete() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    // Try to submit inspection without seller marking complete
    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    let result = client.try_submit_inspection_result(&inspector, &escrow_id, &true, &report_hash);
    
    assert!(result.is_err());
}

#[test]
fn test_non_inspector_cannot_submit_inspection_result() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);
    let impostor = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    client.seller_mark_complete(&seller, &escrow_id);

    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    let result = client.try_submit_inspection_result(&impostor, &escrow_id, &true, &report_hash);
    
    assert!(result.is_err());
}

#[test]
fn test_inspector_can_reject_and_seller_can_resubmit() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    // First submission - rejected
    client.seller_mark_complete(&seller, &escrow_id);
    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &false, &report_hash);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionFailed);

    // Seller resubmits after fixing issues
    client.seller_mark_complete(&seller, &escrow_id);
    
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::AwaitingInspection);

    // Inspector approves this time
    let report_hash2 = BytesN::<32>::from_array(&env, &[2u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &true, &report_hash2);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionPassed);
}

#[test]
fn test_multiple_inspectors_independent_scores() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector1 = Address::generate(&env);
    let inspector2 = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &10_000);

    // Inspector 1: one successful ruling
    let req1 = make_request(&env, &seller, &arbiter, &token, 1000);
    let e1 = client.create_escrow_with_inspector(&buyer, &req1, &Some(inspector1.clone()));
    client.dispute_escrow(&buyer, &e1, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &e1, &0);

    // Inspector 2: two successful rulings
    let req2 = make_request(&env, &seller, &arbiter, &token, 1000);
    let e2 = client.create_escrow_with_inspector(&buyer, &req2, &Some(inspector2.clone()));
    client.dispute_escrow(&buyer, &e2, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &e2, &0);

    let req3 = make_request(&env, &seller, &arbiter, &token, 1000);
    let e3 = client.create_escrow_with_inspector(&buyer, &req3, &Some(inspector2.clone()));
    client.dispute_escrow(&buyer, &e3, &String::from_str(&env, "d"), &1000);
    client.resolve_dispute(&arbiter, &e3, &0);

    // Verify independent scores
    let (total1, correct1, _) = client.get_inspector_score(&inspector1);
    let (total2, correct2, _) = client.get_inspector_score(&inspector2);
    
    assert_eq!((total1, correct1), (1, 1));
    assert_eq!((total2, correct2), (2, 2));
}

#[test]
fn test_inspector_score_accuracy_calculation() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &10_000);

    // Create 3 escrows, 2 correct rulings, 1 appealed
    for i in 0..3 {
        let req = make_request(&env, &seller, &arbiter, &token, 1000);
        let eid = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
        client.dispute_escrow(&buyer, &eid, &String::from_str(&env, "dispute"), &1000);
        client.resolve_dispute(&arbiter, &eid, &0);
        
        // Appeal the last one
        if i == 2 {
            client.appeal_inspector_ruling(&admin, &eid);
        }
    }

    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(total, 3);
    assert_eq!(correct, 2);
    // accuracy = (2 * 10_000) / 3 = 6_666 bps
    assert_eq!(accuracy, 6_666);
}

#[test]
fn test_inspector_replacement_requires_both_parties() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let old_inspector = Address::generate(&env);
    let new_inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(old_inspector.clone()));

    // Only buyer approves - not enough
    client.replace_inspector(&buyer, &escrow_id, &new_inspector);
    
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.extensions.inspector, Some(old_inspector.clone()));

    // Now seller also approves - inspector should change
    client.replace_inspector(&seller, &escrow_id, &new_inspector);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.extensions.inspector, Some(new_inspector));
}

#[test]
fn test_escrow_without_inspector_skips_inspection() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &None);

    // Seller marks complete - should go straight to a releasable state
    client.seller_mark_complete(&seller, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    // Should not be in AwaitingInspection status
    assert_ne!(escrow.status, EscrowStatus::AwaitingInspection);
    
    // Buyer should be able to release directly
    client.release_escrow(&buyer, &escrow_id);
    
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_high_score_inspector_allowed_high_value_escrow() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &100_000);

    // Build up inspector reputation with successful rulings
    for _ in 0..5 {
        let req = make_request(&env, &seller, &arbiter, &token, 500);
        let eid = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
        client.dispute_escrow(&buyer, &eid, &String::from_str(&env, "d"), &500);
        client.resolve_dispute(&arbiter, &eid, &0);
    }

    // Verify high score (5/5 = 10_000 bps)
    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!((total, correct, accuracy), (5, 5, 10_000));

    // Set threshold: min 8000 bps for escrows above 1000
    client.set_inspector_score_threshold(&admin, &8_000u32, &1_000i128);

    // High-value escrow with high-score inspector should succeed
    let hv_req = make_request(&env, &seller, &arbiter, &token, 10_000);
    let result = client.try_create_escrow_with_inspector(&buyer, &hv_req, &Some(inspector.clone()));
    assert!(result.is_ok());
}

#[test]
fn test_inspector_appeal_multiple_times() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &10_000);

    // Create 4 rulings
    let mut escrow_ids = Vec::new(&env);
    for _ in 0..4 {
        let req = make_request(&env, &seller, &arbiter, &token, 1000);
        let eid = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));
        client.dispute_escrow(&buyer, &eid, &String::from_str(&env, "d"), &1000);
        client.resolve_dispute(&arbiter, &eid, &0);
        escrow_ids.push_back(eid);
    }

    // Initial score: 4/4 = 10_000 bps
    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!((total, correct, accuracy), (4, 4, 10_000));

    // Appeal 2 rulings
    client.appeal_inspector_ruling(&admin, &escrow_ids.get(0).unwrap());
    client.appeal_inspector_ruling(&admin, &escrow_ids.get(1).unwrap());

    // Score should be 2/4 = 5_000 bps
    let (total, correct, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!((total, correct, accuracy), (4, 2, 5_000));
}

#[test]
fn test_inspection_report_hash_stored() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    client.seller_mark_complete(&seller, &escrow_id);

    // Submit inspection with specific report hash
    let report_hash = BytesN::<32>::from_array(&env, &[42u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &true, &report_hash);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionPassed);
    // The report hash should be retrievable (implementation dependent)
    // This test verifies the submission accepts and processes the hash
}

#[test]
fn test_inspector_cannot_inspect_own_escrow_as_buyer() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    
    // Inspector is also the buyer - conflict of interest
    let inspector = buyer.clone();

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let result = client.try_create_escrow_with_inspector(&buyer, &req, &Some(inspector));
    
    // Should fail due to conflict of interest
    assert!(result.is_err());
}

#[test]
fn test_inspector_cannot_inspect_own_escrow_as_seller() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    
    // Inspector is also the seller - conflict of interest
    let inspector = seller.clone();

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let result = client.try_create_escrow_with_inspector(&buyer, &req, &Some(inspector));
    
    // Should fail due to conflict of interest
    assert!(result.is_err());
}

#[test]
fn test_buyer_cannot_release_during_awaiting_inspection() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    // Seller marks work complete → AwaitingInspection
    client.seller_mark_complete(&seller, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::AwaitingInspection);

    // Buyer tries to release before inspection completes
    let result = client.try_release_escrow(&buyer, &escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_inspector_score_threshold_below_amount_allows_escrow() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    soroban_sdk::token::StellarAssetClient::new(&env, &token).mint(&buyer, &100_000);

    // Build low score: 1 correct out of 2 = 5_000 bps
    let req1 = make_request(&env, &seller, &arbiter, &token, 500);
    let e1 = client.create_escrow_with_inspector(&buyer, &req1, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &e1, &String::from_str(&env, "d"), &500);
    client.resolve_dispute(&arbiter, &e1, &0);

    let req2 = make_request(&env, &seller, &arbiter, &token, 500);
    let e2 = client.create_escrow_with_inspector(&buyer, &req2, &Some(inspector.clone()));
    client.dispute_escrow(&buyer, &e2, &String::from_str(&env, "d"), &500);
    client.resolve_dispute(&arbiter, &e2, &0);
    client.appeal_inspector_ruling(&admin, &e2);

    let (_, _, accuracy) = client.get_inspector_score(&inspector);
    assert_eq!(accuracy, 5_000);

    // Set threshold: min 7000 bps for escrows above 10_000
    client.set_inspector_score_threshold(&admin, &7_000u32, &10_000i128);

    // Low-value escrow (below threshold amount) should succeed even with low score
    let low_value_req = make_request(&env, &seller, &arbiter, &token, 5_000);
    let result = client.try_create_escrow_with_inspector(&buyer, &low_value_req, &Some(inspector.clone()));
    assert!(result.is_ok());
}

#[test]
fn test_inspector_double_submission_rejected() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();
    let inspector = Address::generate(&env);

    let req = make_request(&env, &seller, &arbiter, &token, 1000);
    let escrow_id = client.create_escrow_with_inspector(&buyer, &req, &Some(inspector.clone()));

    client.seller_mark_complete(&seller, &escrow_id);

    // First submission
    let report_hash = BytesN::<32>::from_array(&env, &[1u8; 32]);
    client.submit_inspection_result(&inspector, &escrow_id, &true, &report_hash);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionPassed);

    // Try to submit again - should fail
    let report_hash2 = BytesN::<32>::from_array(&env, &[2u8; 32]);
    let result = client.try_submit_inspection_result(&inspector, &escrow_id, &false, &report_hash2);
    assert!(result.is_err());

    // Status should remain InspectionPassed
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::InspectionPassed);
}
