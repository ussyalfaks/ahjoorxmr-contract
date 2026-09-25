#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, String, Vec,
};

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorRefundContractClient<'a>,
    admin: Address,
    merchant: Address,
    customer: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorRefundContract, ());
    let client = AhjoorRefundContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let customer = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin);
    client.set_escrow_contract_address(&admin, &Address::generate(&env));

    Setup {
        env,
        client,
        admin,
        merchant,
        customer,
        token_addr,
        token_client,
        token_admin_client,
    }
}

/// Create a refund request with the given amount and status.
fn create_refund_with_amount(s: &Setup, amount: i128, initial_status: RefundStatus) -> u32 {
    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    s.client.request_refund(
        &s.customer,
        &s.merchant,
        &amount,
        &s.token_addr,
        &String::from_str(&s.env, "test refund"),
        &request_deadline,
    );

    // Get the refund ID (should be 0 for first refund)
    let refund_id = 0u32;

    // If we need a different starting status, use the internal function
    if initial_status != RefundStatus::Requested {
        // For simplicity, we'll use the requested status in these tests
        // More complex state transitions would require direct storage manipulation
    }

    refund_id
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

/// Test approve_refund_as_voucher issues store credit instead of transferring tokens.
#[test]
fn test_approve_refund_as_voucher_creates_credit() {
    let s = setup();
    let refund_amount = 100i128;
    let credit_amount = 100i128; // 1:1 with refund amount
    let expiry_ledger = s.env.ledger().sequence() + 1000;

    // Mint tokens to customer and have them request a refund
    s.token_admin_client.mint(&s.customer, &refund_amount);

    // Customer has funds escrowed in the refund contract
    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &refund_amount,
        &s.token_addr,
        &String::from_str(&s.env, "test refund"),
        &request_deadline,
    );

    // Approve as voucher - this should create store credit, not transfer tokens
    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &expiry_ledger,
    );

    // Verify store credit was created
    let credit = s.client.get_store_credit(&s.merchant, &s.customer);
    assert_eq!(credit.credit_amount, credit_amount);
    assert_eq!(credit.expiry_ledger, expiry_ledger);
    assert!(!credit.extension_used);

    // Verify refund status is now Processed
    let refund = s.client.get_refund(&0u32);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert!(refund.processed_at.is_some());
}

/// Test approve_refund_as_voucher with bonus amount.
#[test]
fn test_approve_refund_as_voucher_with_bonus() {
    let s = setup();
    let refund_amount = 100i128;
    let bonus_bps = 1000; // 10% bonus
    let credit_amount = 110i128; // 100 + 10% bonus
    let expiry_ledger = s.env.ledger().sequence() + 1000;

    s.token_admin_client.mint(&s.customer, &refund_amount);

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &refund_amount,
        &s.token_addr,
        &String::from_str(&s.env, "test refund"),
        &request_deadline,
    );

    // Set max voucher bonus to 10%
    s.client.set_max_voucher_bonus_bps(&s.admin, &bonus_bps);

    // Approve as voucher with bonus
    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &expiry_ledger,
    );

    // Verify store credit includes bonus
    let credit = s.client.get_store_credit(&s.merchant, &s.customer);
    assert_eq!(credit.credit_amount, credit_amount);
}

/// Test that voucher bonus exceeding cap is rejected.
#[test]
#[should_panic(expected = "VoucherBonusExceedsCap")]
fn test_approve_refund_as_voucher_bonus_exceeds_cap() {
    let s = setup();
    let refund_amount = 100i128;
    let max_bonus_bps = 1000; // 10% max
    let credit_amount = 200i128; // 100% bonus - exceeds cap
    let expiry_ledger = s.env.ledger().sequence() + 1000;

    s.token_admin_client.mint(&s.customer, &refund_amount);

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &refund_amount,
        &s.token_addr,
        &String::from_str(&s.env, "test refund"),
        &request_deadline,
    );

    // Set max voucher bonus to 10%
    s.client.set_max_voucher_bonus_bps(&s.admin, &max_bonus_bps);

    // This should panic - bonus exceeds cap
    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &expiry_ledger,
    );
}

/// Test apply_store_credit spends credit against a future payment.
#[test]
fn test_apply_store_credit() {
    let s = setup();
    let credit_amount = 100i128;
    let apply_amount = 40i128;
    let expiry_ledger = s.env.ledger().sequence() + 1000;

    // Manually set up store credit by calling the internal function
    // For this test, we'll simulate the state that would exist after approve_refund_as_voucher
    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &expiry_ledger,
    );

    // Apply store credit to a "payment" (payment_id = 1 for this test)
    let applied = s.client.apply_store_credit(
        &s.customer,
        &s.merchant,
        &1u32,
        &apply_amount,
    );

    assert_eq!(applied, apply_amount);

    // Verify remaining credit
    let credit = s.client.get_store_credit(&s.merchant, &s.customer);
    assert_eq!(credit.credit_amount, credit_amount - apply_amount);
}

/// Test apply_store_credit with amount exceeding balance (partial redemption).
#[test]
fn test_apply_store_credit_partial_redemption() {
    let s = setup();
    let credit_amount = 50i128;
    let apply_amount = 100i128; // More than available
    let expiry_ledger = s.env.ledger().sequence() + 1000;

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &expiry_ledger,
    );

    // Apply more than available - should only apply what's available
    let applied = s.client.apply_store_credit(
        &s.customer,
        &s.merchant,
        &1u32,
        &apply_amount,
    );

    assert_eq!(applied, credit_amount); // All remaining credit applied

    // Verify credit is exhausted
    let credit = s.client.get_store_credit(&s.merchant, &s.customer);
    assert_eq!(credit.credit_amount, 0);
}

/// Test that expired store credit cannot be applied.
#[test]
#[should_panic(expected = "StoreCreditExpired")]
fn test_expired_store_credit_cannot_be_applied() {
    let s = setup();
    let credit_amount = 100i128;
    let apply_amount = 40i128;

    // Set expiry to current ledger (already expired)
    let expiry_ledger = s.env.ledger().sequence() as u64;

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &expiry_ledger,
    );

    // Advance past expiry
    s.env.ledger().set_sequence((expiry_ledger + 1) as u32);

    // This should panic - credit is expired
    s.client.apply_store_credit(
        &s.customer,
        &s.merchant,
        &1u32,
        &apply_amount,
    );
}

/// Test extend_store_credit_expiry postpones expiry.
#[test]
fn test_extend_store_credit_expiry() {
    let s = setup();
    let credit_amount = 100i128;
    let initial_expiry = s.env.ledger().sequence() as u64 + 500;
    let new_expiry = s.env.ledger().sequence() as u64 + 1000;

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &initial_expiry,
    );

    // Extend expiry
    s.client.extend_store_credit_expiry(
        &s.merchant,
        &s.customer,
        &new_expiry,
    );

    // Verify expiry was extended
    let credit = s.client.get_store_credit(&s.merchant, &s.customer);
    assert_eq!(credit.expiry_ledger, new_expiry);
    assert!(credit.extension_used);
}

/// Test that extension can only be used once.
#[test]
#[should_panic(expected = "StoreCreditExtensionAlreadyUsed")]
fn test_store_credit_extension_can_only_be_used_once() {
    let s = setup();
    let credit_amount = 100i128;
    let initial_expiry = s.env.ledger().sequence() as u64 + 500;
    let new_expiry = s.env.ledger().sequence() as u64 + 1000;

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &initial_expiry,
    );

    // First extension works
    s.client.extend_store_credit_expiry(
        &s.merchant,
        &s.customer,
        &new_expiry,
    );

    // Second extension should fail
    s.client.extend_store_credit_expiry(
        &s.merchant,
        &s.customer,
        &(new_expiry + 500),
    );
}

/// Test that only the issuing merchant can extend credit expiry.
#[test]
#[should_panic(expected = "Only the issuing merchant can extend store credit expiry")]
fn test_only_merchant_can_extend_expiry() {
    let s = setup();
    let credit_amount = 100i128;
    let expiry = s.env.ledger().sequence() as u64 + 500;
    let other_merchant = Address::generate(&s.env);

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &expiry,
    );

    // Try to extend with a different merchant - should fail
    s.client.extend_store_credit_expiry(
        &other_merchant,
        &s.customer,
        &(expiry + 500),
    );
}

/// Test that applying credit after extension still works before new expiry.
#[test]
fn test_apply_credit_after_extension() {
    let s = setup();
    let credit_amount = 100i128;
    let initial_expiry = s.env.ledger().sequence() as u64 + 100;
    let new_expiry = s.env.ledger().sequence() as u64 + 500;

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount,
        &initial_expiry,
    );

    // Advance to just before initial expiry
    s.env.ledger().set_sequence((initial_expiry - 1) as u32);

    // Extend expiry
    s.client.extend_store_credit_expiry(
        &s.merchant,
        &s.customer,
        &new_expiry,
    );

    // Advance to between old expiry and new expiry
    s.env.ledger().set_sequence((initial_expiry + 1) as u32);

    // Credit should still be valid and applicable
    let applied = s.client.apply_store_credit(
        &s.customer,
        &s.merchant,
        &1u32,
        &50i128,
    );

    assert_eq!(applied, 50);
}

/// Test that credit is accumulated when customer receives multiple vouchers from same merchant.
#[test]
fn test_store_credit_accumulates() {
    let s = setup();
    let credit_amount_1 = 50i128;
    let credit_amount_2 = 75i128;
    let expiry_1 = s.env.ledger().sequence() as u64 + 500;
    let expiry_2 = s.env.ledger().sequence() as u64 + 600;

    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    // First refund
    let _refund_id_1 = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test 1"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &0u32,
        &credit_amount_1,
        &expiry_1,
    );

    // Second refund
    let now = s.env.ledger().timestamp();
    let request_deadline = now + 1000;

    let _refund_id_2 = s.client.request_refund(
        &s.customer,
        &s.merchant,
        &100i128,
        &s.token_addr,
        &String::from_str(&s.env, "test 2"),
        &request_deadline,
    );

    s.client.approve_refund_as_voucher(
        &s.admin,
        &1u32,
        &credit_amount_2,
        &expiry_2,
    );

    // Verify credit is accumulated
    let credit = s.client.get_store_credit(&s.merchant, &s.customer);
    assert_eq!(credit.credit_amount, credit_amount_1 + credit_amount_2);
    // Should use the later expiry
    assert_eq!(credit.expiry_ledger, expiry_2);
}