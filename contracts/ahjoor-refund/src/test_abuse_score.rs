#![cfg(test)]
use super::*;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};
use soroban_sdk::token::{Client as TokenClient, StellarAssetClient as TokenAdminClient};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

fn setup_abuse<'a>() -> (
    Env,
    AhjoorRefundContractClient<'a>,
    AhjoorPaymentsContractClient<'a>,
    Address, // admin
    Address, // token
    TokenClient<'a>,
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    // Configure abuse scoring
    refund_client.set_abuse_block_threshold(&admin, &30u32);
    refund_client.set_block_duration_ledgers(&admin, &10_000u64);
    refund_client.set_rapid_submission_window(&admin, &100u32);

    (env, refund_client, payment_client, admin, token_addr, token_client, token_admin)
}

fn make_payment<'a>(
    env: &Env,
    payment_client: &AhjoorPaymentsContractClient<'a>,
    token_admin: &TokenAdminClient<'a>,
    customer: &Address,
    merchant: &Address,
    token: &Address,
    amount: i128,
) -> u32 {
    token_admin.mint(customer, &(amount * 2));
    let pid = payment_client.create_payment(customer, merchant, &amount, token, &None, &None, &None);
    payment_client.complete_payment(&pid);
    pid
}

#[test]
fn test_abuse_score_increments_on_denial() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &500);
    let rid = refund_client.request_refund(
        &customer, &pid, &500, &String::from_str(&env, "bad"), &0,
    );

    refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "invalid"));

    let record = refund_client.get_customer_abuse_score(&customer);
    assert_eq!(record.denied_count, 1);
    assert_eq!(record.abuse_score, 10);
}

#[test]
fn test_threshold_blocking() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // Submit and reject 3 refunds → score = 30 = threshold
    for _ in 0..3 {
        let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
        token_admin.mint(&customer, &100);
        let rid = refund_client.request_refund(
            &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
        );
        refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));
    }

    let record = refund_client.get_customer_abuse_score(&customer);
    assert!(record.abuse_score >= 30);
    assert!(record.blocked_until_ledger > 0);
}

#[test]
#[should_panic(expected = "CustomerBlockedForAbuse")]
fn test_blocked_customer_cannot_request() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // Hit threshold via 3 rejections
    for _ in 0..3 {
        let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
        token_admin.mint(&customer, &100);
        let rid = refund_client.request_refund(
            &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
        );
        refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));
    }

    // Next request must fail
    let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &100);
    refund_client.request_refund(&customer, &pid, &100, &String::from_str(&env, "bad"), &0);
}

#[test]
fn test_rapid_submission_detected() {
    let (env, refund_client, payment_client, _admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // First request
    let pid1 = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &100);
    refund_client.request_refund(&customer, &pid1, &100, &String::from_str(&env, "a"), &0);

    // Second request within rapid window (no ledger advance)
    let pid2 = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &100);
    refund_client.request_refund(&customer, &pid2, &100, &String::from_str(&env, "b"), &0);

    let record = refund_client.get_customer_abuse_score(&customer);
    assert_eq!(record.rapid_submission_count, 1);
    assert!(record.abuse_score >= 5);
}

#[test]
fn test_flag_refund_abuse_elevated_increment() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &100);
    let rid = refund_client.request_refund(
        &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
    );
    refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));

    // Standard rejection gives +10; flagging adds another +10 (elevated)
    let score_before = refund_client.get_customer_abuse_score(&customer).abuse_score;
    refund_client.flag_refund_abuse(&admin, &rid);
    let score_after = refund_client.get_customer_abuse_score(&customer).abuse_score;
    assert!(score_after > score_before);
}

#[test]
fn test_reset_customer_abuse_score() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &100);
    let rid = refund_client.request_refund(
        &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
    );
    refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));

    refund_client.reset_customer_abuse_score(&admin, &customer);

    let record = refund_client.get_customer_abuse_score(&customer);
    assert_eq!(record.abuse_score, 0);
    assert_eq!(record.denied_count, 0);
    assert_eq!(record.blocked_until_ledger, 0);
}

#[test]
fn test_score_decay_over_ledgers() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &100);
    let rid = refund_client.request_refund(
        &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
    );
    refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));

    let score_before = refund_client.get_customer_abuse_score(&customer).abuse_score;
    assert_eq!(score_before, 10);

    // Advance 10,000+ ledgers (decay should apply: -1 per 10,000 ledgers)
    let seq = env.ledger().sequence();
    env.ledger().set_sequence_number(seq + 10_001);

    let score_after = refund_client.get_customer_abuse_score(&customer).abuse_score;
    assert!(score_after < score_before, "score should have decayed");
}

#[test]
fn test_block_expiry_allows_new_requests() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // Set a very short block duration
    refund_client.set_block_duration_ledgers(&admin, &5u64);

    // Hit threshold
    for _ in 0..3 {
        let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
        token_admin.mint(&customer, &100);
        let rid = refund_client.request_refund(
            &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
        );
        refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));
    }

    let record = refund_client.get_customer_abuse_score(&customer);
    assert!(record.blocked_until_ledger > 0);

    // Advance past block expiry
    let seq = env.ledger().sequence();
    env.ledger().set_sequence_number(seq + 10_001);
    // Reset score to allow new request
    refund_client.reset_customer_abuse_score(&admin, &customer);

    let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
    token_admin.mint(&customer, &100);
    // Should not panic
    refund_client.request_refund(&customer, &pid, &100, &String::from_str(&env, "ok"), &0);
}

// --- #580: list_abuse_flagged_customers ---

#[test]
fn test_list_abuse_flagged_customers_only_above_threshold() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let flagged_customer = Address::generate(&env);
    let unflagged_customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    // 3 rejections push flagged_customer's score to 30 (>= threshold of 30).
    for _ in 0..3 {
        let pid = make_payment(&env, &payment_client, &token_admin, &flagged_customer, &merchant, &token_addr, 1000);
        token_admin.mint(&flagged_customer, &100);
        let rid = refund_client.request_refund(
            &flagged_customer, &pid, &100, &String::from_str(&env, "bad"), &0,
        );
        refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));
    }

    // A single rejection leaves unflagged_customer's score at 10 (< threshold).
    let pid = make_payment(&env, &payment_client, &token_admin, &unflagged_customer, &merchant, &token_addr, 1000);
    token_admin.mint(&unflagged_customer, &100);
    let rid = refund_client.request_refund(
        &unflagged_customer, &pid, &100, &String::from_str(&env, "bad"), &0,
    );
    refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));

    let flagged = refund_client.list_abuse_flagged_customers(&0, &10);
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged.get(0).unwrap().0, flagged_customer);
}

#[test]
fn test_reset_customer_no_longer_appears_in_flagged_list() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    for _ in 0..3 {
        let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
        token_admin.mint(&customer, &100);
        let rid = refund_client.request_refund(
            &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
        );
        refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));
    }

    assert_eq!(refund_client.list_abuse_flagged_customers(&0, &10).len(), 1);

    refund_client.reset_customer_abuse_score(&admin, &customer);

    assert_eq!(refund_client.list_abuse_flagged_customers(&0, &10).len(), 0);
}

#[test]
fn test_list_abuse_flagged_customers_pagination() {
    let (env, refund_client, payment_client, admin, token_addr, _tc, token_admin) = setup_abuse();
    let merchant = Address::generate(&env);

    let mut customers = soroban_sdk::Vec::new(&env);
    for _ in 0..4 {
        let customer = Address::generate(&env);
        for _ in 0..3 {
            let pid = make_payment(&env, &payment_client, &token_admin, &customer, &merchant, &token_addr, 1000);
            token_admin.mint(&customer, &100);
            let rid = refund_client.request_refund(
                &customer, &pid, &100, &String::from_str(&env, "bad"), &0,
            );
            refund_client.reject_refund(&admin, &rid, &String::from_str(&env, "no"));
        }
        customers.push_back(customer);
    }

    let page1 = refund_client.list_abuse_flagged_customers(&0, &2);
    let page2 = refund_client.list_abuse_flagged_customers(&2, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 2);

    let mut collected = soroban_sdk::Vec::new(&env);
    for (customer, _record) in page1.iter() {
        collected.push_back(customer);
    }
    for (customer, _record) in page2.iter() {
        collected.push_back(customer);
    }
    assert_eq!(collected, customers);
}

#[test]
fn test_get_rapid_submission_window() {
    let (env, refund_client, _payment_client, admin, _token, _token_client, _token_admin) =
        setup_abuse();

    // Default value when not explicitly set
    let default_client = {
        let env = Env::default();
        env.mock_all_auths();
        let refund_id = env.register(AhjoorRefundContract, ());
        AhjoorRefundContractClient::new(&env, &refund_id)
    };
    assert_eq!(default_client.get_rapid_submission_window(), 720);

    // Explicitly set value
    refund_client.set_rapid_submission_window(&admin, &500u32);
    assert_eq!(refund_client.get_rapid_submission_window(), 500);
}
