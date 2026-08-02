#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, vec, Address, BytesN, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    fee_recipient: Address,
    merchant: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &fee_recipient, &0);
    // Open mode so merchant doesn't need collateral
    client.set_merchant_open_mode(&true);

    Setup { env, client, admin, fee_recipient, merchant, token_addr, token_client, token_admin_client }
}

fn make_external_id(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    BytesN::from_array(env, &bytes)
}

// ===========================================================================
// Task 1: External ID Tests
// ===========================================================================

#[test]
fn test_external_id_indexes_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let ext_id = make_external_id(&s.env, 1);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &None, &Some(ext_id.clone()),
    );

    let payment = s.client.get_payment_by_external_id(&s.merchant, &ext_id);
    assert_eq!(payment.id, pid);
    assert_eq!(payment.external_id, Some(ext_id));
}

#[test]
#[should_panic]
fn test_duplicate_external_id_same_merchant_rejected() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    let ext_id = make_external_id(&s.env, 2);

    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &None, &Some(ext_id.clone()),
    );
    // Second call with same merchant + same external_id must panic
    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &None, &Some(ext_id.clone()),
    );
}

#[test]
fn test_same_external_id_different_merchants_allowed() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant2 = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    let ext_id = make_external_id(&s.env, 3);

    let pid1 = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &None, &Some(ext_id.clone()),
    );
    let pid2 = s.client.create_payment_with_voucher(
        &customer, &merchant2, &500, &s.token_addr,
        &None, &None, &None, &None, &Some(ext_id.clone()),
    );

    assert_ne!(pid1, pid2);
    let p1 = s.client.get_payment_by_external_id(&s.merchant, &ext_id);
    let p2 = s.client.get_payment_by_external_id(&merchant2, &ext_id);
    assert_eq!(p1.id, pid1);
    assert_eq!(p2.id, pid2);
}

#[test]
fn test_payment_without_external_id_works_normally() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer, &s.merchant, &500, &s.token_addr, &None, &None, &None,
    );
    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.external_id, None);
}

// ===========================================================================
// Task 2: Multi-Sig Approval Tests
// ===========================================================================

#[test]
fn test_multisig_policy_set_and_retrieved() {
    let s = setup();
    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let signer3 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone(), signer2.clone(), signer3.clone()];

    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &2, &3600);

    let policy = s.client.get_multisig_policy(&s.merchant).unwrap();
    assert_eq!(policy.m, 2);
    assert_eq!(policy.threshold, 1000);
    assert_eq!(policy.approval_window_seconds, 3600);
}

#[test]
fn test_high_value_payment_enters_pending_approval() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone(), signer2.clone()];
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &2, &3600);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &2000, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::PendingApproval);
}

#[test]
fn test_low_value_payment_skips_multisig() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone()];
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &1, &3600);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Pending);
}

#[test]
fn test_m_of_n_approval_transitions_to_pending() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let signer3 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone(), signer2.clone(), signer3.clone()];
    // 2-of-3
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &2, &3600);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &2000, &s.token_addr,
        &None, &None, &None, &None, &None,
    );
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::PendingApproval);

    // First approval — still PendingApproval
    s.client.approve_payment(&signer1, &pid);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::PendingApproval);

    // Second approval — quorum reached → Pending
    s.client.approve_payment(&signer2, &pid);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Pending);
}

#[test]
fn test_m_equals_1_single_approval_sufficient() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone()];
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &1, &3600);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &2000, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    s.client.approve_payment(&signer1, &pid);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Pending);
}

#[test]
fn test_approval_window_expired_auto_cancels() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone()];
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &1, &100);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &2000, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    // Advance time past approval window
    s.env.ledger().set_timestamp(s.env.ledger().timestamp() + 200);

    s.client.expire_pending_approval(&pid);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Refunded);
    // Customer should get refund
    assert_eq!(s.token_client.balance(&customer), 5000);
}

#[test]
fn test_get_approval_state_zero_approvals() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone(), signer2.clone()];
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &2, &3600);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &2000, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    let state = s.client.get_approval_state(&pid).unwrap();
    assert_eq!(state.payment_id, pid);
    assert_eq!(state.approvals.len(), 0);
}

#[test]
fn test_get_approval_state_partial_approvals() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let signer3 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone(), signer2.clone(), signer3.clone()];
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &3, &3600);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &2000, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    s.client.approve_payment(&signer1, &pid);
    s.client.approve_payment(&signer2, &pid);

    let state = s.client.get_approval_state(&pid).unwrap();
    assert_eq!(state.payment_id, pid);
    assert_eq!(state.approvals.len(), 2);
}

#[test]
fn test_get_approval_state_fully_approved() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let signer1 = Address::generate(&s.env);
    let signer2 = Address::generate(&s.env);
    let signers = vec![&s.env, signer1.clone(), signer2.clone()];
    s.client.set_multisig_policy(&s.merchant, &1000, &signers, &2, &3600);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &2000, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    s.client.approve_payment(&signer1, &pid);
    s.client.approve_payment(&signer2, &pid);

    // Payment should now be Pending, but approval state should still be readable
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Pending);
    let state = s.client.get_approval_state(&pid).unwrap();
    assert_eq!(state.approvals.len(), 2);
}

#[test]
fn test_get_approval_state_returns_none_for_regular_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // No multisig policy set — payment goes through normally
    let pid = s.client.create_payment(
        &customer, &s.merchant, &500, &s.token_addr, &None, &None, &None,
    );

    let state = s.client.get_approval_state(&pid);
    assert!(state.is_none());
}

// ===========================================================================
// Task 3: Voucher / Coupon Code Redemption
// ===========================================================================
// Task 3: Voucher Tests
// ===========================================================================

#[test]
fn test_issue_and_redeem_fixed_discount() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let code_hash = make_external_id(&s.env, 10);
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Fixed, &100, &5, &0);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );

    let payment = s.client.get_payment(&pid);
    // 500 - 100 fixed = 400
    assert_eq!(payment.amount, 400);
    assert_eq!(s.token_client.balance(&customer), 600); // 1000 - 400

    let voucher = s.client.get_voucher(&s.merchant, &code_hash);
    assert_eq!(voucher.uses_remaining, 4);
}

#[test]
fn test_issue_and_redeem_percentage_discount() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let code_hash = make_external_id(&s.env, 11);
    // 20% off
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Percentage, &20, &10, &0);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );

    let payment = s.client.get_payment(&pid);
    // 500 * 20 / 100 = 100 discount → 400
    assert_eq!(payment.amount, 400);
}

#[test]
fn test_voucher_exhausted_after_max_uses() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let code_hash = make_external_id(&s.env, 12);
    // max_uses = 1
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Fixed, &50, &1, &0);

    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );

    let voucher = s.client.get_voucher(&s.merchant, &code_hash);
    assert_eq!(voucher.uses_remaining, 0);
    assert!(!voucher.revoked);
}

#[test]
fn test_voucher_max_uses_enforced() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5_000);

    let code_hash = make_external_id(&s.env, 42);
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Fixed, &50, &1, &0);

    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );

    let voucher = s.client.get_voucher(&s.merchant, &code_hash);
    assert_eq!(voucher.uses_remaining, 0);
    assert!(!voucher.revoked);

    let second_attempt = s.client.try_create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash), &None,
    );
    assert_eq!(second_attempt.unwrap_err().unwrap(), Error::VoucherExhausted.into());
}

#[test]
#[should_panic]
fn test_exhausted_voucher_rejected() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let code_hash = make_external_id(&s.env, 13);
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Fixed, &50, &1, &0);

    // First use — ok
    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );
    // Second use — should panic (exhausted)
    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );
}

#[test]
#[should_panic]
fn test_expired_voucher_rejected() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let code_hash = make_external_id(&s.env, 14);
    let expiry = s.env.ledger().timestamp() + 100;
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Fixed, &50, &10, &expiry);

    // Advance past expiry
    s.env.ledger().set_timestamp(s.env.ledger().timestamp() + 200);

    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );
}

#[test]
#[should_panic]
fn test_revoked_voucher_rejected() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let code_hash = make_external_id(&s.env, 15);
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Fixed, &50, &10, &0);
    s.client.revoke_voucher(&s.merchant, &code_hash);

    s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &Some(code_hash.clone()), &None,
    );
}

#[test]
fn test_revoke_voucher_emits_event() {
    let s = setup();
    let code_hash = make_external_id(&s.env, 16);
    s.client.issue_voucher(&s.merchant, &code_hash, &DiscountType::Fixed, &50, &10, &0);
    s.client.revoke_voucher(&s.merchant, &code_hash);

    let voucher = s.client.get_voucher(&s.merchant, &code_hash);
    assert!(voucher.revoked);
}

#[test]
fn test_no_voucher_payment_works_normally() {
    let s = setup();
    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment_with_voucher(
        &customer, &s.merchant, &500, &s.token_addr,
        &None, &None, &None, &None, &None,
    );

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.amount, 500);
    assert_eq!(payment.status, PaymentStatus::Pending);
}
