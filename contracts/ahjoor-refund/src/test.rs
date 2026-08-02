#![cfg(test)]
extern crate alloc;
use super::*;
use proptest::prelude::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, String,
};

const UPGRADE_WASM: &[u8] = include_bytes!("../../../fixtures/upgrade_contract.wasm");

// ---------------------------------------------------------------------------
// Import the payments contract for cross-contract integration tests.
// ---------------------------------------------------------------------------
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

// ---------------------------------------------------------------------------
//  Test Helpers
// ---------------------------------------------------------------------------

/// Full integration setup: both payment and refund contracts deployed.
struct TestSetup<'a> {
    env: Env,
    refund_client: AhjoorRefundContractClient<'a>,
    payment_client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
}

fn setup<'a>() -> TestSetup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    // Deploy payment contract
    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    // Deploy refund contract
    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    // Initialize both contracts — 86400 s (1 day) dispute window
    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    TestSetup {
        env,
        refund_client,
        payment_client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
    }
}

/// Helper: create a completed payment and return its ID.
fn create_completed_payment<'a>(
    s: &TestSetup<'a>,
    customer: &Address,
    merchant: &Address,
    amount: i128,
) -> u32 {
    s.token_admin_client.mint(customer, &(amount * 2));
    let pid =
        s.payment_client
            .create_payment(customer, merchant, &amount, &s.token_addr, &None, &None, &None);
    s.payment_client.complete_payment(&pid);
    pid
}

// ===========================================================================
//  Initialize Tests
// ===========================================================================

#[test]
fn test_initialize() {
    let s = setup();
    assert_eq!(s.refund_client.get_refund_counter(), 0);
    assert_eq!(s.refund_client.get_admin(), s.admin);
    assert_eq!(
        s.refund_client.get_payment_contract(),
        s.payment_client.address
    );
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_initialize_twice_panics() {
    let s = setup();
    s.refund_client
        .initialize(&s.admin, &s.payment_client.address, &86_400u64, &None);
}

// ===========================================================================
//  Request Refund Tests (Cross-Contract Validation)
// ===========================================================================

#[test]
fn test_request_refund_against_completed_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);

    // Customer requests refund for 250 (half of payment)
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    assert_eq!(refund_id, 0);
    assert_eq!(s.refund_client.get_refund_counter(), 1);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Requested);
    assert_eq!(refund.amount, 250);
    assert_eq!(refund.customer, customer);
    assert_eq!(refund.payment_id, pid);
}

#[test]
#[should_panic(expected = "PaymentContractError: payment is not completed")]
fn test_request_refund_against_pending_payment_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    s.token_admin_client.mint(&customer, &500);
    let pid =
        s.payment_client
            .create_payment(&customer, &merchant, &500, &s.token_addr, &None, &None, &None);
    // Payment is still Pending — not Completed

    s.token_admin_client.mint(&customer, &100);
    s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "Pending payment"),
        &0u32,
    );
}

#[test]
#[should_panic(expected = "PaymentContractError: payment not found")]
fn test_request_refund_nonexistent_payment_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);

    s.token_admin_client.mint(&customer, &100);
    s.refund_client.request_refund(
        &customer,
        &9999,
        &100,
        &String::from_str(&s.env, "No such payment"),
        &0u32,
    );
}

#[test]
#[should_panic(expected = "ExceedsRefundableAmount")]
fn test_request_refund_exceeds_payment_amount_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 100);

    // Try to refund more than the payment amount
    s.token_admin_client.mint(&customer, &200);
    s.refund_client
        .request_refund(&customer, &pid, &200, &String::from_str(&s.env, "Too much"), &0u32);
}

#[test]
#[should_panic(expected = "Refund amount must be positive")]
fn test_request_refund_zero_amount_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 100);

    s.refund_client
        .request_refund(&customer, &pid, &0, &String::from_str(&s.env, "Invalid"), &0u32);
}

// ===========================================================================
//  Approve Refund Tests
// ===========================================================================

#[test]
fn test_approve_refund() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Approved);
    assert!(refund.approved_at.is_some());
}

#[test]
#[should_panic(expected = "Only admin or merchant delegate can approve refunds")]
fn test_approve_refund_by_non_admin_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let non_admin = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&non_admin, &refund_id);
}

#[test]
#[should_panic(expected = "Refund is not in a state that can be approved")]
fn test_approve_already_approved_refund_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.approve_refund(&s.admin, &refund_id);
}

// ===========================================================================
//  Reject Refund Tests
// ===========================================================================

#[test]
fn test_reject_refund() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.reject_refund(
        &s.admin,
        &refund_id,
        &String::from_str(&s.env, "Invalid reason"),
    );

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Rejected);
}

#[test]
#[should_panic(expected = "Only admin or merchant delegate can reject refunds")]
fn test_reject_refund_by_non_admin_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let non_admin = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.reject_refund(
        &non_admin,
        &refund_id,
        &String::from_str(&s.env, "Invalid reason"),
    );
}

// ===========================================================================
//  Process Refund Tests
// ===========================================================================

#[test]
fn test_process_refund() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.process_refund(&s.admin, &refund_id);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert!(refund.processed_at.is_some());
}

#[test]
#[should_panic(expected = "Only admin can process refunds")]
fn test_process_refund_by_non_admin_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let non_admin = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.process_refund(&non_admin, &refund_id);
}

#[test]
#[should_panic(expected = "Refund is not approved")]
fn test_process_unapproved_refund_panics() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.process_refund(&s.admin, &refund_id);
}

// ===========================================================================
//  Token Transfer Tests
// ===========================================================================

#[test]
fn test_token_transfer_on_process_refund() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    let balance_before = s.token_client.balance(&customer);

    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.process_refund(&s.admin, &refund_id);

    // Customer gets back the 250 they escrowed
    assert_eq!(s.token_client.balance(&customer), balance_before + 250);
}

#[test]
fn test_contract_holds_no_balance_after_process() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.process_refund(&s.admin, &refund_id);

    assert_eq!(s.token_client.balance(&s.refund_client.address), 0);
}

// ===========================================================================
//  Full Lifecycle Tests
// ===========================================================================

#[test]
fn test_full_refund_lifecycle_approved() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);

    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );
    assert_eq!(
        s.refund_client.get_refund(&refund_id).status,
        RefundStatus::Requested
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    assert_eq!(
        s.refund_client.get_refund(&refund_id).status,
        RefundStatus::Approved
    );

    s.refund_client.process_refund(&s.admin, &refund_id);
    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert!(refund.processed_at.is_some());
}

#[test]
fn test_full_refund_lifecycle_rejected() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);

    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.reject_refund(
        &s.admin,
        &refund_id,
        &String::from_str(&s.env, "Invalid reason"),
    );
    assert_eq!(
        s.refund_client.get_refund(&refund_id).status,
        RefundStatus::Rejected
    );
}

// ===========================================================================
//  Event Tests
// ===========================================================================

#[test]
fn test_refund_requested_emits_event() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    assert!(!s.env.events().all().is_empty());
}

#[test]
fn test_refund_approved_emits_event() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    assert!(!s.env.events().all().is_empty());
}

#[test]
fn test_refund_processed_emits_event() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &250);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &250,
        &String::from_str(&s.env, "Item not received"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.process_refund(&s.admin, &refund_id);
    assert!(!s.env.events().all().is_empty());
}

// ===========================================================================
//  Counter Tests
// ===========================================================================

#[test]
fn test_refund_counter_increments() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid1 = create_completed_payment(&s, &customer, &merchant, 500);
    let pid2 = create_completed_payment(&s, &customer, &merchant, 500);

    s.token_admin_client.mint(&customer, &300);
    s.refund_client.request_refund(
        &customer,
        &pid1,
        &100,
        &String::from_str(&s.env, "Reason 1"),
        &0u32,
    );
    s.refund_client.request_refund(
        &customer,
        &pid2,
        &200,
        &String::from_str(&s.env, "Reason 2"),
        &0u32,
    );

    assert_eq!(s.refund_client.get_refund_counter(), 2);
}

// ===========================================================================
//  Upgrade / Migration Tests
// ===========================================================================

#[test]
fn test_admin_upgrade_increments_version() {
    let s = setup();
    assert_eq!(s.refund_client.get_version(), 1);

    let wasm_hash = s.env.deployer().upload_contract_wasm(UPGRADE_WASM);
    s.refund_client.upgrade(&s.admin, &wasm_hash);

    let version: u32 = s.env.as_contract(&s.refund_client.address, || {
        s.env
            .storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_upgrade_by_non_admin_fails() {
    let s = setup();
    let intruder = Address::generate(&s.env);
    let wasm_hash = s.env.deployer().upload_contract_wasm(UPGRADE_WASM);
    assert!(s.refund_client.try_upgrade(&intruder, &wasm_hash).is_err());
    assert_eq!(s.refund_client.get_version(), 1);
}

#[test]
fn test_migration_runs_once_per_version() {
    let s = setup();
    s.refund_client.migrate(&s.admin);
    assert!(s.refund_client.try_migrate(&s.admin).is_err());
}

#[test]
fn test_upgrade_atomicity_with_invalid_hash() {
    let s = setup();
    let invalid_hash = BytesN::from_array(&s.env, &[9u8; 32]);
    assert!(s
        .refund_client
        .try_upgrade(&s.admin, &invalid_hash)
        .is_err());
    assert_eq!(s.refund_client.get_version(), 1);
}

// ===========================================================================
//  Pause Mechanism Tests
// ===========================================================================

#[test]
fn test_admin_can_pause_and_resume_contract() {
    let s = setup();
    let reason = String::from_str(&s.env, "Emergency maintenance");
    s.refund_client.pause_contract(&s.admin, &reason);

    assert_eq!(s.refund_client.is_paused(), true);
    assert_eq!(s.refund_client.get_pause_reason(), reason);

    s.refund_client.resume_contract(&s.admin);
    assert_eq!(s.refund_client.is_paused(), false);
    assert_eq!(
        s.refund_client.get_pause_reason(),
        String::from_str(&s.env, "")
    );
}

#[test]
fn test_non_admin_cannot_pause_or_resume() {
    let s = setup();
    let attacker = Address::generate(&s.env);
    assert!(s
        .refund_client
        .try_pause_contract(&attacker, &String::from_str(&s.env, "Malicious"))
        .is_err());

    s.refund_client
        .pause_contract(&s.admin, &String::from_str(&s.env, "Incident"));
    assert!(s.refund_client.try_resume_contract(&attacker).is_err());
}

#[test]
fn test_write_operations_blocked_when_paused() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &100);

    s.refund_client
        .pause_contract(&s.admin, &String::from_str(&s.env, "Emergency"));

    let res = s.refund_client.try_request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "reason"),
        &0u32,
    );
    assert!(res.is_err());
    assert_eq!(s.refund_client.get_refund_counter(), 0);
}

#[test]
fn test_recovery_after_resume() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &100);

    s.refund_client
        .pause_contract(&s.admin, &String::from_str(&s.env, "Emergency"));
    s.refund_client.resume_contract(&s.admin);

    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "post-resume"),
        &0u32,
    );
    assert_eq!(refund_id, 0);
    assert_eq!(s.refund_client.get_refund_counter(), 1);
}

// ===========================================================================
//  Boundary / Auth Tests
// ===========================================================================

#[test]
fn test_boundary_amount_i128_max_rejected_without_balance() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 1);
    s.token_admin_client.mint(&customer, &1);

    let res = s.refund_client.try_request_refund(
        &customer,
        &pid,
        &i128::MAX,
        &String::from_str(&s.env, "too large"),
        &0u32,
    );
    assert!(res.is_err());
}

#[test]
fn test_boundary_refund_id_not_found() {
    let s = setup();
    assert!(s.refund_client.try_get_refund(&9999u32).is_err());
}

#[test]
fn test_auth_required_for_admin_approve_refund() {
    let env = Env::default();
    let payment_id_addr = env.register(AhjoorPaymentsContract, ());
    let refund_id_addr = env.register(AhjoorRefundContract, ());
    let client = AhjoorRefundContractClient::new(&env, &refund_id_addr);
    let admin = Address::generate(&env);
    client.initialize(&admin, &payment_id_addr, &86_400u64, &None);

    let res = client.try_approve_refund(&admin, &0);
    assert!(res.is_err());
}

// ===========================================================================
//  Property-Based Tests
// ===========================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(120))]

    #[test]
    fn prop_total_refunded_never_exceeds_paid(
        paid in prop::collection::vec(1i128..500_000, 1..120),
        refunded in prop::collection::vec(0i128..500_000, 1..120),
    ) {
        let mut total_paid = 0i128;
        let mut total_refunded = 0i128;
        let len = core::cmp::min(paid.len(), refunded.len());

        for i in 0..len {
            total_paid += paid[i];
            total_refunded += core::cmp::min(refunded[i], paid[i]);
        }

        prop_assert!(total_refunded <= total_paid);
    }
}

// ===========================================================================
//  Indexing & Pagination Tests (#62)
// ===========================================================================

/// Helper: request N refunds from the same customer against N distinct payments.
/// Each payment has amount 100 and each refund requests 10.
/// Returns the vec of refund IDs in creation order.
fn request_n_refunds<'a>(
    s: &TestSetup<'a>,
    customer: &Address,
    merchant: &Address,
    n: u32,
) -> soroban_sdk::Vec<u32> {
    let mut ids = soroban_sdk::Vec::new(&s.env);
    for _ in 0..n {
        let pid = create_completed_payment(s, customer, merchant, 100);
        s.token_admin_client.mint(customer, &10);
        let rid = s.refund_client.request_refund(
            customer,
            &pid,
            &10,
            &String::from_str(&s.env, "reason"),
        &0u32,
        );
        ids.push_back(rid);
    }
    ids
}

// --- get_refunds_by_customer ---

#[test]
fn test_get_refunds_by_customer_empty() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let result = s.refund_client.get_refunds_by_customer(&customer, &10, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_get_refunds_by_customer_full_page() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let ids = request_n_refunds(&s, &customer, &merchant, 5);

    let page = s.refund_client.get_refunds_by_customer(&customer, &5, &0);
    assert_eq!(page.len(), 5);
    for i in 0..5u32 {
        assert_eq!(page.get(i).unwrap(), ids.get(i).unwrap());
    }
}

#[test]
fn test_get_refunds_by_customer_partial_page() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    request_n_refunds(&s, &customer, &merchant, 3);

    // ask for 10 but only 3 exist
    let page = s.refund_client.get_refunds_by_customer(&customer, &10, &0);
    assert_eq!(page.len(), 3);
}

#[test]
fn test_get_refunds_by_customer_second_page() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let ids = request_n_refunds(&s, &customer, &merchant, 5);

    // page size 2, offset 2 → items at index 2,3
    let page = s.refund_client.get_refunds_by_customer(&customer, &2, &2);
    assert_eq!(page.len(), 2);
    assert_eq!(page.get(0).unwrap(), ids.get(2).unwrap());
    assert_eq!(page.get(1).unwrap(), ids.get(3).unwrap());
}

#[test]
fn test_get_refunds_by_customer_offset_beyond_end() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    request_n_refunds(&s, &customer, &merchant, 3);

    let page = s
        .refund_client
        .get_refunds_by_customer(&customer, &10, &100);
    assert_eq!(page.len(), 0);
}

#[test]
fn test_get_refunds_by_customer_last_partial_page() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let ids = request_n_refunds(&s, &customer, &merchant, 5);

    // offset 4, limit 10 → only the last item
    let page = s.refund_client.get_refunds_by_customer(&customer, &10, &4);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), ids.get(4).unwrap());
}

// --- get_refunds_by_merchant ---

#[test]
fn test_get_refunds_by_merchant_empty() {
    let s = setup();
    let merchant = Address::generate(&s.env);
    let result = s.refund_client.get_refunds_by_merchant(&merchant, &10, &0);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_get_refunds_by_merchant_full_page() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let ids = request_n_refunds(&s, &customer, &merchant, 4);

    let page = s.refund_client.get_refunds_by_merchant(&merchant, &4, &0);
    assert_eq!(page.len(), 4);
    for i in 0..4u32 {
        assert_eq!(page.get(i).unwrap(), ids.get(i).unwrap());
    }
}

#[test]
fn test_get_refunds_by_merchant_second_page() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let ids = request_n_refunds(&s, &customer, &merchant, 6);

    // page size 3, offset 3 → items at index 3,4,5
    let page = s.refund_client.get_refunds_by_merchant(&merchant, &3, &3);
    assert_eq!(page.len(), 3);
    assert_eq!(page.get(0).unwrap(), ids.get(3).unwrap());
    assert_eq!(page.get(1).unwrap(), ids.get(4).unwrap());
    assert_eq!(page.get(2).unwrap(), ids.get(5).unwrap());
}

#[test]
fn test_get_refunds_by_merchant_offset_beyond_end() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    request_n_refunds(&s, &customer, &merchant, 2);

    let page = s.refund_client.get_refunds_by_merchant(&merchant, &5, &50);
    assert_eq!(page.len(), 0);
}

// --- get_refunds_by_payment ---

#[test]
fn test_get_refunds_by_payment_empty() {
    let s = setup();
    let result = s.refund_client.get_refunds_by_payment(&9999);
    assert_eq!(result.len(), 0);
}

#[test]
fn test_get_refunds_by_payment_single() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &10);
    let rid =
        s.refund_client
            .request_refund(&customer, &pid, &10, &String::from_str(&s.env, "reason"), &0u32);

    let result = s.refund_client.get_refunds_by_payment(&pid);
    assert_eq!(result.len(), 1);
    assert_eq!(result.get(0).unwrap(), rid);
}

#[test]
fn test_get_refunds_by_payment_multiple_refunds_same_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    // One payment, two partial refunds
    let pid = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &20);
    let rid0 =
        s.refund_client
            .request_refund(&customer, &pid, &10, &String::from_str(&s.env, "first"), &0u32);
    let rid1 =
        s.refund_client
            .request_refund(&customer, &pid, &10, &String::from_str(&s.env, "second"), &0u32);

    let result = s.refund_client.get_refunds_by_payment(&pid);
    assert_eq!(result.len(), 2);
    assert_eq!(result.get(0).unwrap(), rid0);
    assert_eq!(result.get(1).unwrap(), rid1);
}

// --- cross-index consistency ---

#[test]
fn test_indexes_consistent_across_all_three_dimensions() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &10);
    let rid =
        s.refund_client
            .request_refund(&customer, &pid, &10, &String::from_str(&s.env, "reason"), &0u32);

    assert_eq!(
        s.refund_client
            .get_refunds_by_customer(&customer, &10, &0)
            .get(0)
            .unwrap(),
        rid
    );
    assert_eq!(
        s.refund_client
            .get_refunds_by_merchant(&merchant, &10, &0)
            .get(0)
            .unwrap(),
        rid
    );
    assert_eq!(
        s.refund_client.get_refunds_by_payment(&pid).get(0).unwrap(),
        rid
    );
}

#[test]
fn test_different_customers_indexes_are_isolated() {
    let s = setup();
    let customer_a = Address::generate(&s.env);
    let customer_b = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid_a = create_completed_payment(&s, &customer_a, &merchant, 100);
    s.token_admin_client.mint(&customer_a, &10);
    s.refund_client
        .request_refund(&customer_a, &pid_a, &10, &String::from_str(&s.env, "a"), &0u32);

    let pid_b = create_completed_payment(&s, &customer_b, &merchant, 100);
    s.token_admin_client.mint(&customer_b, &10);
    s.refund_client
        .request_refund(&customer_b, &pid_b, &10, &String::from_str(&s.env, "b"), &0u32);

    assert_eq!(
        s.refund_client
            .get_refunds_by_customer(&customer_a, &10, &0)
            .len(),
        1
    );
    assert_eq!(
        s.refund_client
            .get_refunds_by_customer(&customer_b, &10, &0)
            .len(),
        1
    );
}

// ===========================================================================
//  Per-Payment Refund Cap Tracking Tests (#165)
// ===========================================================================

#[test]
fn test_get_refundable_remaining_full_before_any_refund() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 300);
    assert_eq!(s.refund_client.get_refundable_remaining(&pid), 300);
}

#[test]
fn test_get_refundable_remaining_decreases_after_processed_refund() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 300);
    s.token_admin_client.mint(&customer, &100);
    let rid = s
        .refund_client
        .request_refund(&customer, &pid, &100, &String::from_str(&s.env, "r1"), &0u32);
    s.refund_client.approve_refund(&s.admin, &rid);
    s.refund_client.process_refund(&s.admin, &rid);

    assert_eq!(s.refund_client.get_refundable_remaining(&pid), 200);
}

#[test]
fn test_three_partial_refunds_exhaust_payment() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    // Payment of 300; three refunds of 100 each
    let pid = create_completed_payment(&s, &customer, &merchant, 300);

    for i in 0..3u32 {
        s.token_admin_client.mint(&customer, &100);
        let reason = if i == 0 {
            String::from_str(&s.env, "first")
        } else if i == 1 {
            String::from_str(&s.env, "second")
        } else {
            String::from_str(&s.env, "third")
        };
        let rid = s
            .refund_client
            .request_refund(&customer, &pid, &100, &reason, &0u32);
        s.refund_client.approve_refund(&s.admin, &rid);
        s.refund_client.process_refund(&s.admin, &rid);
    }

    assert_eq!(s.refund_client.get_refundable_remaining(&pid), 0);
}

#[test]
fn test_cumulative_over_refund_rejected() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    // Payment of 200; refund 150, then try to refund 100 more (total 250 > 200)
    let pid = create_completed_payment(&s, &customer, &merchant, 200);

    s.token_admin_client.mint(&customer, &150);
    let rid = s
        .refund_client
        .request_refund(&customer, &pid, &150, &String::from_str(&s.env, "partial"), &0u32);
    s.refund_client.approve_refund(&s.admin, &rid);
    s.refund_client.process_refund(&s.admin, &rid);

    // 50 remaining — requesting 100 should fail
    s.token_admin_client.mint(&customer, &100);
    let result = s.refund_client.try_request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "over-refund"),
        &0u32,
    );
    assert!(result.is_err());
    assert_eq!(s.refund_client.get_refundable_remaining(&pid), 50);
}

#[test]
fn test_multiple_partial_refunds_accumulate_correctly() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);

    // Refund 100
    s.token_admin_client.mint(&customer, &100);
    let rid1 = s
        .refund_client
        .request_refund(&customer, &pid, &100, &String::from_str(&s.env, "r1"), &0u32);
    s.refund_client.approve_refund(&s.admin, &rid1);
    s.refund_client.process_refund(&s.admin, &rid1);
    assert_eq!(s.refund_client.get_refundable_remaining(&pid), 400);

    // Refund 150
    s.token_admin_client.mint(&customer, &150);
    let rid2 = s
        .refund_client
        .request_refund(&customer, &pid, &150, &String::from_str(&s.env, "r2"), &0u32);
    s.refund_client.approve_refund(&s.admin, &rid2);
    s.refund_client.process_refund(&s.admin, &rid2);
    assert_eq!(s.refund_client.get_refundable_remaining(&pid), 250);

    // Refund 250 (exactly remaining)
    s.token_admin_client.mint(&customer, &250);
    let rid3 = s
        .refund_client
        .request_refund(&customer, &pid, &250, &String::from_str(&s.env, "r3"), &0u32);
    s.refund_client.approve_refund(&s.admin, &rid3);
    s.refund_client.process_refund(&s.admin, &rid3);
    assert_eq!(s.refund_client.get_refundable_remaining(&pid), 0);
}

#[test]
fn test_cap_event_emitted_when_remaining_near_zero() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    // Payment of 100; refund 95 (remaining = 5, which is <= 10% of 100)
    let pid = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &95);
    let rid = s
        .refund_client
        .request_refund(&customer, &pid, &95, &String::from_str(&s.env, "near-cap"), &0u32);
    s.refund_client.approve_refund(&s.admin, &rid);
    s.refund_client.process_refund(&s.admin, &rid);

    use soroban_sdk::IntoVal;
    let events = s.env.events().all();
    let cap_event = events.iter().find(|e| {
        e.1 == (soroban_sdk::Symbol::new(&s.env, "partial_refund_cap_applied"),)
            .into_val(&s.env)
    });
    assert!(cap_event.is_some());
}

// ===========================================================================
//  Snapshot / Fuzz Tests
// ===========================================================================

#[test]
fn test_write_functions_blocked_when_paused_reads_still_work() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&customer, &100);
    let _ = s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "snapshot"),
        &0u32,
    );

    let events = s.env.events().all();
    assert!(!events.is_empty());
    let snapshot = alloc::format!("{:?}", events);
    assert!(!snapshot.is_empty());
}

// ===========================================================================
//  Auto-Approve Refund Tests (dispute window)
// ===========================================================================

/// Helper: create a setup with a custom dispute_window.
fn setup_with_dispute_window<'a>(dispute_window: u64) -> TestSetup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let payment_id = env.register(AhjoorPaymentsContract, ());
    let payment_client = AhjoorPaymentsContractClient::new(&env, &payment_id);

    let refund_id = env.register(AhjoorRefundContract, ());
    let refund_client = AhjoorRefundContractClient::new(&env, &refund_id);

    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    payment_client.initialize(&admin, &admin, &0u32);
    refund_client.initialize(&admin, &payment_id, &dispute_window, &None);

    TestSetup {
        env,
        refund_client,
        payment_client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
    }
}

#[test]
fn test_get_dispute_window_returns_configured_value() {
    let s = setup_with_dispute_window(3600);
    assert_eq!(s.refund_client.get_dispute_window(), 3600);
}

// --- #582: dispute_window minimum floor ---

#[test]
#[should_panic(expected = "DisputeWindowBelowMinimum")]
fn test_initialize_rejects_zero_dispute_window() {
    setup_with_dispute_window(0);
}

#[test]
#[should_panic(expected = "DisputeWindowBelowMinimum")]
fn test_initialize_rejects_dispute_window_below_floor() {
    setup_with_dispute_window(3599);
}

#[test]
fn test_set_dispute_window_updates_value_above_floor() {
    let s = setup_with_dispute_window(86_400);
    s.refund_client.set_dispute_window(&s.admin, &7_200u64);
    assert_eq!(s.refund_client.get_dispute_window(), 7_200);
}

#[test]
#[should_panic(expected = "DisputeWindowBelowMinimum")]
fn test_set_dispute_window_rejects_zero() {
    let s = setup_with_dispute_window(86_400);
    s.refund_client.set_dispute_window(&s.admin, &0u64);
}

#[test]
#[should_panic(expected = "Dispute window has not elapsed")]
fn test_auto_approve_early_panics() {
    let s = setup_with_dispute_window(86_400);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    // Ledger starts at timestamp 0; request at t=0
    s.env.ledger().set_timestamp(0);
    let pid = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &100);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "missing item"),
        &0u32,
    );

    // Only 1 hour has passed — window is 1 day
    s.env.ledger().set_timestamp(3600);
    s.refund_client.auto_approve_refund(&refund_id);
}

#[test]
#[should_panic(expected = "Refund has already been acted on")]
fn test_auto_approve_after_merchant_approved_panics() {
    let s = setup_with_dispute_window(86_400);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    s.env.ledger().set_timestamp(0);
    let pid = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &100);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "broken"),
        &0u32,
    );

    // Admin approves before the window elapses
    s.refund_client.approve_refund(&s.admin, &refund_id);

    // Advance past the dispute window
    s.env.ledger().set_timestamp(86_401);
    // Should panic because merchant already acted
    s.refund_client.auto_approve_refund(&refund_id);
}

#[test]
#[should_panic(expected = "Refund has already been acted on")]
fn test_auto_approve_after_merchant_rejected_panics() {
    let s = setup_with_dispute_window(86_400);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    s.env.ledger().set_timestamp(0);
    let pid = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &100);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "broken"),
        &0u32,
    );

    // Admin rejects before the window elapses
    s.refund_client.reject_refund(
        &s.admin,
        &refund_id,
        &String::from_str(&s.env, "not valid"),
    );

    // Advance past the dispute window
    s.env.ledger().set_timestamp(86_401);
    s.refund_client.auto_approve_refund(&refund_id);
}

#[test]
fn test_auto_approve_after_window_transfers_tokens_and_sets_processed() {
    let s = setup_with_dispute_window(86_400);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    s.env.ledger().set_timestamp(0);
    let pid = create_completed_payment(&s, &customer, &merchant, 200);
    s.token_admin_client.mint(&customer, &100);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "no response"),
        &0u32,
    );

    // Verify tokens are in escrow
    let balance_before = s.token_client.balance(&customer);

    // Advance one ledger past the boundary: requested_at(0) + dispute_window(86400) = 86400.
    // The window uses an exclusive boundary (consistent with escrow/rosca's permissionless
    // timeout functions, see #553): the dispute window has elapsed only once the current
    // timestamp is strictly greater than requested_at + dispute_window.
    s.env.ledger().set_timestamp(86_401);
    s.refund_client.auto_approve_refund(&refund_id);

    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert_eq!(refund.processed_at, Some(86_401));

    // Customer received the tokens back
    assert_eq!(s.token_client.balance(&customer), balance_before + 100);
}

#[test]
#[should_panic(expected = "Dispute window has not elapsed")]
fn test_auto_approve_exactly_at_window_boundary_panics() {
    // Boundary-ledger test for #553: at the exact instant the dispute window
    // elapses (now == requested_at + dispute_window) the refund is not yet
    // auto-approvable — this matches the exclusive-boundary convention used
    // by escrow's auto_release_expired/expire_cancellation/
    // expire_seller_transfer_veto and rosca's close_round/finalize_round.
    let s = setup_with_dispute_window(86_400);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    s.env.ledger().set_timestamp(0);
    let pid = create_completed_payment(&s, &customer, &merchant, 200);
    s.token_admin_client.mint(&customer, &100);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &100,
        &String::from_str(&s.env, "no response"),
        &0u32,
    );

    s.env.ledger().set_timestamp(86_400);
    s.refund_client.auto_approve_refund(&refund_id);
}

#[test]
fn test_auto_approve_emits_event() {
    let s = setup_with_dispute_window(3600);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    s.env.ledger().set_timestamp(0);
    let pid = create_completed_payment(&s, &customer, &merchant, 50);
    s.token_admin_client.mint(&customer, &50);
    let refund_id = s.refund_client.request_refund(
        &customer,
        &pid,
        &50,
        &String::from_str(&s.env, "auto"),
        &0u32,
    );

    s.env.ledger().set_timestamp(3601);
    s.refund_client.auto_approve_refund(&refund_id);

    let events = s.env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn test_refund_tiers_cap_amount_and_expire_after_all_windows() {
    let s = setup();

    let mut tiers = Vec::new(&s.env);
    tiers.push_back((86_400u64, 10_000u32));
    tiers.push_back((604_800u64, 5_000u32));
    s.refund_client.set_refund_tiers(&s.admin, &tiers);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    s.env.ledger().set_timestamp(100);
    let pid_a = create_completed_payment(&s, &customer, &merchant, 200);

    let rid_a = s.refund_client.request_refund(
        &customer,
        &pid_a,
        &300,
        &String::from_str(&s.env, "tier-a"),
        &0u32,
    );
    let refund_a = s.refund_client.get_refund(&rid_a);
    assert_eq!(refund_a.amount, 200);

    s.env.ledger().set_timestamp(200);
    let pid_b = create_completed_payment(&s, &customer, &merchant, 200);
    s.env.ledger().set_timestamp(200 + 2 * 86_400);
    let rid_b = s.refund_client.request_refund(
        &customer,
        &pid_b,
        &180,
        &String::from_str(&s.env, "tier-b"),
        &0u32,
    );
    let refund_b = s.refund_client.get_refund(&rid_b);
    assert_eq!(refund_b.amount, 100);

    s.env.ledger().set_timestamp(300);
    let pid_c = create_completed_payment(&s, &customer, &merchant, 200);
    s.env.ledger().set_timestamp(300 + 604_800 + 1);
    let expired = s.refund_client.try_request_refund(
        &customer,
        &pid_c,
        &50,
        &String::from_str(&s.env, "expired"),
        &0u32,
    );
    assert!(expired.is_err());
}

#[test]
fn test_merchant_refund_immediate_transfer_and_balance_deduction() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&merchant, &300);

    let customer_before = s.token_client.balance(&customer);
    let merchant_before = s.token_client.balance(&merchant);

    let rid = s
        .refund_client
        .merchant_refund(&merchant, &pid, &120, &42u32);

    let refund = s.refund_client.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert_eq!(s.token_client.balance(&customer), customer_before + 120);
    assert_eq!(s.token_client.balance(&merchant), merchant_before - 120);
}

#[test]
fn test_merchant_refund_rejects_non_payment_merchant() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let attacker = Address::generate(&s.env);

    let pid = create_completed_payment(&s, &customer, &merchant, 500);
    s.token_admin_client.mint(&attacker, &500);

    let res = s
        .refund_client
        .try_merchant_refund(&attacker, &pid, &100, &7u32);
    assert!(res.is_err());
}

#[test]
fn test_bulk_process_refunds_success_and_stats_update() {
    let s = setup();
    let customer_a = Address::generate(&s.env);
    let customer_b = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid_a = create_completed_payment(&s, &customer_a, &merchant, 200);
    let pid_b = create_completed_payment(&s, &customer_b, &merchant, 300);

    let rid_a = s.refund_client.request_refund(
        &customer_a,
        &pid_a,
        &100,
        &String::from_str(&s.env, "bulk-a"),
        &0u32,
    );
    let rid_b = s.refund_client.request_refund(
        &customer_b,
        &pid_b,
        &150,
        &String::from_str(&s.env, "bulk-b"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &rid_a);
    s.refund_client.approve_refund(&s.admin, &rid_b);

    let mut ids = Vec::new(&s.env);
    ids.push_back(rid_a);
    ids.push_back(rid_b);
    s.refund_client.bulk_process_refunds(&s.admin, &ids);

    assert_eq!(s.refund_client.get_refund(&rid_a).status, RefundStatus::Processed);
    assert_eq!(s.refund_client.get_refund(&rid_b).status, RefundStatus::Processed);

    let stats = s.refund_client.get_global_refund_stats();
    assert!(stats.total_processed >= 2);
}

#[test]
fn test_bulk_process_refunds_mixed_status_fails_atomically() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid_a = create_completed_payment(&s, &customer, &merchant, 200);
    let pid_b = create_completed_payment(&s, &customer, &merchant, 200);

    let rid_a = s.refund_client.request_refund(
        &customer,
        &pid_a,
        &50,
        &String::from_str(&s.env, "mix-a"),
        &0u32,
    );
    let rid_b = s.refund_client.request_refund(
        &customer,
        &pid_b,
        &60,
        &String::from_str(&s.env, "mix-b"),
        &0u32,
    );

    s.refund_client.approve_refund(&s.admin, &rid_a);

    let mut ids = Vec::new(&s.env);
    ids.push_back(rid_a);
    ids.push_back(rid_b);

    let res = s.refund_client.try_bulk_process_refunds(&s.admin, &ids);
    assert!(res.is_err());
    assert_eq!(s.refund_client.get_refund(&rid_a).status, RefundStatus::Approved);
    assert_eq!(s.refund_client.get_refund(&rid_b).status, RefundStatus::Requested);
}

#[test]
fn test_bulk_process_refunds_oversized_batch_rejected() {
    let s = setup();

    let mut ids = Vec::new(&s.env);
    for i in 0..21u32 {
        ids.push_back(i);
    }

    let res = s.refund_client.try_bulk_process_refunds(&s.admin, &ids);
    assert!(res.is_err());
}

// ===========================================================================
//  #583: export_refund_policy
// ===========================================================================

#[test]
fn test_export_refund_policy_returns_default_when_unset() {
    let s = setup();
    let merchant = Address::generate(&s.env);

    let policy = s.refund_client.export_refund_policy(&merchant);
    assert_eq!(policy.eligible_window_ledgers, u32::MAX);
    assert_eq!(policy.max_refund_bps, 10_000);
    assert_eq!(policy.excluded_tags.len(), 0);
}

// Seeds the global refund policy directly in storage. `set_global_refund_policy`
// itself calls `admin.require_auth()` and then `require_admin` (which calls
// `require_auth()` again), which soroban-sdk's `mock_all_auths` rejects as a
// duplicate authorization on the same frame — a pre-existing issue in that
// setter unrelated to #583, so we bypass it here to isolate the view logic.
fn seed_global_refund_policy(s: &TestSetup, policy: &RefundPolicy) {
    s.env.as_contract(&s.refund_client.address, || {
        s.env
            .storage()
            .instance()
            .set(&DataKey2::GlobalRefundPolicy, policy);
    });
}

#[test]
fn test_export_refund_policy_reflects_global_policy_immediately() {
    let s = setup();
    let merchant = Address::generate(&s.env);

    seed_global_refund_policy(
        &s,
        &RefundPolicy {
            eligible_window_ledgers: 1000,
            max_refund_bps: 5000,
            excluded_tags: Vec::new(&s.env),
        },
    );

    let policy = s.refund_client.export_refund_policy(&merchant);
    assert_eq!(policy.eligible_window_ledgers, 1000);
    assert_eq!(policy.max_refund_bps, 5000);
}

#[test]
fn test_export_refund_policy_prefers_merchant_over_global() {
    let s = setup();
    let merchant = Address::generate(&s.env);

    seed_global_refund_policy(
        &s,
        &RefundPolicy {
            eligible_window_ledgers: 1000,
            max_refund_bps: 5000,
            excluded_tags: Vec::new(&s.env),
        },
    );
    s.refund_client.publish_refund_policy(
        &merchant,
        &2000u32,
        &7500u32,
        &Vec::new(&s.env),
    );

    let policy = s.refund_client.export_refund_policy(&merchant);
    assert_eq!(policy.eligible_window_ledgers, 2000);
    assert_eq!(policy.max_refund_bps, 7500);
}

// ===========================================================================
//  #584: get_refund_stats_by_merchant
// ===========================================================================

#[test]
fn test_get_refund_stats_by_merchant_zeroed_for_no_activity() {
    let s = setup();
    let merchant = Address::generate(&s.env);

    let stats = s.refund_client.get_refund_stats_by_merchant(&merchant);
    assert_eq!(stats.total_requested, 0);
    assert_eq!(stats.total_approved, 0);
    assert_eq!(stats.total_rejected, 0);
    assert_eq!(stats.total_processed, 0);
    assert_eq!(stats.total_amount_refunded, 0);
}

#[test]
fn test_get_refund_stats_by_merchant_mixed_activity() {
    let s = setup();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);

    let pid_a = create_completed_payment(&s, &customer, &merchant, 100);
    s.token_admin_client.mint(&customer, &300);
    let rid_a = s.refund_client.request_refund(
        &customer,
        &pid_a,
        &100,
        &String::from_str(&s.env, "a"),
        &0u32,
    );
    s.refund_client.approve_refund(&s.admin, &rid_a);
    s.refund_client.process_refund(&s.admin, &rid_a);

    let pid_b = create_completed_payment(&s, &customer, &merchant, 60);
    let rid_b = s.refund_client.request_refund(
        &customer,
        &pid_b,
        &60,
        &String::from_str(&s.env, "b"),
        &0u32,
    );
    s.refund_client
        .reject_refund(&s.admin, &rid_b, &String::from_str(&s.env, "no"));

    let stats = s.refund_client.get_refund_stats_by_merchant(&merchant);
    assert_eq!(stats.total_requested, 2);
    assert_eq!(stats.total_approved, 1);
    assert_eq!(stats.total_rejected, 1);
    assert_eq!(stats.total_processed, 1);
    assert_eq!(stats.total_amount_refunded, 100);

    // Matches the existing get_merchant_refund_stats view exactly.
    assert_eq!(
        stats.total_requested,
        s.refund_client.get_merchant_refund_stats(&merchant).total_requested
    );
}
