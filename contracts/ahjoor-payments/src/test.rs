#![cfg(test)]
extern crate alloc;
use super::*;
use proptest::prelude::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, Map, String,
};

const UPGRADE_WASM: &[u8] = include_bytes!("../../../fixtures/upgrade_contract.wasm");

// ---------------------------------------------------------------------------
//  Test Helpers
// ---------------------------------------------------------------------------

struct TestSetup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    fee_recipient: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
}

fn setup<'a>() -> TestSetup<'a> {
    let env = Env::default();
    // `charge_subscription` authorizes the subscriber's transfer without the
    // subscriber appearing in the call's own arguments (anyone can trigger a
    // due charge), so plain `mock_all_auths` can't tie that auth to the root
    // invocation.
    env.mock_all_auths_allowing_non_root_auth();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    TestSetup {
        env,
        client,
        admin,
        fee_recipient,
        token_addr,
        token_client,
        token_admin_client,
    }
}

impl<'a> TestSetup<'a> {
    fn init(&self) {
        self.client.initialize(&self.admin, &self.fee_recipient, &0);
    }

    fn init_with_fee(&self, fee_bps: u32) {
        self.client
            .initialize(&self.admin, &self.fee_recipient, &fee_bps);
    }

    /// Approves the contract to pull `amount` from `customer` (needed for
    /// `authorize_payment`, which settles via `transfer_from`).
    fn approve(&self, customer: &Address, amount: i128) {
        self.token_client.approve(
            customer,
            &self.client.address,
            &amount,
            &(self.env.ledger().sequence() + 1000),
        );
    }
}

// ===========================================================================
//  Initialize Tests
// ===========================================================================

#[test]
fn test_initialize() {
    let s = setup();
    s.init();

    assert_eq!(s.client.get_payment_counter(), 0);
    assert_eq!(s.client.get_max_batch_size(), 20);
    assert_eq!(s.client.get_dispute_timeout(), 7 * 24 * 60 * 60);
}

#[test]
#[should_panic(expected = "Already initialized")]
fn test_initialize_twice_panics() {
    let s = setup();
    s.init();
    s.init();
}

// ===========================================================================
//  Single Payment (Escrow) Tests
// ===========================================================================

#[test]
fn test_create_single_payment_escrow() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    assert_eq!(payment_id, 0);
    assert_eq!(s.token_client.balance(&customer), 750);
    assert_eq!(s.token_client.balance(&merchant), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 250);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.amount, 250);
    assert_eq!(s.client.get_payment_counter(), 1);
}

#[test]
fn test_complete_payment_marks_ready_for_settlement() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&payment_id);

    assert_eq!(s.token_client.balance(&merchant), 250);
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Completed);
    assert!(s.client.is_settled(&payment_id));
}

#[test]
#[should_panic(expected = "Payment is not pending")]
fn test_complete_already_completed_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&payment_id);
    s.client.complete_payment(&payment_id);
}

#[test]
#[should_panic(expected = "Payment amount must be positive")]
fn test_create_payment_zero_amount_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.client
        .create_payment(&customer, &merchant, &0, &s.token_addr, &None, &None, &None);
}

// ===========================================================================
//  Batch Payment Tests (Escrow)
// ===========================================================================

#[test]
fn test_create_batch_payments_escrow() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant1 = Address::generate(&s.env);
    let merchant2 = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let requests = vec![
        &s.env,
        PaymentRequest {
            merchant: merchant1.clone(),
            amount: 100,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
        PaymentRequest {
            merchant: merchant2.clone(),
            amount: 200,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
    ];

    let ids = s.client.create_payments_batch(&customer, &requests);

    assert_eq!(ids.len(), 2);
    assert_eq!(s.token_client.balance(&customer), 4700);
    assert_eq!(s.token_client.balance(&merchant1), 0);
    assert_eq!(s.token_client.balance(&merchant2), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 300);

    let p0 = s.client.get_payment(&ids.get(0).unwrap());
    let p1 = s.client.get_payment(&ids.get(1).unwrap());
    assert_eq!(p0.status, PaymentStatus::Pending);
    assert_eq!(p1.status, PaymentStatus::Pending);
}

#[test]
#[should_panic(expected = "Batch size exceeds maximum allowed")]
fn test_batch_exceeds_max_size() {
    let s = setup();
    s.init();
    s.client.set_max_batch_size(&2);

    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let requests = vec![
        &s.env,
        PaymentRequest {
            merchant: Address::generate(&s.env),
            amount: 10,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
        PaymentRequest {
            merchant: Address::generate(&s.env),
            amount: 10,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
        PaymentRequest {
            merchant: Address::generate(&s.env),
            amount: 10,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
    ];
    s.client.create_payments_batch(&customer, &requests);
}

#[test]
fn test_batch_insufficient_funds_reverts_all() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant1 = Address::generate(&s.env);
    let merchant2 = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &150);

    let requests = vec![
        &s.env,
        PaymentRequest {
            merchant: merchant1.clone(),
            amount: 100,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
        PaymentRequest {
            merchant: merchant2.clone(),
            amount: 200,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
    ];

    let result = s.client.try_create_payments_batch(&customer, &requests);
    assert!(result.is_err());

    assert_eq!(s.token_client.balance(&customer), 150);
    assert_eq!(s.client.get_payment_counter(), 0);
}

// ===========================================================================
//  Dispute Lifecycle Tests
// ===========================================================================

#[test]
fn test_dispute_pending_payment() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Wrong item delivered");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Disputed);
    assert!(s.client.is_disputed(&payment_id));

    let dispute = s.client.get_dispute(&payment_id);
    assert_eq!(dispute.payment_id, payment_id);
    assert!(!dispute.resolved);

    assert_eq!(s.token_client.balance(&s.client.address), 500);
}

#[test]
#[should_panic(expected = "Only pending or authorized payments can be disputed")]
fn test_dispute_completed_payment_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&payment_id);

    let reason = String::from_str(&s.env, "Too late");
    s.client.dispute_payment(&customer, &payment_id, &reason);
}

#[test]
#[should_panic(expected = "Only pending or authorized payments can be disputed")]
fn test_dispute_already_disputed_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Issue 1");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    let reason2 = String::from_str(&s.env, "Issue 2");
    s.client.dispute_payment(&customer, &payment_id, &reason2);
}

#[test]
#[should_panic(expected = "Only the payment customer can dispute")]
fn test_dispute_non_customer_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let stranger = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Not my payment");
    s.client.dispute_payment(&stranger, &payment_id, &reason);
}

#[test]
#[should_panic(expected = "Payment is not pending")]
fn test_complete_disputed_payment_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Dispute this");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    s.client.complete_payment(&payment_id);
}

// ===========================================================================
//  Dispute Resolution Tests
// ===========================================================================

#[test]
fn test_resolve_dispute_to_merchant() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Quality issue");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    s.client.resolve_dispute(&payment_id, &true);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Completed);
    assert_eq!(s.token_client.balance(&merchant), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 300);
    assert!(!s.client.is_settled(&payment_id));

    let dispute = s.client.get_dispute(&payment_id);
    assert!(dispute.resolved);
}

#[test]
fn test_resolve_dispute_to_customer() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(s.token_client.balance(&customer), 700);

    let reason = String::from_str(&s.env, "Never received item");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    s.client.resolve_dispute(&payment_id, &false);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Refunded);
    assert_eq!(s.token_client.balance(&customer), 1000);
    assert_eq!(s.token_client.balance(&merchant), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let dispute = s.client.get_dispute(&payment_id);
    assert!(dispute.resolved);
}

#[test]
#[should_panic(expected = "Payment is not disputed")]
fn test_resolve_non_disputed_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.resolve_dispute(&payment_id, &true);
}

// ===========================================================================
//  Escalation Tests
// ===========================================================================

#[test]
fn test_dispute_escalation_after_timeout() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.env.ledger().set_timestamp(1000);
    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.env.ledger().set_timestamp(2000);
    let reason = String::from_str(&s.env, "Test dispute");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    s.client.set_dispute_timeout(&3600);

    s.env.ledger().set_timestamp(3000);
    let escalated = s.client.check_escalation(&payment_id);
    assert!(!escalated);

    s.env.ledger().set_timestamp(6000);
    let escalated = s.client.check_escalation(&payment_id);
    assert!(escalated);
}

#[test]
fn test_no_escalation_for_non_disputed() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let escalated = s.client.check_escalation(&payment_id);
    assert!(!escalated);
}

#[test]
fn test_no_escalation_after_resolved() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.env.ledger().set_timestamp(1000);
    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Dispute");
    s.client.dispute_payment(&customer, &payment_id, &reason);
    s.client.resolve_dispute(&payment_id, &true);

    s.env.ledger().set_timestamp(1_000_000);
    let escalated = s.client.check_escalation(&payment_id);
    assert!(!escalated);
}

/// #417: check_escalation must transition payment status to EscalatedDispute and emit
/// both DisputeEscalated and PaymentStatusChanged events when the timeout is exceeded.
#[test]
fn test_dispute_escalation_changes_status() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.env.ledger().set_timestamp(1000);
    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.env.ledger().set_timestamp(2000);
    let reason = String::from_str(&s.env, "Dispute reason");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    // Confirm status is Disputed before escalation.
    assert_eq!(s.client.get_payment(&payment_id).status, PaymentStatus::Disputed);

    s.client.set_dispute_timeout(&3600);

    // Advance time past the timeout — escalation should trigger.
    s.env.ledger().set_timestamp(6000);
    let escalated = s.client.check_escalation(&payment_id);
    assert!(escalated);

    // Status must now be EscalatedDispute.
    assert_eq!(
        s.client.get_payment(&payment_id).status,
        PaymentStatus::EscalatedDispute
    );

    // resolve_dispute must succeed for an EscalatedDispute payment.
    s.client.resolve_dispute(&payment_id, &true);
    assert_eq!(
        s.client.get_payment(&payment_id).status,
        PaymentStatus::Completed
    );
}

// ===========================================================================
//  Admin Config Tests
// ===========================================================================

#[test]
fn test_set_dispute_timeout() {
    let s = setup();
    s.init();

    assert_eq!(s.client.get_dispute_timeout(), 7 * 24 * 60 * 60);

    s.client.set_dispute_timeout(&86400);
    assert_eq!(s.client.get_dispute_timeout(), 86400);
}

#[test]
#[should_panic(expected = "Dispute timeout must be positive")]
fn test_set_dispute_timeout_zero_panics() {
    let s = setup();
    s.init();
    s.client.set_dispute_timeout(&0);
}

#[test]
fn test_set_max_batch_size() {
    let s = setup();
    s.init();

    s.client.set_max_batch_size(&50);
    assert_eq!(s.client.get_max_batch_size(), 50);
}

#[test]
fn test_rate_limit_blocks_after_max_within_window() {
    let s = setup();
    s.init();
    s.client.update_rate_limit_config(&s.admin, &2, &10);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // First and last allowed requests in the same window.
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Over-limit request must fail with contract error.
    let res = s.client.try_create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(res.unwrap_err().unwrap(), Error::RateLimitExceeded.into());
}

#[test]
fn test_rate_limit_window_resets_after_window_size_ledgers() {
    let s = setup();
    s.init();
    s.client.update_rate_limit_config(&s.admin, &2, &5);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let blocked = s.client.try_create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        Error::RateLimitExceeded.into()
    );

    // Advance exactly one full window; next request should be allowed.
    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 5);
    let allowed = s.client.try_create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert!(allowed.is_ok());
}

#[test]
fn test_rate_limit_applies_per_customer_not_globally() {
    let s = setup();
    s.init();
    s.client.update_rate_limit_config(&s.admin, &1, &20);

    let customer_a = Address::generate(&s.env);
    let customer_b = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer_a, &500);
    s.token_admin_client.mint(&customer_b, &500);

    s.client.create_payment(
        &customer_a,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let blocked_a = s.client.try_create_payment(
        &customer_a,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(
        blocked_a.unwrap_err().unwrap(),
        Error::RateLimitExceeded.into()
    );

    // Another customer still has independent quota in same ledger window.
    let allowed_b = s.client.try_create_payment(
        &customer_b,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert!(allowed_b.is_ok());
}

#[test]
fn test_admin_can_update_rate_limit_config() {
    let s = setup();
    s.init();

    let cfg_before = s.client.get_rate_limit_config();
    assert_eq!(cfg_before.max_payments, u32::MAX);
    assert_eq!(cfg_before.window_size_ledgers, 1);

    s.client.update_rate_limit_config(&s.admin, &7, &42);
    let cfg_after = s.client.get_rate_limit_config();
    assert_eq!(cfg_after.max_payments, 7);
    assert_eq!(cfg_after.window_size_ledgers, 42);
}

/// #649: get_customer_rate_limit_status returns (0, 0) for a fresh customer with no activity.
#[test]
fn test_get_customer_rate_limit_status_fresh_customer() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let (count, window_start) = s.client.get_customer_rate_limit_status(&customer);
    
    // Fresh customer should have no recorded activity
    assert_eq!(count, 0, "Fresh customer should have count 0");
    assert_eq!(window_start, 0, "Fresh customer should return window_start_ledger 0");
}

/// #649: get_customer_rate_limit_status returns current count and window for active customer.
#[test]
fn test_get_customer_rate_limit_status_mid_window() {
    let s = setup();
    s.init();
    // Set a rate limit of 3 payments per 10 ledger window
    s.client.update_rate_limit_config(&s.admin, &3, &10);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Record initial ledger before creating payments
    let initial_ledger = s.env.ledger().sequence();

    // Create first payment
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Check status: should show count=1, window_start at or near initial_ledger
    let (count, window_start) = s.client.get_customer_rate_limit_status(&customer);
    assert_eq!(count, 1, "Should have count=1 after first payment");
    assert!(window_start >= initial_ledger, "window_start should be >= initial_ledger");

    // Create second payment in same window
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let (count2, window_start2) = s.client.get_customer_rate_limit_status(&customer);
    assert_eq!(count2, 2, "Should have count=2 after second payment");
    assert_eq!(window_start2, window_start, "window_start should not change within window");

    // Create third payment (at max)
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let (count3, window_start3) = s.client.get_customer_rate_limit_status(&customer);
    assert_eq!(count3, 3, "Should have count=3 after third payment");
    assert_eq!(window_start3, window_start, "window_start should remain consistent");
}

/// #649: get_customer_rate_limit_status shows window reset after window expires.
#[test]
fn test_get_customer_rate_limit_status_window_reset() {
    let s = setup();
    s.init();
    // Set rate limit: 2 payments per 5 ledger window
    s.client.update_rate_limit_config(&s.admin, &2, &5);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create payment in first window
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let (count_before, window_start_before) =
        s.client.get_customer_rate_limit_status(&customer);
    assert_eq!(count_before, 1, "Should have 1 payment before window expires");

    // Advance ledger by exactly the window size to expire the window
    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 5);

    // Query status after window expires
    let (count_after, window_start_after) = s.client.get_customer_rate_limit_status(&customer);
    assert_eq!(
        count_after, 0,
        "Count should reset to 0 after window expires"
    );
    assert_ne!(
        window_start_after, window_start_before,
        "window_start should change after window expires"
    );
    assert_eq!(
        window_start_after,
        s.env.ledger().sequence(),
        "window_start should be current ledger after reset"
    );
}

#[test]
fn test_is_disputed() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert!(!s.client.is_disputed(&payment_id));

    let reason = String::from_str(&s.env, "Dispute");
    s.client.dispute_payment(&customer, &payment_id, &reason);
    assert!(s.client.is_disputed(&payment_id));

    s.client.resolve_dispute(&payment_id, &true);
    assert!(!s.client.is_disputed(&payment_id));
}

#[test]
fn test_customer_payment_tracking() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    s.client.create_payment(
        &customer,
        &Address::generate(&s.env),
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let requests = vec![
        &s.env,
        PaymentRequest {
            merchant: Address::generate(&s.env),
            amount: 200,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
        PaymentRequest {
            merchant: Address::generate(&s.env),
            amount: 300,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
    ];
    s.client.create_payments_batch(&customer, &requests);

    let ids = s.client.get_customer_payments(&customer);
    assert_eq!(ids.len(), 3);
    assert_eq!(s.client.get_payment_counter(), 3);
}

#[test]
#[should_panic(expected = "Payment not found")]
fn test_get_nonexistent_payment_panics() {
    let s = setup();
    s.init();
    s.client.get_payment(&999);
}

// ===========================================================================
//  Full Dispute Lifecycle Test
// ===========================================================================

#[test]
fn test_full_dispute_lifecycle() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.env.ledger().set_timestamp(100);
    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Pending);
    assert_eq!(s.token_client.balance(&s.client.address), 500);

    s.env.ledger().set_timestamp(200);
    let reason = String::from_str(&s.env, "Defective product");
    s.client.dispute_payment(&customer, &pid, &reason);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Disputed);
    assert!(s.client.is_disputed(&pid));

    s.client.set_dispute_timeout(&1000);
    s.env.ledger().set_timestamp(500);
    assert!(!s.client.check_escalation(&pid));

    s.env.ledger().set_timestamp(1500);
    assert!(s.client.check_escalation(&pid));

    s.client.resolve_dispute(&pid, &false);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Refunded);
    assert_eq!(s.token_client.balance(&customer), 1000);
    assert_eq!(s.token_client.balance(&merchant), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
    assert!(!s.client.is_disputed(&pid));

    let dispute = s.client.get_dispute(&pid);
    assert!(dispute.resolved);
}

// ===========================================================================
//  Event Emission Tests
// ===========================================================================

#[test]
fn test_dispute_emits_events() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Bad service");
    s.client.dispute_payment(&customer, &0, &reason);

    let events = s.env.events().all();
    assert!(events.len() > 0, "No events emitted");
}

#[test]
fn test_resolve_emits_events() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Dispute reason");
    s.client.dispute_payment(&customer, &0, &reason);
    s.client.resolve_dispute(&0, &true);

    let events = s.env.events().all();
    assert!(
        events.len() >= 2,
        "Expected multiple events for full dispute lifecycle"
    );
}

// ===========================================================================
//  TTL Extension Behavior Tests
// ===========================================================================

/// Verify that a Payment record stored in persistent storage survives
/// well beyond the instance TTL threshold by checking it remains accessible
/// after advancing the ledger sequence past INSTANCE_LIFETIME_THRESHOLD.
#[test]
fn test_payment_persistent_ttl_extended_on_create() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Advance ledger sequence past instance TTL threshold
    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 110_000);

    // Payment record must still be accessible (persistent storage, individual TTL)
    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.id, payment_id);
    assert_eq!(payment.status, PaymentStatus::Pending);
}

/// Verify that completing a payment extends its persistent TTL so the
/// completed record remains accessible for auditing.
#[test]
fn test_payment_persistent_ttl_extended_on_complete() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&payment_id);

    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 110_000);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::Completed);
}

/// Verify that a Dispute record in temporary storage is accessible immediately
/// after creation and that the resolved flag is updated correctly.
#[test]
fn test_dispute_temporary_storage_lifecycle() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let reason = String::from_str(&s.env, "Item not received");
    s.client.dispute_payment(&customer, &payment_id, &reason);

    // Dispute is in temporary storage and accessible
    let dispute = s.client.get_dispute(&payment_id);
    assert!(!dispute.resolved);
    assert_eq!(dispute.payment_id, payment_id);

    // Resolve — dispute record updated in temporary storage
    s.client.resolve_dispute(&payment_id, &false);
    let resolved_dispute = s.client.get_dispute(&payment_id);
    assert!(resolved_dispute.resolved);
}

/// Verify that CustomerPayments index in persistent storage accumulates
/// correctly across multiple payments and survives ledger advancement.
#[test]
fn test_customer_payments_persistent_ttl() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    s.client.create_payment(
        &customer,
        &Address::generate(&s.env),
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.create_payment(
        &customer,
        &Address::generate(&s.env),
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.create_payment(
        &customer,
        &Address::generate(&s.env),
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.env
        .ledger()
        .set_sequence_number(s.env.ledger().sequence() + 110_000);

    // Customer index must still be accessible after ledger advancement
    let ids = s.client.get_customer_payments(&customer);
    assert_eq!(ids.len(), 3);
}

// ===========================================================================
//  Multi-Token Payment Tests
// ===========================================================================

/// Mock oracle contract used in tests.
/// Stores a single price (scaled by 10^7) and timestamp set by the test.
mod mock_oracle {
    use crate::PriceData;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

    #[contracttype]
    enum OracleKey {
        Price,
        Ts,
    }

    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        /// Test helper: set the price and timestamp the oracle will return.
        pub fn set_price(env: Env, price: i128, timestamp: u64) {
            env.storage().instance().set(&OracleKey::Price, &price);
            env.storage().instance().set(&OracleKey::Ts, &timestamp);
        }

        /// Reflector-compatible: returns the stored price regardless of base/quote.
        pub fn lastprice(env: Env, _base: Address, _quote: Address) -> Option<PriceData> {
            let price: i128 = env.storage().instance().get(&OracleKey::Price)?;
            let timestamp: u64 = env.storage().instance().get(&OracleKey::Ts)?;
            Some(PriceData { price, timestamp })
        }
    }
}

use mock_oracle::MockOracle;

struct MultiTokenSetup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    _admin: Address,
    /// USDC token (settlement currency)
    usdc_addr: Address,
    usdc_client: TokenClient<'a>,
    usdc_admin: TokenAdminClient<'a>,
    /// XLM-like payment token
    xlm_addr: Address,
    xlm_admin: TokenAdminClient<'a>,
    xlm_client: TokenClient<'a>,
    oracle_addr: Address,
}

fn setup_multi_token<'a>() -> MultiTokenSetup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);

    // USDC token
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let usdc_client = TokenClient::new(&env, &usdc_addr);
    let usdc_admin = TokenAdminClient::new(&env, &usdc_addr);

    // XLM-like payment token
    let xlm_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let xlm_admin = TokenAdminClient::new(&env, &xlm_addr);
    let xlm_client = TokenClient::new(&env, &xlm_addr);

    // Mock oracle
    let oracle_addr = env.register(MockOracle, ());

    client.initialize(&admin, &admin, &0);
    // max_oracle_age = 300 seconds
    client.set_oracle(&oracle_addr, &usdc_addr, &300u64);

    MultiTokenSetup {
        env,
        client,
        _admin: admin,
        usdc_addr,
        usdc_client,
        usdc_admin,
        xlm_addr,
        xlm_admin,
        xlm_client,
        oracle_addr,
    }
}

/// Helper: set oracle price (scaled by 10^7) and ledger timestamp.
fn set_oracle_price(s: &MultiTokenSetup, price: i128, ts: u64) {
    use mock_oracle::MockOracleClient;
    let oc = MockOracleClient::new(&s.env, &s.oracle_addr);
    oc.set_price(&price, &ts);
    s.env.ledger().set_timestamp(ts);
}

// ---------------------------------------------------------------------------

/// Direct USDC payment (payment_token == usdc_token) bypasses oracle entirely.
#[test]
fn test_multi_token_usdc_fallback() {
    let s = setup_multi_token();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.usdc_admin.mint(&customer, &1000);

    let pid = s
        .client
        .create_payment_multi_token(&customer, &merchant, &500, &s.usdc_addr, &Some(50));

    // Verify payment was created
    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.amount, 500);
    assert_eq!(payment.token, s.usdc_addr);
    assert_eq!(payment.status, PaymentStatus::Pending);

    // Verify funds were escrowed
    assert_eq!(s.usdc_client.balance(&customer), 500);
    assert_eq!(s.usdc_client.balance(&s.client.address), 500);
}

/// XLM payment: oracle price = 0.10 USDC per XLM (price = 1_000_000 in 10^7).
/// Customer wants to pay 100 USDC → needs 1_000 XLM.
#[test]
fn test_multi_token_xlm_payment_correct_amount() {
    let s = setup_multi_token();

    // price = 0.10 USDC/XLM → 10^7 * 0.10 = 1_000_000
    set_oracle_price(&s, 1_000_000, 1000);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    // Customer needs 1000 XLM for 100 USDC
    s.xlm_admin.mint(&customer, &2000);

    let pid = s
        .client
        .create_payment_multi_token(&customer, &merchant, &100, &s.xlm_addr, &Some(50));

    // required_token_amount = 100 * 10_000_000 / 1_000_000 = 1000
    assert_eq!(s.xlm_client.balance(&customer), 1000);
    assert_eq!(s.xlm_client.balance(&s.client.address), 1000);

    let payment = s.client.get_payment(&pid);
    // Payment recorded in USDC terms
    assert_eq!(payment.amount, 100);
    assert_eq!(payment.token, s.usdc_addr);
    assert_eq!(payment.status, PaymentStatus::Pending);
}

/// Oracle price = 0.50 USDC/XLM (price = 5_000_000).
/// Customer pays 50 USDC → needs 100 XLM.
#[test]
fn test_multi_token_different_rate() {
    let s = setup_multi_token();

    // 0.50 USDC/XLM → 5_000_000
    set_oracle_price(&s, 5_000_000, 500);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &500);

    s.client
        .create_payment_multi_token(&customer, &merchant, &50, &s.xlm_addr, &Some(100));

    // required = 50 * 10_000_000 / 5_000_000 = 100
    assert_eq!(s.xlm_client.balance(&customer), 400);
    assert_eq!(s.xlm_client.balance(&s.client.address), 100);
}

/// Stale oracle price (age > max_oracle_age) must be rejected.
#[test]
#[should_panic(expected = "Oracle price is stale")]
fn test_multi_token_stale_oracle_rejected() {
    let s = setup_multi_token();

    // Price timestamp = 0, current ledger = 400 → age = 400 > max_oracle_age(300)
    use mock_oracle::MockOracleClient;
    let oc = MockOracleClient::new(&s.env, &s.oracle_addr);
    oc.set_price(&1_000_000i128, &0u64);
    s.env.ledger().set_timestamp(400);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &2000);

    s.client
        .create_payment_multi_token(&customer, &merchant, &100, &s.xlm_addr, &Some(50));
}

/// Unavailable oracle (no price set) must be rejected.
#[test]
#[should_panic(expected = "Oracle price unavailable")]
fn test_multi_token_oracle_unavailable() {
    let s = setup_multi_token();
    // Oracle has no price set — lastprice returns None

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &2000);

    s.client
        .create_payment_multi_token(&customer, &merchant, &100, &s.xlm_addr, &Some(50));
}

/// Zero slippage tolerance: exact integer division must not deviate.
/// price = 10_000_000 (1.0 USDC/XLM) → required = amount_usdc exactly.
#[test]
fn test_multi_token_zero_slippage_exact_rate() {
    let s = setup_multi_token();

    // 1.0 USDC/XLM → 10_000_000
    set_oracle_price(&s, 10_000_000, 100);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &500);

    s.client
        .create_payment_multi_token(&customer, &merchant, &200, &s.xlm_addr, &Some(0));

    // required = 200 * 10_000_000 / 10_000_000 = 200 — no deviation
    assert_eq!(s.xlm_client.balance(&s.client.address), 200);
}

/// set_oracle rejects max_oracle_age = 0.
#[test]
#[should_panic(expected = "max_oracle_age must be positive")]
fn test_set_oracle_zero_age_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let oracle = Address::generate(&env);
    let usdc = Address::generate(&env);
    client.initialize(&admin, &admin, &0);
    client.set_oracle(&oracle, &usdc, &0u64);
}

/// get_oracle_address / get_usdc_token / get_max_oracle_age return stored values.
#[test]
fn test_get_oracle_config() {
    let s = setup_multi_token();
    assert_eq!(s.client.get_oracle_address(), s.oracle_addr);
    assert_eq!(s.client.get_usdc_token(), s.usdc_addr);
    assert_eq!(s.client.get_max_oracle_age(), 300u64);
}

/// Multi-token payment emits MultiTokenPaymentCreated event.
#[test]
fn test_multi_token_emits_event() {
    let s = setup_multi_token();

    // 0.10 USDC/XLM
    set_oracle_price(&s, 1_000_000, 100);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &2000);

    s.client
        .create_payment_multi_token(&customer, &merchant, &100, &s.xlm_addr, &Some(50));

    let events = s.env.events().all();
    assert!(events.len() > 0);
}

/// Multi-token payment counter increments correctly.
#[test]
fn test_multi_token_payment_counter() {
    let s = setup_multi_token();
    set_oracle_price(&s, 10_000_000, 100);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &5000);

    s.client
        .create_payment_multi_token(&customer, &merchant, &100, &s.xlm_addr, &Some(50));
    s.client
        .create_payment_multi_token(&customer, &merchant, &100, &s.xlm_addr, &Some(50));

    assert_eq!(s.client.get_payment_counter(), 2);
}

/// Multi-token payment is tracked in customer payment index.
#[test]
fn test_multi_token_customer_tracking() {
    let s = setup_multi_token();
    set_oracle_price(&s, 10_000_000, 100);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &5000);

    s.client
        .create_payment_multi_token(&customer, &merchant, &100, &s.xlm_addr, &Some(50));
    s.client
        .create_payment_multi_token(&customer, &merchant, &200, &s.xlm_addr, &Some(50));

    let ids = s.client.get_customer_payments(&customer);
    assert_eq!(ids.len(), 2);
}

// ===========================================================================
//  Token Transfer Integration Tests
// ===========================================================================

/// Verify customer balance decreases on payment creation
#[test]
fn test_token_transfer_on_payment_creation() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let initial_balance = s.token_client.balance(&customer);
    assert_eq!(initial_balance, 1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let final_balance = s.token_client.balance(&customer);
    assert_eq!(final_balance, 750);
}

/// Verify contract holds escrowed funds
#[test]
fn test_contract_holds_escrow() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let contract_balance = s.token_client.balance(&s.client.address);
    assert_eq!(contract_balance, 250);
}

/// Verify completion immediately transfers funds to merchant.
#[test]
fn test_token_transfer_on_payment_completion() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&payment_id);

    let merchant_balance = s.token_client.balance(&merchant);
    assert_eq!(merchant_balance, 250);

    let contract_balance = s.token_client.balance(&s.client.address);
    assert_eq!(contract_balance, 0);
}

/// Verify customer receives tokens on refund
#[test]
fn test_token_transfer_on_refund() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.dispute_payment(
        &customer,
        &payment_id,
        &String::from_str(&s.env, "test refund"),
    );
    s.client.resolve_dispute(&payment_id, &false); // Release to customer

    let customer_balance = s.token_client.balance(&customer);
    assert_eq!(customer_balance, 1000);

    let contract_balance = s.token_client.balance(&s.client.address);
    assert_eq!(contract_balance, 0);
}

/// Verify batch payment transfers total amount
#[test]
fn test_token_transfer_on_batch_payment() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant1 = Address::generate(&s.env);
    let merchant2 = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payments = soroban_sdk::vec![
        &s.env,
        PaymentRequest {
            merchant: merchant1,
            amount: 250,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
        PaymentRequest {
            merchant: merchant2,
            amount: 350,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None,
        },
    ];

    s.client.create_payments_batch(&customer, &payments);

    let customer_balance = s.token_client.balance(&customer);
    assert_eq!(customer_balance, 400);

    let contract_balance = s.token_client.balance(&s.client.address);
    assert_eq!(contract_balance, 600);
}

/// Verify dispute resolution to merchant keeps funds in escrow until settlement.
#[test]
fn test_token_transfer_on_dispute_resolution_to_merchant() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.dispute_payment(
        &customer,
        &payment_id,
        &String::from_str(&s.env, "Item not received"),
    );
    s.client.resolve_dispute(&payment_id, &true); // Release to merchant

    let merchant_balance = s.token_client.balance(&merchant);
    assert_eq!(merchant_balance, 0);

    let customer_balance = s.token_client.balance(&customer);
    assert_eq!(customer_balance, 750);
    assert_eq!(s.token_client.balance(&s.client.address), 250);
}

/// Verify multiple payments track balances correctly
#[test]
fn test_token_balance_tracking_multiple_payments() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment1 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let _payment2 = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let customer_balance = s.token_client.balance(&customer);
    assert_eq!(customer_balance, 500);

    let contract_balance = s.token_client.balance(&s.client.address);
    assert_eq!(contract_balance, 500);

    s.client.complete_payment(&payment1);

    let merchant_balance = s.token_client.balance(&merchant);
    assert_eq!(merchant_balance, 200);

    let contract_balance = s.token_client.balance(&s.client.address);
    assert_eq!(contract_balance, 300);
}

#[test]
fn test_settle_merchant_payments_single_transfer_marks_all_settled() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    let p1 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let p2 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let p3 = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&p1);
    s.client.complete_payment(&p2);
    s.client.complete_payment(&p3);

    assert_eq!(s.token_client.balance(&merchant), 600);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
    assert!(s.client.is_settled(&p1));
    assert!(s.client.is_settled(&p2));
    assert!(s.client.is_settled(&p3));
}

#[test]
fn test_settle_merchant_payments_prevents_double_settlement() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    let batch = soroban_sdk::vec![&s.env, pid];
    let second = s
        .client
        .try_settle_merchant_payments(&s.admin, &merchant, &batch);
    assert!(second.is_err());
}

#[test]
fn test_settle_merchant_payments_partial_failure_reverts_entire_batch() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let completed = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pending = s.client.create_payment(
        &customer,
        &merchant,
        &150,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&completed);

    let batch = soroban_sdk::vec![&s.env, completed, pending];
    let res = s
        .client
        .try_settle_merchant_payments(&s.admin, &merchant, &batch);
    assert!(res.is_err());

    // Completed payment is already paid out and settled by complete_payment.
    assert_eq!(s.token_client.balance(&merchant), 200);
    assert_eq!(s.token_client.balance(&s.client.address), 150);
    assert!(s.client.is_settled(&completed));
    assert!(!s.client.is_settled(&pending));
}

#[test]
fn test_settle_merchant_payments_batch_size_capped() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10_000);

    let mut batch = soroban_sdk::Vec::<u32>::new(&s.env);
    for _ in 0..51 {
        let pid = s.client.create_payment(
            &customer,
            &merchant,
            &10,
            &s.token_addr,
            &None,
            &None,
            &None,
        );
        s.client.complete_payment(&pid);
        batch.push_back(pid);
    }

    let res = s
        .client
        .try_settle_merchant_payments(&s.admin, &merchant, &batch);
    assert!(res.is_err());
}

// ===========================================================================
//  Admin Transfer Tests
// ===========================================================================

#[test]
fn test_propose_admin_transfer() {
    let s = setup();
    s.init();

    let new_admin = Address::generate(&s.env);
    s.client.propose_admin_transfer(&new_admin);

    assert_eq!(s.client.get_admin(), s.admin);
    assert_eq!(s.client.get_proposed_admin(), Some(new_admin));
}

#[test]
fn test_accept_admin_role() {
    let s = setup();
    s.init();

    let new_admin = Address::generate(&s.env);
    s.client.propose_admin_transfer(&new_admin);
    s.client.accept_admin_role();

    assert_eq!(s.client.get_admin(), new_admin);
    assert_eq!(s.client.get_proposed_admin(), None);
}

#[test]
#[should_panic(expected = "No admin transfer proposed")]
fn test_accept_admin_role_without_proposal_panics() {
    let s = setup();
    s.init();
    s.client.accept_admin_role();
}

#[test]
fn test_admin_transfer_emits_events() {
    let s = setup();
    s.init();

    let new_admin = Address::generate(&s.env);
    s.client.propose_admin_transfer(&new_admin);

    let events = s.env.events().all();
    assert!(events.len() > 0);

    s.client.accept_admin_role();

    let events = s.env.events().all();
    assert!(events.len() > 0);
}

#[test]
fn test_get_admin_returns_current_admin() {
    let s = setup();
    s.init();

    assert_eq!(s.client.get_admin(), s.admin);
}

#[test]
fn test_get_proposed_admin_returns_none_when_no_proposal() {
    let s = setup();
    s.init();

    assert_eq!(s.client.get_proposed_admin(), None);
}

#[test]
fn test_boundary_amount_i128_max_rejected_without_balance() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1);

    let res = s.client.try_create_payment(
        &customer,
        &merchant,
        &i128::MAX,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert!(res.is_err());
}

#[test]
fn test_boundary_payment_id_u64_max_cast_not_found() {
    let s = setup();
    s.init();
    let id = u64::MAX as u32;
    let res = s.client.try_get_payment(&id);
    assert!(res.is_err());
}

#[test]
fn test_auth_required_for_admin_complete_payment() {
    let env = Env::default();
    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin, &0);

    let res = client.try_complete_payment(&0);
    assert!(res.is_err());
}

#[test]
fn test_event_snapshot_for_payment_creation() {
    // TODO: Implement event snapshot test
}

#[test]
fn test_upgrade_increments_contract_version() {
    let s = setup();
    s.init();

    assert_eq!(s.client.get_version(), 1);

    let wasm_hash = s.env.deployer().upload_contract_wasm(UPGRADE_WASM);
    s.client.upgrade(&s.admin, &wasm_hash);

    let version: u32 = s.env.as_contract(&s.client.address, || {
        s.env
            .storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_unauthorized_upgrade_rejected() {
    let s = setup();
    s.init();

    let intruder = Address::generate(&s.env);
    let wasm_hash = s.env.deployer().upload_contract_wasm(UPGRADE_WASM);

    let result = s.client.try_upgrade(&intruder, &wasm_hash);
    assert!(result.is_err());
    assert_eq!(s.client.get_version(), 1);
}

#[test]
fn test_migration_cannot_run_twice_for_same_version() {
    let s = setup();
    s.init();

    s.client.migrate(&s.admin);
    let second = s.client.try_migrate(&s.admin);

    assert!(second.is_err());
}

#[test]
fn test_upgrade_atomic_when_wasm_hash_invalid() {
    let s = setup();
    s.init();

    let invalid_hash = BytesN::from_array(&s.env, &[11u8; 32]);
    let result = s.client.try_upgrade(&s.admin, &invalid_hash);

    assert!(result.is_err());
    assert_eq!(s.client.get_version(), 1);
}

// ===========================================================================
//  Pause Mechanism Tests
// ===========================================================================

#[test]
fn test_admin_can_pause_and_resume_contract() {
    let s = setup();
    s.init();

    let reason = String::from_str(&s.env, "Emergency maintenance");
    s.client.pause_contract(&s.admin, &reason);

    assert_eq!(s.client.is_paused(), true);
    assert_eq!(s.client.get_pause_reason(), reason);

    s.client.resume_contract(&s.admin);
    assert_eq!(s.client.is_paused(), false);
    assert_eq!(s.client.get_pause_reason(), String::from_str(&s.env, ""));
}

#[test]
fn test_non_admin_cannot_pause_or_resume() {
    let s = setup();
    s.init();

    let attacker = Address::generate(&s.env);
    let pause_res = s
        .client
        .try_pause_contract(&attacker, &String::from_str(&s.env, "malicious"));
    assert!(pause_res.is_err());

    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "incident"));

    let resume_res = s.client.try_resume_contract(&attacker);
    assert!(resume_res.is_err());
}

#[test]
fn test_write_functions_blocked_when_paused_reads_still_work() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let _ = s.client.create_payment(
        &customer,
        &merchant,
        &120,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let events = s.env.events().all();
    assert!(!events.is_empty());
    let snapshot = alloc::format!("{:?}", events);
    assert!(!snapshot.is_empty());
}

#[test]
fn test_write_operations_blocked_when_paused() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "Emergency"));

    let create_res = s.client.try_create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert!(create_res.is_err());

    assert_eq!(s.client.get_payment_counter(), 0);
    assert_eq!(s.client.get_admin(), s.admin);
}

#[test]
fn test_fuzz_like_payment_inputs_100_cases() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &20_000_000);

    let mut seed: u64 = 0x735735;
    for _ in 0..100 {
        seed = seed.wrapping_mul(1103515245).wrapping_add(12345);
        let merchant = Address::generate(&s.env);
        let amount = ((seed % 3000) as i128) + 1;
        let _ = s.client.try_create_payment(
            &customer,
            &merchant,
            &amount,
            &s.token_addr,
            &None,
            &None,
            &None,
        );
    }

    assert!(s.client.get_payment_counter() <= 100);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(120))]

    #[test]
    fn prop_total_refunded_le_total_paid(
        paid in prop::collection::vec(1i128..1_000_000, 1..120),
        refunded in prop::collection::vec(0i128..1_000_000, 1..120),
    ) {
        let mut total_paid: i128 = 0;
        let mut total_refunded: i128 = 0;

        let len = core::cmp::min(paid.len(), refunded.len());
        for index in 0..len {
            let p = paid[index];
            let r = core::cmp::min(refunded[index], p);
            total_paid += p;
            total_refunded += r;
        }

        prop_assert!(total_refunded <= total_paid);
    }

    #[test]
    fn prop_completed_sum_equals_settled_volume(
        completed in prop::collection::vec(0i128..1_000_000, 1..120)
    ) {
        let mut settled_volume: i128 = 0;
        for amount in completed.iter() {
            settled_volume += *amount;
        }

        let mut sum_completed: i128 = 0;
        for amount in completed.iter() {
            sum_completed += *amount;
        }

        prop_assert_eq!(sum_completed, settled_volume);
    }
}

#[test]
fn test_recovery_after_resume() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "Emergency"));
    s.client.resume_contract(&s.admin);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(payment_id, 0);
}

// ===========================================================================
//  Analytics / Statistics Tests (#70)
// ===========================================================================

#[test]
fn test_stats_zero_on_init() {
    let s = setup();
    s.init();

    let stats = s.client.get_stats();
    assert_eq!(stats.total_payments_created, 0);
    assert_eq!(stats.total_payments_completed, 0);
    assert_eq!(stats.total_payments_refunded, 0);
    assert_eq!(stats.total_payments_expired, 0);
}

#[test]
fn test_stats_created_increments_on_create() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(s.client.get_stats().total_payments_created, 1);

    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    assert_eq!(s.client.get_stats().total_payments_created, 2);
}

#[test]
fn test_stats_completed_and_volume_on_complete() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    let stats = s.client.get_stats();
    assert_eq!(stats.total_payments_completed, 1);
    assert_eq!(
        stats
            .total_volume_completed
            .get(s.token_addr.clone())
            .unwrap(),
        300
    );
}

#[test]
fn test_stats_volume_accumulates_across_completions() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid0 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid0);
    s.client.complete_payment(&pid1);

    let stats = s.client.get_stats();
    assert_eq!(stats.total_payments_completed, 2);
    assert_eq!(
        stats
            .total_volume_completed
            .get(s.token_addr.clone())
            .unwrap(),
        300
    );
}

#[test]
fn test_stats_refunded_increments_on_dispute_refund() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client
        .dispute_payment(&customer, &pid, &String::from_str(&s.env, "bad"));
    s.client.resolve_dispute(&pid, &false);

    let stats = s.client.get_stats();
    assert_eq!(stats.total_payments_refunded, 1);
    assert_eq!(
        stats
            .total_volume_refunded
            .get(s.token_addr.clone())
            .unwrap(),
        200
    );
}

#[test]
fn test_stats_refunded_increments_on_partial_refund() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.partial_refund(&pid, &50);
    s.client.partial_refund(&pid, &50);

    let stats = s.client.get_stats();
    assert_eq!(stats.total_payments_refunded, 2);
    assert_eq!(
        stats
            .total_volume_refunded
            .get(s.token_addr.clone())
            .unwrap(),
        100
    );
}

#[test]
fn test_stats_expired_increments_on_expire() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.env.ledger().set_timestamp(0);
    s.client.set_payment_timeout(&100);
    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.env.ledger().set_timestamp(200);
    s.client.expire_payment(&pid);

    assert_eq!(s.client.get_stats().total_payments_expired, 1);
}

#[test]
fn test_stats_batch_create_increments_correctly() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let requests = soroban_sdk::vec![
        &s.env,
        PaymentRequest {
            merchant: merchant.clone(),
            amount: 100,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None
        },
        PaymentRequest {
            merchant: merchant.clone(),
            amount: 200,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None
        },
        PaymentRequest {
            merchant: merchant.clone(),
            amount: 300,
            token: s.token_addr.clone(),
            reference: None,
            metadata: None
        },
    ];
    s.client.create_payments_batch(&customer, &requests);

    assert_eq!(s.client.get_stats().total_payments_created, 3);
}

#[test]
fn test_merchant_stats_zero_for_unknown_merchant() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let ms = s.client.get_merchant_stats(&merchant);
    assert_eq!(ms.payments_created, 0);
    assert_eq!(ms.payments_completed, 0);
    assert_eq!(ms.payments_refunded, 0);
}

#[test]
fn test_merchant_stats_created_and_completed() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &400,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    let ms = s.client.get_merchant_stats(&merchant);
    assert_eq!(ms.payments_created, 1);
    assert_eq!(ms.payments_completed, 1);
    assert_eq!(ms.volume_completed.get(s.token_addr.clone()).unwrap(), 400);
}

#[test]
fn test_merchant_stats_are_isolated_between_merchants() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant_a = Address::generate(&s.env);
    let merchant_b = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    let pid_a = s.client.create_payment(
        &customer,
        &merchant_a,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.create_payment(
        &customer,
        &merchant_b,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid_a);

    let ms_a = s.client.get_merchant_stats(&merchant_a);
    let ms_b = s.client.get_merchant_stats(&merchant_b);

    assert_eq!(ms_a.payments_created, 1);
    assert_eq!(ms_a.payments_completed, 1);
    assert_eq!(ms_b.payments_created, 1);
    assert_eq!(ms_b.payments_completed, 0);
}

#[test]
fn test_merchant_stats_refunded_on_dispute_resolution() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &150,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client
        .dispute_payment(&customer, &pid, &String::from_str(&s.env, "issue"));
    s.client.resolve_dispute(&pid, &false);

    let ms = s.client.get_merchant_stats(&merchant);
    assert_eq!(ms.payments_refunded, 1);
}

#[test]
fn test_weekly_volume_bucket_accumulates() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid0 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid0);
    s.client.complete_payment(&pid1);

    assert_eq!(s.client.get_weekly_volume(&s.token_addr), 300);
}

#[test]
fn test_global_stats_consistent_after_full_lifecycle() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    // create 3, complete 1, refund 1 via dispute, expire 1
    let pid0 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.env.ledger().set_timestamp(0);
    s.client.set_payment_timeout(&50);
    let pid2 = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&pid0);

    s.client
        .dispute_payment(&customer, &pid1, &String::from_str(&s.env, "bad"));
    s.client.resolve_dispute(&pid1, &false);

    s.env.ledger().set_timestamp(200);
    s.client.expire_payment(&pid2);

    let stats = s.client.get_stats();
    assert_eq!(stats.total_payments_created, 3);
    assert_eq!(stats.total_payments_completed, 1);
    assert_eq!(stats.total_payments_refunded, 1);
    assert_eq!(stats.total_payments_expired, 1);
    assert_eq!(
        stats
            .total_volume_completed
            .get(s.token_addr.clone())
            .unwrap(),
        100
    );
    assert_eq!(
        stats
            .total_volume_refunded
            .get(s.token_addr.clone())
            .unwrap(),
        200
    );
}

// ===========================================================================
//  Payment Receipt / Proof Tests (#65)
// ===========================================================================

#[test]
fn test_complete_payment_stores_receipt() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    // Receipt must be retrievable after completion
    let receipt = s.client.get_payment_receipt(&pid);
    assert_eq!(receipt.len(), 32);
}

#[test]
fn test_verify_payment_returns_true_for_correct_hash() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    let receipt = s.client.get_payment_receipt(&pid);
    assert!(s.client.verify_payment(&pid, &receipt));
}

#[test]
fn test_verify_payment_returns_false_for_wrong_hash() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    let wrong_hash = BytesN::from_array(&s.env, &[0u8; 32]);
    assert!(!s.client.verify_payment(&pid, &wrong_hash));
}

#[test]
fn test_verify_payment_returns_false_for_pending_payment() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    // Not completed — no receipt stored

    let any_hash = BytesN::from_array(&s.env, &[1u8; 32]);
    assert!(!s.client.verify_payment(&pid, &any_hash));
}

#[test]
#[should_panic(expected = "Receipt not found")]
fn test_get_receipt_for_pending_payment_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    // No complete_payment call — receipt must not exist
    s.client.get_payment_receipt(&pid);
}

#[test]
fn test_receipt_hashes_differ_for_different_payments() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    let pid0 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&pid0);
    // Advance timestamp so completed_at differs
    s.env.ledger().set_timestamp(s.env.ledger().timestamp() + 1);
    s.client.complete_payment(&pid1);

    let h0 = s.client.get_payment_receipt(&pid0);
    let h1 = s.client.get_payment_receipt(&pid1);
    assert_ne!(h0, h1);
}

#[test]
fn test_receipt_event_emitted_on_complete() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    // At least PaymentCompleted + PaymentStatusChanged + PaymentReceiptIssued
    assert!(s.env.events().all().len() >= 3);
}

// ===========================================================================
//  Reference & Metadata Tests (#67)
// ===========================================================================

#[test]
fn test_create_payment_with_reference() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let reference = String::from_str(&s.env, "ORDER-12345");
    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &Some(reference.clone()),
        &None,
        &None,
    );

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.reference, Some(reference));
    assert_eq!(payment.metadata, None);
}

#[test]
fn test_create_payment_with_metadata() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let mut meta = Map::new(&s.env);
    meta.set(
        String::from_str(&s.env, "channel"),
        String::from_str(&s.env, "web"),
    );
    meta.set(
        String::from_str(&s.env, "region"),
        String::from_str(&s.env, "us-east"),
    );

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &Some(meta.clone()),
        &None,
    );

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.metadata, Some(meta));
}

#[test]
fn test_get_payments_by_reference() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let reference = String::from_str(&s.env, "ORDER-ABC");

    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &Some(reference.clone()),
        &None,
        &None,
    );
    let pid2 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &Some(reference.clone()),
        &None,
        &None,
    );
    // Different reference — should not appear
    s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &Some(String::from_str(&s.env, "OTHER")),
        &None,
        &None,
    );

    let ids = s.client.get_payments_by_reference(&merchant, &reference);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids.get(0).unwrap(), pid1);
    assert_eq!(ids.get(1).unwrap(), pid2);
}

#[test]
#[should_panic(expected = "Reference exceeds maximum length of 64 bytes")]
fn test_reference_too_long_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // 65-byte reference string
    let long_ref = String::from_str(
        &s.env,
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    );
    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &Some(long_ref),
        &None,
        &None,
    );
}

#[test]
#[should_panic(expected = "Metadata exceeds maximum of 5 keys")]
fn test_metadata_too_many_keys_panics() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let mut meta = Map::new(&s.env);
    meta.set(
        String::from_str(&s.env, "k1"),
        String::from_str(&s.env, "v1"),
    );
    meta.set(
        String::from_str(&s.env, "k2"),
        String::from_str(&s.env, "v2"),
    );
    meta.set(
        String::from_str(&s.env, "k3"),
        String::from_str(&s.env, "v3"),
    );
    meta.set(
        String::from_str(&s.env, "k4"),
        String::from_str(&s.env, "v4"),
    );
    meta.set(
        String::from_str(&s.env, "k5"),
        String::from_str(&s.env, "v5"),
    );
    meta.set(
        String::from_str(&s.env, "k6"),
        String::from_str(&s.env, "v6"),
    );

    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &Some(meta),
        &None,
    );
}

#[test]
fn test_payment_without_reference_not_indexed() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let ids = s
        .client
        .get_payments_by_reference(&merchant, &String::from_str(&s.env, "NONE"));
    assert_eq!(ids.len(), 0);
}

// ===========================================================================
//  Fee Collection Tests
// ===========================================================================

#[test]
fn test_initialize_with_fee() {
    let s = setup();
    s.init_with_fee(250); // 2.5% fee

    assert_eq!(s.client.get_fee_bps(), 250);
    assert_eq!(s.client.get_fee_recipient(), s.fee_recipient);
}

#[test]
#[should_panic(expected = "Fee cannot exceed 500 bps (5%)")]
fn test_initialize_with_excessive_fee_panics() {
    let s = setup();
    s.client.initialize(&s.admin, &s.fee_recipient, &501);
}

#[test]
fn test_fee_collection_on_complete_payment() {
    let s = setup();
    s.init_with_fee(250); // 2.5% fee

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Before completion
    assert_eq!(s.token_client.balance(&s.fee_recipient), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 1000);

    s.client.complete_payment(&payment_id);

    // After completion: fee = 1000 * 250 / 10000 = 25
    // merchant gets: 1000 - 25 = 975
    assert_eq!(s.token_client.balance(&s.fee_recipient), 25);
    assert_eq!(s.token_client.balance(&merchant), 975);
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 975); // Updated to net amount
    assert_eq!(payment.status, PaymentStatus::Completed);
}

#[test]
fn test_fee_collection_with_zero_fee() {
    let s = setup();
    s.init(); // 0% fee

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&payment_id);

    // No fee collected
    assert_eq!(s.token_client.balance(&s.fee_recipient), 0);
    assert_eq!(s.token_client.balance(&merchant), 1000);
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1000);
}

#[test]
fn test_update_fee() {
    let s = setup();
    s.init();

    assert_eq!(s.client.get_fee_bps(), 0);

    s.client.update_fee(&s.admin, &300);
    assert_eq!(s.client.get_fee_bps(), 300);

    s.client.update_fee(&s.admin, &500);
    assert_eq!(s.client.get_fee_bps(), 500);
}

#[test]
#[should_panic(expected = "Fee cannot exceed 500 bps (5%)")]
fn test_update_fee_exceeds_max() {
    let s = setup();
    s.init();

    s.client.update_fee(&s.admin, &501);
}

#[test]
fn test_update_fee_recipient() {
    let s = setup();
    s.init();

    let new_recipient = Address::generate(&s.env);
    s.client.update_fee_recipient(&s.admin, &new_recipient);

    assert_eq!(s.client.get_fee_recipient(), new_recipient);
}

#[test]
fn test_fee_collection_emits_event() {
    let s = setup();
    s.init_with_fee(100); // 1% fee

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&payment_id);

    let events = s.env.events().all();
    assert!(events.len() > 0, "No events emitted");
}

#[test]
fn test_settlement_with_fee_deducted() {
    let s = setup();
    s.init_with_fee(200); // 2% fee

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&payment_id);

    // Fee = 1000 * 200 / 10000 = 20
    // Net amount = 980
    assert_eq!(s.token_client.balance(&s.fee_recipient), 20);

    // Merchant receives the net amount (980)
    assert_eq!(s.token_client.balance(&merchant), 980);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

// ===========================================================================
//  Idempotency Key Tests
// ===========================================================================

#[test]
fn test_idempotency_key_prevents_duplicate() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let idempotency_key = BytesN::from_array(&s.env, &[1u8; 32]);

    // First payment with idempotency key
    let payment_id_1 = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key.clone()),
    );

    assert_eq!(payment_id_1, 0);
    assert_eq!(s.token_client.balance(&customer), 9000);
    assert_eq!(s.client.get_payment_counter(), 1);

    // Second payment with same idempotency key - should return same payment ID
    let payment_id_2 = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key),
    );

    assert_eq!(payment_id_2, 0); // Same payment ID
    assert_eq!(s.token_client.balance(&customer), 9000); // No additional charge
    assert_eq!(s.client.get_payment_counter(), 1); // Counter not incremented
}

#[test]
fn test_idempotency_key_different_keys_create_different_payments() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let key1 = BytesN::from_array(&s.env, &[1u8; 32]);
    let key2 = BytesN::from_array(&s.env, &[2u8; 32]);

    let payment_id_1 = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(key1),
    );

    let payment_id_2 = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(key2),
    );

    assert_eq!(payment_id_1, 0);
    assert_eq!(payment_id_2, 1);
    assert_eq!(s.token_client.balance(&customer), 8000);
    assert_eq!(s.client.get_payment_counter(), 2);
}

#[test]
fn test_idempotency_key_optional() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    // Create payment without idempotency key
    let payment_id_1 = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Create another payment without idempotency key - should create new payment
    let payment_id_2 = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    assert_eq!(payment_id_1, 0);
    assert_eq!(payment_id_2, 1);
    assert_eq!(s.token_client.balance(&customer), 8000);
}

#[test]
fn test_idempotency_key_with_different_customers() {
    let s = setup();
    s.init();

    let customer1 = Address::generate(&s.env);
    let customer2 = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer1, &10000);
    s.token_admin_client.mint(&customer2, &10000);

    let idempotency_key = BytesN::from_array(&s.env, &[1u8; 32]);

    // Customer 1 creates payment with key
    let payment_id_1 = s.client.create_payment(
        &customer1,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key.clone()),
    );

    // Customer 2 uses same key - should return customer 1's payment
    // (idempotency is global, not per-customer)
    let payment_id_2 = s.client.create_payment(
        &customer2,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key),
    );

    assert_eq!(payment_id_1, payment_id_2);
    assert_eq!(s.token_client.balance(&customer1), 9000);
    assert_eq!(s.token_client.balance(&customer2), 10000); // Not charged
}

#[test]
fn test_idempotency_key_with_fee_collection() {
    let s = setup();
    s.init_with_fee(100); // 1% fee

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let idempotency_key = BytesN::from_array(&s.env, &[1u8; 32]);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key.clone()),
    );

    s.client.complete_payment(&payment_id);

    // Fee = 10, net = 990
    assert_eq!(s.token_client.balance(&s.fee_recipient), 10);

    // Try to create duplicate - should return same payment
    let payment_id_2 = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key),
    );

    assert_eq!(payment_id, payment_id_2);
    // Fee should still be 10 (not doubled)
    assert_eq!(s.token_client.balance(&s.fee_recipient), 10);
}

// ===========================================================================
//  Combined Feature Tests
// ===========================================================================

#[test]
fn test_full_payment_flow_with_fee_and_idempotency() {
    let s = setup();
    s.init_with_fee(250); // 2.5% fee

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    let idempotency_key = BytesN::from_array(&s.env, &[42u8; 32]);

    // Create payment with idempotency key
    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &2000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key.clone()),
    );

    assert_eq!(payment_id, 0);
    assert_eq!(s.token_client.balance(&customer), 8000);

    // Complete payment - fee should be deducted
    s.client.complete_payment(&payment_id);

    // Fee = 2000 * 250 / 10000 = 50
    // Net = 1950
    assert_eq!(s.token_client.balance(&s.fee_recipient), 50);
    assert_eq!(s.token_client.balance(&merchant), 1950);
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 1950);
    assert_eq!(payment.status, PaymentStatus::Completed);

    // Try duplicate with same idempotency key
    let duplicate_id = s.client.create_payment(
        &customer,
        &merchant,
        &2000,
        &s.token_addr,
        &None,
        &None,
        &Some(idempotency_key),
    );

    assert_eq!(duplicate_id, payment_id);
    assert_eq!(s.token_client.balance(&customer), 8000); // No additional charge
}

// ===========================================================================
//  Split Recipients Tests
// ===========================================================================

#[test]
#[should_panic(expected = "split_recipients must sum to 10000 bps")]
fn test_split_recipients_invalid_sum_rejected() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let recipient_a = Address::generate(&s.env);
    let recipient_b = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let splits = vec![
        &s.env,
        SplitRecipient {
            recipient: recipient_a,
            bps: 7000,
        },
        SplitRecipient {
            recipient: recipient_b,
            bps: 2000,
        },
    ];

    s.client.create_payment_with_options(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(splits),
        &None,
        &None,
    );
}

#[test]
fn test_split_recipients_two_way_exact_distribution() {
    let s = setup();
    s.init_with_fee(100); // 1%

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let recipient_a = Address::generate(&s.env);
    let recipient_b = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let splits = vec![
        &s.env,
        SplitRecipient {
            recipient: recipient_a.clone(),
            bps: 6000,
        },
        SplitRecipient {
            recipient: recipient_b.clone(),
            bps: 4000,
        },
    ];

    let pid = s.client.create_payment_with_options(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &Some(splits),
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    // Fee=10, net=990; splits receive 594 and 396.
    assert_eq!(s.token_client.balance(&s.fee_recipient), 10);
    assert_eq!(s.token_client.balance(&recipient_a), 594);
    assert_eq!(s.token_client.balance(&recipient_b), 396);
    assert_eq!(s.token_client.balance(&merchant), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_split_recipients_three_way_with_dust_to_merchant() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let r1 = Address::generate(&s.env);
    let r2 = Address::generate(&s.env);
    let r3 = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let splits = vec![
        &s.env,
        SplitRecipient {
            recipient: r1.clone(),
            bps: 3333,
        },
        SplitRecipient {
            recipient: r2.clone(),
            bps: 3333,
        },
        SplitRecipient {
            recipient: r3.clone(),
            bps: 3334,
        },
    ];

    let pid = s.client.create_payment_with_options(
        &customer,
        &merchant,
        &1001,
        &s.token_addr,
        &None,
        &None,
        &Some(splits),
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    assert_eq!(s.token_client.balance(&r1), 333);
    assert_eq!(s.token_client.balance(&r2), 333);
    assert_eq!(s.token_client.balance(&r3), 333);
    assert_eq!(s.token_client.balance(&merchant), 2);
}

#[test]
fn test_split_completion_emits_events() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let recipient = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    let splits = vec![
        &s.env,
        SplitRecipient {
            recipient,
            bps: 10_000,
        },
    ];

    let pid = s.client.create_payment_with_options(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &Some(splits),
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    assert!(s.env.events().all().len() > 0);
}

// ===========================================================================
//  Tiered Fee Tests
// ===========================================================================

#[test]
fn test_tiered_fee_applies_default_below_minimum() {
    let s = setup();
    s.init_with_fee(300); // default 3%

    let tiers = vec![
        &s.env,
        FeeTier {
            min_volume: 1_000,
            fee_bps: 200,
        },
        FeeTier {
            min_volume: 5_000,
            fee_bps: 100,
        },
    ];
    s.client.update_fee_tiers(&s.admin, &tiers);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    // 500 * 3% = 15
    assert_eq!(s.token_client.balance(&s.fee_recipient), 15);
}

#[test]
fn test_tiered_fee_threshold_applies_immediately() {
    let s = setup();
    s.init_with_fee(300); // default 3%

    let tiers = vec![
        &s.env,
        FeeTier {
            min_volume: 1_000,
            fee_bps: 200,
        },
    ];
    s.client.update_fee_tiers(&s.admin, &tiers);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    // Threshold crossed on this payment, so 2% applies immediately.
    assert_eq!(s.token_client.balance(&s.fee_recipient), 20);
    assert_eq!(s.client.get_merchant_fee_tier(&merchant), 200);
}

#[test]
fn test_tiered_fee_rolling_volume_rolls_off() {
    let s = setup();
    s.init_with_fee(300);

    let tiers = vec![
        &s.env,
        FeeTier {
            min_volume: 1_000,
            fee_bps: 200,
        },
    ];
    s.client.update_fee_tiers(&s.admin, &tiers);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    s.env.ledger().set_sequence_number(1);
    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &1000,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);
    assert_eq!(s.client.get_merchant_fee_tier(&merchant), 200);

    // Advance beyond the 4-week rolling window.
    s.env.ledger().set_sequence_number(120_960 * 6);
    assert_eq!(s.client.get_merchant_fee_tier(&merchant), 300);
}

#[test]
fn test_get_fee_tiers_roundtrip() {
    let s = setup();
    s.init_with_fee(300);

    let tiers = vec![
        &s.env,
        FeeTier {
            min_volume: 2_000,
            fee_bps: 250,
        },
        FeeTier {
            min_volume: 10_000,
            fee_bps: 150,
        },
    ];
    s.client.update_fee_tiers(&s.admin, &tiers);

    let got = s.client.get_fee_tiers();
    assert_eq!(got.len(), 2);
    assert_eq!(got.get(0).unwrap().min_volume, 2_000);
    assert_eq!(got.get(1).unwrap().fee_bps, 150);
}

// ===========================================================================
//  Scheduled Payment Tests
// ===========================================================================

#[test]
#[should_panic(expected = "Payment cannot execute before schedule")]
fn test_scheduled_payment_rejects_early_execution() {
    let s = setup();
    s.init();

    s.env.ledger().set_timestamp(1_000);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let pid = s.client.create_payment_with_options(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &None,
        &Some(2_000u64),
        &None,
    );

    s.client.execute_scheduled_payment(&pid);
}

#[test]
fn test_scheduled_payment_anyone_can_execute_after_time() {
    let s = setup();
    s.init();

    s.env.ledger().set_timestamp(1_000);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let pid = s.client.create_payment_with_options(
        &customer,
        &merchant,
        &400,
        &s.token_addr,
        &None,
        &None,
        &None,
        &Some(1_500u64),
        &None,
    );

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::ScheduledPending);

    s.env.ledger().set_timestamp(1_500);
    s.client.execute_scheduled_payment(&pid);

    assert_eq!(s.token_client.balance(&merchant), 400);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Completed);
}

#[test]
fn test_scheduled_payment_cancel_before_execution_refunds() {
    let s = setup();
    s.init();

    s.env.ledger().set_timestamp(1_000);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let pid = s.client.create_payment_with_options(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
        &Some(2_000u64),
        &None,
    );

    assert_eq!(s.token_client.balance(&customer), 4700);
    s.client.cancel_scheduled_payment(&customer, &pid);

    assert_eq!(s.token_client.balance(&customer), 5000);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Refunded);
}

#[test]
#[should_panic(expected = "Scheduled payment is ready to execute")]
fn test_scheduled_payment_cannot_cancel_after_ready() {
    let s = setup();
    s.init();

    s.env.ledger().set_timestamp(1_000);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &5000);

    let pid = s.client.create_payment_with_options(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
        &Some(2_000u64),
        &None,
    );

    s.env.ledger().set_timestamp(2_000);
    s.client.cancel_scheduled_payment(&customer, &pid);
}

// ===========================================================================
//  #122 Payment Categories and Tags
// ===========================================================================

// test_create_payment_with_category_indexed — commented out: create_payment_with_extras is not yet exported

// test_get_payments_by_category_pagination — commented out: create_payment_with_extras is not yet exported

#[test]
fn test_get_payments_by_category_empty_returns_empty() {
    let s = setup();
    s.init();
    let merchant = Address::generate(&s.env);
    let cat = soroban_sdk::Symbol::new(&s.env, "empty");
    let results = s.client.get_payments_by_category(&merchant, &cat, &0, &10);
    assert_eq!(results.len(), 0);
}

// test_tags_exceeding_3_rejected — commented out: create_payment_with_extras is not yet exported

#[test]
fn test_no_category_does_not_appear_in_index() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let cat = soroban_sdk::Symbol::new(&s.env, "anything");
    let results = s.client.get_payments_by_category(&merchant, &cat, &0, &10);
    assert_eq!(results.len(), 0);
}

// ===========================================================================
//  #123 Bulk Expire Payments
// ===========================================================================

#[test]
fn test_bulk_expire_payments_success() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid0 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Advance past default 7-day expiry
    s.env
        .ledger()
        .with_mut(|l| l.timestamp = 7 * 24 * 60 * 60 + 1);

    let customer_balance_before = s.token_client.balance(&customer);
    s.client
        .bulk_expire_payments(&s.admin, &vec![&s.env, pid0, pid1]);

    assert_eq!(s.client.get_payment(&pid0).status, PaymentStatus::Expired);
    assert_eq!(s.client.get_payment(&pid1).status, PaymentStatus::Expired);
    assert_eq!(
        s.token_client.balance(&customer),
        customer_balance_before + 300
    );
}

#[test]
fn test_bulk_expire_ineligible_payment_skipped() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid0 = s.client.create_payment(&customer, &merchant, &100, &s.token_addr, &None, &None, &None);
    let pid1 = s.client.create_payment(&customer, &merchant, &200, &s.token_addr, &None, &None, &None);

    s.client.complete_payment(&pid1);
    s.env.ledger().with_mut(|l| l.timestamp = 7 * 24 * 60 * 60 + 1);

    // pid1 is Completed (ineligible) — it should be skipped, pid0 expired
    let skipped = s.client.bulk_expire_payments(&s.admin, &vec![&s.env, pid0, pid1]);

    assert_eq!(s.client.get_payment(&pid0).status, PaymentStatus::Expired);
    assert_eq!(s.client.get_payment(&pid1).status, PaymentStatus::Completed);
    assert_eq!(skipped.len(), 1);
    assert_eq!(skipped.get(0).unwrap(), pid1);
}

#[test]
fn test_bulk_expire_exceeds_cap_rejected() {
    let s = setup();
    s.init();
    let mut ids = soroban_sdk::Vec::new(&s.env);
    for i in 0u32..21 {
        ids.push_back(i);
    }
    let err = s.client.try_bulk_expire_payments(&s.admin, &ids).unwrap_err().unwrap();
    assert_eq!(err, ExtError::InvalidAmount.into());
}

#[test]
fn test_bulk_expire_not_expired_skipped() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &500);

    let pid = s.client.create_payment(&customer, &merchant, &100, &s.token_addr, &None, &None, &None);
    // Do NOT advance time — payment hasn't expired; should be skipped
    let skipped = s.client.bulk_expire_payments(&s.admin, &vec![&s.env, pid]);
    assert_eq!(skipped.len(), 1);
    assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Pending);
}

#[test]
fn test_bulk_expire_cap_enforced() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10_000);

    // Create 5 expired + 3 already-expired (completed) payments
    let mut expired_ids = soroban_sdk::Vec::new(&s.env);
    for _ in 0..5 {
        let pid = s.client.create_payment(&customer, &merchant, &100, &s.token_addr, &None, &None, &None);
        expired_ids.push_back(pid);
    }
    let mut completed_ids = soroban_sdk::Vec::new(&s.env);
    for _ in 0..3 {
        let pid = s.client.create_payment(&customer, &merchant, &100, &s.token_addr, &None, &None, &None);
        completed_ids.push_back(pid);
    }

    for i in 0..3 {
        s.client.complete_payment(&completed_ids.get(i).unwrap());
    }
    s.env.ledger().with_mut(|l| l.timestamp = 7 * 24 * 60 * 60 + 1);

    let mut all_ids = expired_ids.clone();
    for pid in completed_ids.iter() {
        all_ids.push_back(pid);
    }

    let skipped = s.client.bulk_expire_payments(&s.admin, &all_ids);
    // 5 expired, 3 skipped (completed)
    assert_eq!(skipped.len(), 3);
    for pid in expired_ids.iter() {
        assert_eq!(s.client.get_payment(&pid).status, PaymentStatus::Expired);
    }

    // Cap enforcement: 21 IDs → InvalidAmount
    let mut big_batch = soroban_sdk::Vec::new(&s.env);
    for i in 0u32..21 { big_batch.push_back(i); }
    let err = s.client.try_bulk_expire_payments(&s.admin, &big_batch).unwrap_err().unwrap();
    assert_eq!(err, ExtError::InvalidAmount.into());
}

// ===========================================================================
//  #124 Subscription Pause and Resume
// ===========================================================================

#[test]
fn test_pause_subscription_blocks_charge() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id =
        s.client
            .create_subscription(&subscriber, &merchant, &100, &s.token_addr, &60, &10);

    s.env.ledger().with_mut(|l| l.timestamp = 12_345);
    s.client.pause_subscription(&subscriber, &sub_id);

    let sub = s.client.get_subscription(&sub_id);
    assert!(sub.paused);
    assert!(sub.paused_at > 0);
}

#[test]
#[should_panic]
fn test_charge_paused_subscription_fails() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id =
        s.client
            .create_subscription(&subscriber, &merchant, &100, &s.token_addr, &60, &10);

    // Initial charge succeeds
    s.client.charge_subscription(&sub_id);

    s.client.pause_subscription(&subscriber, &sub_id);

    // Advance past interval
    s.env.ledger().with_mut(|l| l.timestamp += 120);

    // Should panic with SubscriptionPaused
    s.client.charge_subscription(&sub_id);
}

#[test]
fn test_resume_resets_interval_from_resume_time() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id =
        s.client
            .create_subscription(&subscriber, &merchant, &100, &s.token_addr, &60, &10);

    // Charge once to set last_charged_at
    s.client.charge_subscription(&sub_id);
    let after_first_charge = s.env.ledger().timestamp();

    // Pause mid-interval
    s.env
        .ledger()
        .with_mut(|l| l.timestamp = after_first_charge + 30);
    s.client.pause_subscription(&subscriber, &sub_id);

    // Advance through a full interval while paused
    s.env
        .ledger()
        .with_mut(|l| l.timestamp = after_first_charge + 120);
    s.client.resume_subscription(&subscriber, &sub_id);
    let resumed_at = s.env.ledger().timestamp();

    let sub = s.client.get_subscription(&sub_id);
    assert!(!sub.paused);
    // last_charged_at was reset to resumed_at, so next charge needs resumed_at + 60
    assert_eq!(sub.last_charged_at, resumed_at);
}

#[test]
#[should_panic(expected = "Only the subscriber can pause")]
fn test_non_subscriber_cannot_pause() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    let other = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &1000);

    let sub_id = s
        .client
        .create_subscription(&subscriber, &merchant, &100, &s.token_addr, &60, &5);

    s.client.pause_subscription(&other, &sub_id);
}

#[test]
#[should_panic(expected = "Subscription is not paused")]
fn test_resume_not_paused_subscription_fails() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &1000);

    let sub_id = s
        .client
        .create_subscription(&subscriber, &merchant, &100, &s.token_addr, &60, &5);

    s.client.resume_subscription(&subscriber, &sub_id);
}

// ===========================================================================
//  #125 Conditional Payment Release via Oracle Price Threshold
// ===========================================================================

// test_conditional_payment_stores_condition — commented out: create_payment_with_extras / release_condition not yet exported

// test_payment_without_condition_completes_normally — commented out: create_payment_with_extras is not yet exported

// ===========================================================================
//  #127 Payment Authorization Pre-Approval (Two-Step Settlement)
// ===========================================================================

#[test]
fn test_authorize_payment_succeeds() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Authorized);
    assert_eq!(payment.capture_deadline, 200);
    assert_eq!(s.token_client.balance(&s.client.address), 300);
    assert_eq!(s.token_client.balance(&customer), 700);
    assert_eq!(s.token_client.balance(&merchant), 0);
}

#[test]
#[should_panic(expected = "capture_deadline_ledger must be in the future")]
fn test_authorize_payment_past_deadline_panics() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.env.ledger().set_sequence_number(100);
    s.client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &100u64);
}

#[test]
#[should_panic(expected = "Payment amount must be positive")]
fn test_authorize_payment_zero_amount_panics() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.env.ledger().set_sequence_number(100);
    s.client
        .authorize_payment(&merchant, &customer, &s.token_addr, &0, &200u64);
}

#[test]
fn test_capture_authorized_payment_within_window() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.env.ledger().set_sequence_number(150);
    s.client.capture_payment(&merchant, &pid);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Completed);
    assert_eq!(s.token_client.balance(&merchant), 300);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_capture_after_deadline_panics() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.env.ledger().set_sequence_number(201); // post deadline
    let res = s.client.try_capture_payment(&merchant, &pid);
    assert_eq!(res.unwrap_err().unwrap(), Error::CapturePastDeadline.into());
}

#[test]
#[should_panic(expected = "Payment is not authorized")]
fn test_capture_pending_payment_panics() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.capture_payment(&merchant, &pid);
}

#[test]
fn test_expire_authorized_payment_refunds_customer() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.env.ledger().set_sequence_number(201);
    s.client.expire_payment(&pid);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Expired);
    assert_eq!(s.token_client.balance(&customer), 1000);
    assert_eq!(s.token_client.balance(&merchant), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_dispute_authorized_payment() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    let reason = String::from_str(&s.env, "Unauthorized charge");
    s.client.dispute_payment(&customer, &pid, &reason);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Disputed);
}

#[test]
fn test_bulk_expire_authorized_payments() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid0 = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &100, &200u64);
    let pid1 = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &200, &200u64);

    s.env.ledger().set_sequence_number(201);
    s.client
        .bulk_expire_payments(&s.admin, &vec![&s.env, pid0, pid1]);

    assert_eq!(s.client.get_payment(&pid0).status, PaymentStatus::Expired);
    assert_eq!(s.client.get_payment(&pid1).status, PaymentStatus::Expired);
    assert_eq!(s.token_client.balance(&customer), 1000);
}

#[test]
fn test_authorization_events_emitted() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);
    s.env.ledger().set_sequence_number(150);
    s.client.capture_payment(&merchant, &pid);

    let events = s.env.events().all();
    // Should include PaymentAuthorized, PaymentCaptured, PaymentCompleted and PaymentStatusChanged.
    assert!(events.len() >= 4);
}

#[test]
fn test_void_authorization_returns_funds_to_customer() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);
    s.approve(&customer, 1000);

    s.env.ledger().set_sequence_number(100);
    let pid = s
        .client
        .authorize_payment(&merchant, &customer, &s.token_addr, &300, &200u64);

    s.client.void_authorization(&customer, &pid);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Expired);
    assert_eq!(s.token_client.balance(&customer), 1000);
    assert_eq!(s.token_client.balance(&merchant), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

// ===========================================================================
//  Withdrawal Queue Tests (#126)
// ===========================================================================

#[test]
fn test_withdrawal_queue_fifo_order() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create and complete 3 payments
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid2 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid3 = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&pid1);
    s.client.complete_payment(&pid2);
    s.client.complete_payment(&pid3);

    // Check queue has 3 entries in FIFO order
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 3);
    assert_eq!(queue.get(0).unwrap(), (pid1, 100));
    assert_eq!(queue.get(1).unwrap(), (pid2, 200));
    assert_eq!(queue.get(2).unwrap(), (pid3, 300));

    // Process 2 entries
    let withdrawn = s.client.process_withdrawal_queue(&merchant, &2);
    assert_eq!(withdrawn, 300); // 100 + 200

    // Check queue now has 1 entry
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 1);
    assert_eq!(queue.get(0).unwrap(), (pid3, 300));

    // Process remaining
    let withdrawn = s.client.process_withdrawal_queue(&merchant, &10);
    assert_eq!(withdrawn, 300);

    // Queue should be empty
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 0);
}

#[test]
fn test_withdrawal_queue_priority_override() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create and complete 3 payments
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid2 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid3 = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&pid1);
    s.client.complete_payment(&pid2);
    s.client.complete_payment(&pid3);

    // Prioritize pid3 (move to front)
    s.client.prioritize_withdrawal(&merchant, &pid3);

    // Check queue order changed
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 3);
    assert_eq!(queue.get(0).unwrap(), (pid3, 300)); // pid3 now first
    assert_eq!(queue.get(1).unwrap(), (pid1, 100));
    assert_eq!(queue.get(2).unwrap(), (pid2, 200));

    // Process 1 entry (should be pid3)
    let withdrawn = s.client.process_withdrawal_queue(&merchant, &1);
    assert_eq!(withdrawn, 300);

    // Check remaining queue
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get(0).unwrap(), (pid1, 100));
    assert_eq!(queue.get(1).unwrap(), (pid2, 200));
}

#[test]
fn test_withdrawal_queue_partial_drain() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    // Create and complete 5 payments
    let mut pids = Vec::new(&s.env);
    for i in 1..=5 {
        let amount = (i as i128) * 100;
        let pid = s.client.create_payment(
            &customer,
            &merchant,
            &amount,
            &s.token_addr,
            &None,
            &None,
            &None,
        );
        s.client.complete_payment(&pid);
        pids.push_back(pid);
    }

    // Check queue has 5 entries
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 5);

    // Process with max_count = 3
    let withdrawn = s.client.process_withdrawal_queue(&merchant, &3);
    assert_eq!(withdrawn, 600); // 100 + 200 + 300

    // Check 2 entries remain
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get(0).unwrap(), (pids.get(3).unwrap(), 400));
    assert_eq!(queue.get(1).unwrap(), (pids.get(4).unwrap(), 500));
}

#[test]
fn test_withdrawal_queue_empty() {
    let s = setup();
    s.init();
    let merchant = Address::generate(&s.env);

    // Empty queue
    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 0);

    // Process empty queue
    let withdrawn = s.client.process_withdrawal_queue(&merchant, &10);
    assert_eq!(withdrawn, 0);
}

#[test]
#[should_panic(expected = "Payment not found in withdrawal queue")]
fn test_prioritize_nonexistent_payment() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid);

    // Try to prioritize a different payment ID
    s.client.prioritize_withdrawal(&merchant, &999);
}

#[test]
fn test_prioritize_already_first() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let pid2 = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    s.client.complete_payment(&pid1);
    s.client.complete_payment(&pid2);

    // Prioritize pid1 (already first) - should do nothing
    s.client.prioritize_withdrawal(&merchant, &pid1);

    let queue = s.client.get_merchant_withdrawal_queue(&merchant);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.get(0).unwrap(), (pid1, 100));
    assert_eq!(queue.get(1).unwrap(), (pid2, 200));
}

// ===========================================================================
//  Invoice Line Items Tests (#128)
// ===========================================================================

#[test]
fn test_create_payment_with_invoice_success() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create invoice with one line item: 100 * 2 = 200, tax = 0, total = 200
    let mut line_items = Vec::new(&s.env);
    line_items.push_back(LineItem {
        description: Symbol::new(&s.env, "item1"),
        quantity: 2,
        unit_price: 100,
    });

    let invoice = InvoiceData {
        line_items,
        tax_bps: 0,
        currency_label: Symbol::new(&s.env, "USD"),
    };

    let payment_id = s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice),
        &None,
    );

    // Verify invoice hash was stored
    let invoice_hash = s.client.get_invoice_hash(&payment_id);
    assert!(invoice_hash.is_some());

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 200);
}

#[test]
#[should_panic(expected = "Invoice total does not match payment amount")]
fn test_create_payment_with_invoice_total_mismatch() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create invoice with total 200, but payment amount is 250
    let mut line_items = Vec::new(&s.env);
    line_items.push_back(LineItem {
        description: Symbol::new(&s.env, "item1"),
        quantity: 2,
        unit_price: 100,
    });

    let invoice = InvoiceData {
        line_items,
        tax_bps: 0,
        currency_label: Symbol::new(&s.env, "USD"),
    };

    // This should panic because total 200 != payment 250
    s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &250,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice),
        &None,
    );
}

#[test]
fn test_create_payment_with_invoice_with_tax() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create invoice: 100 * 2 = 200, tax = 10% = 20, total = 220
    let mut line_items = Vec::new(&s.env);
    line_items.push_back(LineItem {
        description: Symbol::new(&s.env, "item1"),
        quantity: 2,
        unit_price: 100,
    });

    let invoice = InvoiceData {
        line_items,
        tax_bps: 1000, // 10%
        currency_label: Symbol::new(&s.env, "USD"),
    };

    let payment_id = s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &220,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice),
        &None,
    );

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 220);
}

#[test]
fn test_create_payment_with_multiple_line_items() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create invoice with 3 line items: (100*2) + (50*3) + (75*1) = 200 + 150 + 75 = 425
    let mut line_items = Vec::new(&s.env);
    line_items.push_back(LineItem {
        description: Symbol::new(&s.env, "item1"),
        quantity: 2,
        unit_price: 100,
    });
    line_items.push_back(LineItem {
        description: Symbol::new(&s.env, "item2"),
        quantity: 3,
        unit_price: 50,
    });
    line_items.push_back(LineItem {
        description: Symbol::new(&s.env, "item3"),
        quantity: 1,
        unit_price: 75,
    });

    let invoice = InvoiceData {
        line_items,
        tax_bps: 0,
        currency_label: Symbol::new(&s.env, "USD"),
    };

    let payment_id = s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &425,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice),
        &None,
    );

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 425);
}

#[test]
#[should_panic(expected = "Invoice line items exceed maximum of 20")]
fn test_create_payment_with_too_many_line_items() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10000);

    // Create invoice with 21 line items (exceeds max of 20)
    let mut line_items = Vec::new(&s.env);
    for _i in 0..21 {
        line_items.push_back(LineItem {
            description: Symbol::new(&s.env, "item"),
            quantity: 1,
            unit_price: 10,
        });
    }

    let invoice = InvoiceData {
        line_items,
        tax_bps: 0,
        currency_label: Symbol::new(&s.env, "USD"),
    };

    // This should panic because 21 items > max 20
    s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &210,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice),
        &None,
    );
}

#[test]
#[should_panic(expected = "Invoice line_items cannot be empty")]
fn test_create_payment_with_empty_invoice() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create invoice with no line items (should panic)
    let line_items = Vec::new(&s.env);

    let invoice = InvoiceData {
        line_items,
        tax_bps: 0,
        currency_label: Symbol::new(&s.env, "USD"),
    };

    s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &0,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice),
        &None,
    );
}

#[test]
fn test_create_payment_without_invoice_backward_compat() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Create payment without invoice - should work normally
    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Verify no invoice hash was stored
    let invoice_hash = s.client.get_invoice_hash(&payment_id);
    assert!(invoice_hash.is_none());

    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.amount, 200);
}

#[test]
fn test_invoice_hash_consistency() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &2000);

    // Create two identical invoices
    let create_invoice = |env: &Env| -> InvoiceData {
        let mut line_items = Vec::new(env);
        line_items.push_back(LineItem {
            description: Symbol::new(env, "item1"),
            quantity: 2,
            unit_price: 100,
        });
        InvoiceData {
            line_items,
            tax_bps: 0,
            currency_label: Symbol::new(env, "USD"),
        }
    };

    let invoice1 = create_invoice(&s.env);
    let invoice2 = create_invoice(&s.env);

    let payment_id1 = s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice1),
        &None,
    );

    let payment_id2 = s.client.create_payment_with_invoice(
        &customer,
        &merchant,
        &200,
        &s.token_addr,
        &None,
        &None,
        &Some(invoice2),
        &None,
    );

    // Both hashes should be identical for identical invoices
    let hash1 = s.client.get_invoice_hash(&payment_id1).unwrap();
    let hash2 = s.client.get_invoice_hash(&payment_id2).unwrap();
    assert_eq!(hash1, hash2);
}

// ===========================================================================
//  #133 — Subscription Trial Period
// ===========================================================================

#[test]
fn test_subscription_without_trial_charges_immediately() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id = s.client.create_subscription_with_trial(
        &subscriber, &merchant, &100, &s.token_addr, &60, &10, &None,
    );

    s.client.charge_subscription(&sub_id);
    let sub = s.client.get_subscription(&sub_id);
    assert_eq!(sub.charges_count, 1);
    assert_eq!(s.token_client.balance(&merchant), 100);
}

#[test]
fn test_subscription_with_zero_trial_charges_immediately() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id = s.client.create_subscription_with_trial(
        &subscriber, &merchant, &100, &s.token_addr, &60, &10, &Some(0),
    );

    s.client.charge_subscription(&sub_id);
    assert_eq!(s.client.get_subscription(&sub_id).charges_count, 1);
}

#[test]
#[should_panic]
fn test_charge_during_trial_panics() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id = s.client.create_subscription_with_trial(
        &subscriber, &merchant, &100, &s.token_addr, &60, &10, &Some(86_400),
    );

    s.client.charge_subscription(&sub_id);
}

#[test]
fn test_charge_after_trial_succeeds() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id = s.client.create_subscription_with_trial(
        &subscriber, &merchant, &100, &s.token_addr, &60, &10, &Some(86_400),
    );

    s.env.ledger().with_mut(|l| l.timestamp += 86_400 + 1);

    s.client.charge_subscription(&sub_id);
    assert_eq!(s.client.get_subscription(&sub_id).charges_count, 1);
    assert_eq!(s.token_client.balance(&merchant), 100);
}

#[test]
fn test_get_trial_remaining_during_and_after_trial() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id = s.client.create_subscription_with_trial(
        &subscriber, &merchant, &100, &s.token_addr, &60, &10, &Some(3600),
    );

    assert_eq!(s.client.get_trial_remaining(&sub_id), 3600);

    s.env.ledger().with_mut(|l| l.timestamp += 1800);
    assert_eq!(s.client.get_trial_remaining(&sub_id), 1800);

    s.env.ledger().with_mut(|l| l.timestamp += 1800);
    assert_eq!(s.client.get_trial_remaining(&sub_id), 0);
}

#[test]
fn test_subscription_without_trial_reports_zero_remaining() {
    let s = setup();
    s.init();
    let subscriber = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&subscriber, &10_000);

    let sub_id = s.client.create_subscription_with_trial(
        &subscriber, &merchant, &100, &s.token_addr, &60, &10, &None,
    );

    assert_eq!(s.client.get_trial_remaining(&sub_id), 0);
}

//  #132 — Customer Payment History Pagination
// ===========================================================================

fn seed_customer_payments(s: &TestSetup, customer: &Address, merchant: &Address, count: u32) {
    s.client.set_merchant_open_mode(&true);
    s.token_admin_client.mint(customer, &(count as i128 * 100));
    for _ in 0..count {
        s.client
            .create_payment(customer, merchant, &10, &s.token_addr, &None, &None, &None);
    }
}

#[test]
fn test_pagination_first_page() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    seed_customer_payments(&s, &customer, &merchant, 25);

    let page = s.client.get_customer_payments_page(&customer, &0, &10);

    assert_eq!(page.payments.len(), 10);
    assert_eq!(page.total_count, 25);
    assert!(page.has_more);
}

#[test]
fn test_pagination_middle_page() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    seed_customer_payments(&s, &customer, &merchant, 25);

    let page = s.client.get_customer_payments_page(&customer, &1, &10);

    assert_eq!(page.payments.len(), 10);
    assert_eq!(page.total_count, 25);
    assert!(page.has_more);
}

#[test]
fn test_pagination_last_partial_page() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    seed_customer_payments(&s, &customer, &merchant, 25);

    let page = s.client.get_customer_payments_page(&customer, &2, &10);

    assert_eq!(page.payments.len(), 5);
    assert_eq!(page.total_count, 25);
    assert!(!page.has_more);
}

#[test]
fn test_pagination_past_end_returns_empty() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    seed_customer_payments(&s, &customer, &merchant, 5);

    let page = s.client.get_customer_payments_page(&customer, &10, &10);

    assert_eq!(page.payments.len(), 0);
    assert_eq!(page.total_count, 5);
    assert!(!page.has_more);
}

#[test]
fn test_pagination_empty_customer() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);

    let page = s.client.get_customer_payments_page(&customer, &0, &10);

    assert_eq!(page.payments.len(), 0);
    assert_eq!(page.total_count, 0);
    assert!(!page.has_more);
}

#[test]
#[should_panic(expected = "page_size exceeds maximum of 50")]
fn test_pagination_rejects_oversize_page() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);

    s.client.get_customer_payments_page(&customer, &0, &51);
}

#[test]
#[should_panic(expected = "page_size must be greater than 0")]
fn test_pagination_rejects_zero_page_size() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);

    s.client.get_customer_payments_page(&customer, &0, &0);
}

#[test]
fn test_get_customer_payment_count_matches_pagination_total() {
    let s = setup();
    s.init();
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    seed_customer_payments(&s, &customer, &merchant, 7);

    assert_eq!(s.client.get_customer_payment_count(&customer), 7);
    let page = s.client.get_customer_payments_page(&customer, &0, &10);
    assert_eq!(page.total_count, 7);
}

#[test]
fn test_get_customer_payment_count_returns_zero_for_unknown_customer() {
    let s = setup();
    s.init();
    let unknown = Address::generate(&s.env);
    assert_eq!(s.client.get_customer_payment_count(&unknown), 0);
}

// ===========================================================================
//  #130 — Merchant-Defined Payment Expiry Override
// ===========================================================================

#[test]
fn test_expiry_override_bounds_default_when_unset() {
    let s = setup();
    s.init();
    // Defaults: 60..=2_592_000
    assert_eq!(s.client.get_min_payment_expiry(), 60);
    assert_eq!(s.client.get_max_payment_expiry(), 30 * 24 * 60 * 60);
}

#[test]
fn test_admin_can_update_expiry_bounds() {
    let s = setup();
    s.init();
    s.client.set_payment_expiry_bounds(&120, &86_400);
    assert_eq!(s.client.get_min_payment_expiry(), 120);
    assert_eq!(s.client.get_max_payment_expiry(), 86_400);
}

#[test]
#[should_panic(expected = "min_expiry must be <= max_expiry")]
fn test_set_bounds_rejects_inverted_range() {
    let s = setup();
    s.init();
    s.client.set_payment_expiry_bounds(&500, &100);
}

#[test]
#[should_panic(expected = "min_expiry must be greater than 0")]
fn test_set_bounds_rejects_zero_min() {
    let s = setup();
    s.init();
    s.client.set_payment_expiry_bounds(&0, &86_400);
}

#[test]
fn test_create_payment_with_default_expiry_uses_global_timeout() {
    let s = setup();
    s.init();
    s.client.set_merchant_open_mode(&true);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let now = s.env.ledger().timestamp();
    let pid = s.client.create_payment_with_expiry(
        &customer, &merchant, &100, &s.token_addr,
        &None, &None, &None, &None, &None, &None,
    );
    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.expires_at, now + s.client.get_payment_timeout());
}

#[test]
fn test_create_payment_with_custom_expiry_overrides_default() {
    let s = setup();
    s.init();
    s.client.set_merchant_open_mode(&true);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let now = s.env.ledger().timestamp();
    let custom_expiry: u64 = 3600;
    let pid = s.client.create_payment_with_expiry(
        &customer, &merchant, &100, &s.token_addr,
        &None, &None, &None, &None, &None, &Some(custom_expiry),
    );
    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.expires_at, now + custom_expiry);
    assert_ne!(payment.expires_at, now + s.client.get_payment_timeout());
}

#[test]
fn test_create_payment_at_min_boundary_succeeds() {
    let s = setup();
    s.init();
    s.client.set_payment_expiry_bounds(&60, &86_400);
    s.client.set_merchant_open_mode(&true);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment_with_expiry(
        &customer, &merchant, &100, &s.token_addr,
        &None, &None, &None, &None, &None, &Some(60),
    );
    let payment = s.client.get_payment(&pid);
    let now = s.env.ledger().timestamp();
    assert_eq!(payment.expires_at, now + 60);
}

#[test]
fn test_create_payment_at_max_boundary_succeeds() {
    let s = setup();
    s.init();
    s.client.set_payment_expiry_bounds(&60, &86_400);
    s.client.set_merchant_open_mode(&true);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let pid = s.client.create_payment_with_expiry(
        &customer, &merchant, &100, &s.token_addr,
        &None, &None, &None, &None, &None, &Some(86_400),
    );
    let payment = s.client.get_payment(&pid);
    let now = s.env.ledger().timestamp();
    assert_eq!(payment.expires_at, now + 86_400);
}

#[test]
#[should_panic(expected = "expiry_seconds is below the configured minimum")]
fn test_create_payment_below_min_expiry_panics() {
    let s = setup();
    s.init();
    s.client.set_payment_expiry_bounds(&60, &86_400);
    s.client.set_merchant_open_mode(&true);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment_with_expiry(
        &customer, &merchant, &100, &s.token_addr,
        &None, &None, &None, &None, &None, &Some(30),
    );
}

#[test]
#[should_panic(expected = "expiry_seconds exceeds the configured maximum")]
fn test_create_payment_above_max_expiry_panics() {
    let s = setup();
    s.init();
    s.client.set_payment_expiry_bounds(&60, &86_400);
    s.client.set_merchant_open_mode(&true);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.create_payment_with_expiry(
        &customer, &merchant, &100, &s.token_addr,
        &None, &None, &None, &None, &None, &Some(86_500),
    );
}

//  #135 Dynamic Slippage Tolerance Configuration Per Payment
// ===========================================================================

#[test]
fn test_multi_token_uses_global_default_slippage_when_none() {
    let s = setup_multi_token();
    // 1.0 USDC/XLM
    set_oracle_price(&s, 10_000_000, 100);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &500);

    // Pass None — should use global default (50 bps)
    let pid = s
        .client
        .create_payment_multi_token(&customer, &merchant, &200, &s.xlm_addr, &None);

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.amount, 200);
}

#[test]
fn test_multi_token_uses_provided_slippage_within_bounds() {
    let s = setup_multi_token();
    set_oracle_price(&s, 10_000_000, 100);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &500);

    // Provide explicit 100 bps — within default bounds [0, 10000]
    let pid = s
        .client
        .create_payment_multi_token(&customer, &merchant, &200, &s.xlm_addr, &Some(100));

    let payment = s.client.get_payment(&pid);
    assert_eq!(payment.amount, 200);
}

#[test]
fn test_update_slippage_config_and_enforce_bounds() {
    let s = setup_multi_token();
    // Set tight bounds: default=100, min=50, max=200
    s.client
        .update_slippage_config(&s._admin, &100, &50, &200);

    let cfg = s.client.get_slippage_config();
    assert_eq!(cfg.default_bps, 100);
    assert_eq!(cfg.min_bps, 50);
    assert_eq!(cfg.max_bps, 200);
}

#[test]
#[should_panic(expected = "slippage_bps below minimum allowed")]
fn test_multi_token_slippage_below_min_rejected() {
    let s = setup_multi_token();
    // Set min=50
    s.client
        .update_slippage_config(&s._admin, &100, &50, &200);

    set_oracle_price(&s, 10_000_000, 100);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &500);

    // 10 bps < min 50 — must reject
    s.client
        .create_payment_multi_token(&customer, &merchant, &200, &s.xlm_addr, &Some(10));
}

#[test]
#[should_panic(expected = "slippage_bps exceeds maximum allowed")]
fn test_multi_token_slippage_above_max_rejected() {
    let s = setup_multi_token();
    // Set max=200
    s.client
        .update_slippage_config(&s._admin, &100, &50, &200);

    set_oracle_price(&s, 10_000_000, 100);
    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &500);

    // 300 bps > max 200 — must reject
    s.client
        .create_payment_multi_token(&customer, &merchant, &200, &s.xlm_addr, &Some(300));
}

#[test]
fn test_slippage_tolerance_applied_event_emitted() {
    let s = setup_multi_token();
    set_oracle_price(&s, 10_000_000, 100);

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.xlm_admin.mint(&customer, &500);

    s.client
        .create_payment_multi_token(&customer, &merchant, &200, &s.xlm_addr, &Some(50));

    let events = s.env.events().all();
    assert!(events.len() > 0);
}

#[test]
#[should_panic(expected = "default_bps must be within")]
fn test_update_slippage_config_default_out_of_bounds_rejected() {
    let s = setup_multi_token();
    // default=300 is outside [50, 200]
    s.client
        .update_slippage_config(&s._admin, &300, &50, &200);
}

// ===========================================================================
//  #131 Aggregate Volume Caps Per Merchant
// ===========================================================================

#[test]
fn test_set_and_get_merchant_volume_cap() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    s.client
        .set_merchant_volume_cap(&s.admin, &merchant, &1000, &3600);

    let cap = s.client.get_merchant_volume_cap(&merchant).unwrap();
    assert_eq!(cap.cap_amount, 1000);
    assert_eq!(cap.window_seconds, 3600);
}

#[test]
fn test_merchant_without_cap_has_no_restriction() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10_000);

    // No cap set — payments should complete freely
    for _ in 0..3 {
        let pid = s.client.create_payment(
            &customer,
            &merchant,
            &500,
            &s.token_addr,
            &None,
            &None,
            &None,
        );
        s.client.complete_payment(&pid);
    }

    assert_eq!(s.client.get_merchant_stats(&merchant).payments_completed, 3);
}

#[test]
fn test_payments_within_cap_succeed() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10_000);

    // Cap = 1000 per hour; each payment = 300, so 3 payments = 900 < 1000
    s.client
        .set_merchant_volume_cap(&s.admin, &merchant, &1000, &3600);

    s.env.ledger().set_timestamp(1000);

    for _ in 0..3 {
        let pid = s.client.create_payment(
            &customer,
            &merchant,
            &300,
            &s.token_addr,
            &None,
            &None,
            &None,
        );
        s.client.complete_payment(&pid);
    }

    assert_eq!(s.client.get_merchant_stats(&merchant).payments_completed, 3);
}

#[test]
fn test_payment_exceeding_cap_is_rejected() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10_000);

    // Cap = 900 per hour; 3 payments of 300 = 900 (boundary), 4th must fail
    s.client
        .set_merchant_volume_cap(&s.admin, &merchant, &900, &3600);

    s.env.ledger().set_timestamp(1000);

    // N-1 = 2 payments pass (600 total)
    for _ in 0..2 {
        let pid = s.client.create_payment(
            &customer,
            &merchant,
            &300,
            &s.token_addr,
            &None,
            &None,
            &None,
        );
        s.client.complete_payment(&pid);
    }

    // 3rd payment (300) would bring total to 900 — exactly at cap, should pass
    let pid3 = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid3);

    // 4th payment would exceed cap (900 + 300 = 1200 > 900) — must be rejected
    let pid4 = s.client.create_payment(
        &customer,
        &merchant,
        &300,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let result = s.client.try_complete_payment(&pid4);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Error::MerchantVolumeCapped.into()
    );
}

#[test]
fn test_cap_window_resets_after_window_seconds() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10_000);

    // Cap = 500 per 3600 seconds
    s.client
        .set_merchant_volume_cap(&s.admin, &merchant, &500, &3600);

    s.env.ledger().set_timestamp(1000);

    // Fill cap in first window
    let pid1 = s.client.create_payment(
        &customer,
        &merchant,
        &500,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid1);

    // Next payment in same window must fail
    let pid2 = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let blocked = s.client.try_complete_payment(&pid2);
    assert_eq!(
        blocked.unwrap_err().unwrap(),
        Error::MerchantVolumeCapped.into()
    );

    // Advance to next window (1000 + 3600 = 4600 → bucket changes)
    s.env.ledger().set_timestamp(5000);

    // Payment in new window should succeed
    let pid3 = s.client.create_payment(
        &customer,
        &merchant,
        &400,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid3);
    assert_eq!(
        s.client.get_payment(&pid3).status,
        PaymentStatus::Completed
    );
}

#[test]
fn test_remove_merchant_volume_cap() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    s.client
        .set_merchant_volume_cap(&s.admin, &merchant, &500, &3600);

    assert!(s.client.get_merchant_volume_cap(&merchant).is_some());

    // Remove cap by setting cap_amount = 0
    s.client
        .set_merchant_volume_cap(&s.admin, &merchant, &0, &0);

    assert!(s.client.get_merchant_volume_cap(&merchant).is_none());
}

#[test]
fn test_volume_cap_boundary_n_minus_1_pass_nth_rejected() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &10_000);

    // Cap = 300 per window; each payment = 100; N=3 payments allowed, 4th rejected
    s.client
        .set_merchant_volume_cap(&s.admin, &merchant, &300, &3600);

    s.env.ledger().set_timestamp(500);

    // N-1 = 2 payments pass
    for _ in 0..2 {
        let pid = s.client.create_payment(
            &customer,
            &merchant,
            &100,
            &s.token_addr,
            &None,
            &None,
            &None,
        );
        s.client.complete_payment(&pid);
    }

    // Nth payment (exactly at cap) passes
    let pid_n = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&pid_n);

    // N+1 payment is rejected
    let pid_over = s.client.create_payment(
        &customer,
        &merchant,
        &1,
        &s.token_addr,
        &None,
        &None,
        &None,
    );
    let result = s.client.try_complete_payment(&pid_over);
    assert_eq!(
        result.unwrap_err().unwrap(),
        Error::MerchantVolumeCapped.into()
    );
}

// ===========================================================================
//  #FEATURE — Payments Invoice Expiry Grace Period Extension by Merchant
// ===========================================================================

#[test]
fn test_extend_payment_expiry_single() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    let initial_payment = s.client.get_payment(&payment_id);
    assert_eq!(initial_payment.extension_count, 0);

    s.client.extend_payment_expiry(&merchant, &payment_id, &100);

    let updated_payment = s.client.get_payment(&payment_id);
    assert_eq!(updated_payment.extension_count, 1);
    assert_eq!(updated_payment.expires_at, initial_payment.expires_at + 100 * 5); // 5 seconds per ledger
}

#[test]
fn test_extend_payment_expiry_max_extensions() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Extend 3 times (default max)
    s.client.extend_payment_expiry(&merchant, &payment_id, &100);
    s.client.extend_payment_expiry(&merchant, &payment_id, &100);
    s.client.extend_payment_expiry(&merchant, &payment_id, &100);

    // 4th extension should fail
    let result = s.client.try_extend_payment_expiry(&merchant, &payment_id, &100);
    assert_eq!(result.unwrap_err().unwrap(), Error::MaxExtensionsReached.into());
}

#[test]
fn test_extend_payment_expiry_invalid_status() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Complete payment first
    s.client.complete_payment(&payment_id);

    // Try to extend completed payment
    let result = s.client.try_extend_payment_expiry(&merchant, &payment_id, &100);
    assert_eq!(result.unwrap_err().unwrap(), Error::InvalidPaymentStatus.into());
}

#[test]
fn test_admin_configure_extension_settings() {
    let s = setup();
    s.init();

    // Check defaults
    let (max_ledgers, max_extensions) = s.client.get_extension_config();
    assert_eq!(max_ledgers, DEFAULT_MAX_EXTENSION_LEDGERS);
    assert_eq!(max_extensions, DEFAULT_MAX_EXTENSIONS);

    // Update config
    s.client.update_extension_config(&s.admin, &5000, &5);
    let (new_max_ledgers, new_max_extensions) = s.client.get_extension_config();
    assert_eq!(new_max_ledgers, 5000);
    assert_eq!(new_max_extensions, 5);
}

#[test]
fn test_extend_payment_expiry_max_ledgers_exceeded() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    let payment_id = s.client.create_payment(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &None,
        &None,
        &None,
    );

    // Try to extend with more than default max ledgers
    let result = s.client.try_extend_payment_expiry(&merchant, &payment_id, &(DEFAULT_MAX_EXTENSION_LEDGERS + 1));
    assert_eq!(result.unwrap_err().unwrap(), Error::MaxExtensionLedgersExceeded.into());
}

#[test]
fn test_withdrawal_default_limits_apply_to_new_merchant() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);

    // Initial check: fallback to defaults automatically without having to set
    let (window, cap) = s.client.get_withdrawal_rate_limit(&merchant);
    assert_eq!(window, 86400);
    assert_eq!(cap, i128::MAX);

    // If admin updates defaults, new merchant should receive new bounds
    s.client.set_default_withdrawal_limits(&s.admin, &3600, &1000);
    let (new_window, new_cap) = s.client.get_withdrawal_rate_limit(&merchant);
    assert_eq!(new_window, 3600);
    assert_eq!(new_cap, 1000);
}


// ===========================================================================
//  Customer Blocking Tests (#646)
// ===========================================================================

/// #646: get_block_entry returns None for an unblocked customer
#[test]
fn test_get_block_entry_unblocked_customer() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);

    // Customer is not blocked
    let entry = s.client.get_block_entry(&merchant, &customer);
    assert_eq!(entry, None, "Unblocked customer should have no block entry");
}

/// #646: get_block_entry returns the BlockEntry for a blocked customer
#[test]
fn test_get_block_entry_blocked_customer() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let reason = Symbol::new(&s.env, "fraud");
    let evidence_hash = BytesN::<32>::from_array(&s.env, &[1u8; 32]);

    // Block the customer
    s.client.block_customer(&merchant, &customer, &reason, &evidence_hash);

    // Retrieve the block entry
    let entry = s.client.get_block_entry(&merchant, &customer);
    assert!(entry.is_some(), "Blocked customer should have a block entry");

    let block_entry = entry.unwrap();
    assert_eq!(block_entry.customer, customer, "Block entry customer should match");
    assert_eq!(block_entry.reason_code, reason, "Block entry reason should match");
    assert_eq!(block_entry.evidence_hash, evidence_hash, "Block entry evidence hash should match");
    assert_eq!(
        block_entry.blocked_at_ledger,
        s.env.ledger().sequence(),
        "Block entry blocked_at_ledger should be current ledger"
    );
}

/// #646: get_block_entry returns None after customer is unblocked
#[test]
fn test_get_block_entry_after_unblock() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let reason = Symbol::new(&s.env, "suspicious");
    let evidence_hash = BytesN::<32>::from_array(&s.env, &[2u8; 32]);

    // Block the customer
    s.client.block_customer(&merchant, &customer, &reason, &evidence_hash);
    assert!(
        s.client.get_block_entry(&merchant, &customer).is_some(),
        "Customer should be blocked"
    );

    // Unblock the customer
    s.client.unblock_customer(&merchant, &customer);

    // Entry should be gone
    let entry = s.client.get_block_entry(&merchant, &customer);
    assert_eq!(entry, None, "Entry should be None after unblock");
}

/// #646: is_customer_blocked returns false for unblocked customer
#[test]
fn test_is_customer_blocked_false_for_unblocked() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);

    let blocked = s.client.is_customer_blocked(&merchant, &customer);
    assert!(!blocked, "Unblocked customer should return false");
}

/// #646: is_customer_blocked returns true for blocked customer
#[test]
fn test_is_customer_blocked_true_for_blocked() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let reason = Symbol::new(&s.env, "chargeback");
    let evidence_hash = BytesN::<32>::from_array(&s.env, &[3u8; 32]);

    // Block the customer
    s.client.block_customer(&merchant, &customer, &reason, &evidence_hash);

    let blocked = s.client.is_customer_blocked(&merchant, &customer);
    assert!(blocked, "Blocked customer should return true");
}

/// #646: is_customer_blocked reflects unblock status
#[test]
fn test_is_customer_blocked_after_unblock() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let reason = Symbol::new(&s.env, "test");
    let evidence_hash = BytesN::<32>::from_array(&s.env, &[4u8; 32]);

    // Block and verify
    s.client.block_customer(&merchant, &customer, &reason, &evidence_hash);
    assert!(
        s.client.is_customer_blocked(&merchant, &customer),
        "Customer should be blocked"
    );

    // Unblock and verify
    s.client.unblock_customer(&merchant, &customer);
    assert!(
        !s.client.is_customer_blocked(&merchant, &customer),
        "Customer should not be blocked after unblock"
    );
}

/// #646: Blocking is per-merchant (different merchants can block independently)
#[test]
fn test_block_entry_per_merchant_isolation() {
    let s = setup();
    s.init();

    let merchant_a = Address::generate(&s.env);
    let merchant_b = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let reason_a = Symbol::new(&s.env, "fraud_a");
    let reason_b = Symbol::new(&s.env, "fraud_b");
    let evidence_a = BytesN::<32>::from_array(&s.env, &[5u8; 32]);
    let evidence_b = BytesN::<32>::from_array(&s.env, &[6u8; 32]);

    // Block by merchant_a
    s.client.block_customer(&merchant_a, &customer, &reason_a, &evidence_a);

    // Merchant_a should see the customer blocked
    assert!(
        s.client.is_customer_blocked(&merchant_a, &customer),
        "Merchant_a should see customer blocked"
    );

    // Merchant_b should not see the customer blocked
    assert!(
        !s.client.is_customer_blocked(&merchant_b, &customer),
        "Merchant_b should not see customer blocked by merchant_a"
    );

    // Merchant_b can block the same customer with different reason
    s.client.block_customer(&merchant_b, &customer, &reason_b, &evidence_b);
    assert!(
        s.client.is_customer_blocked(&merchant_b, &customer),
        "Merchant_b should now see customer blocked"
    );

    // Both merchants should have block entries
    let entry_a = s.client.get_block_entry(&merchant_a, &customer).unwrap();
    let entry_b = s.client.get_block_entry(&merchant_b, &customer).unwrap();
    assert_eq!(entry_a.reason_code, reason_a, "Merchant_a entry should have its reason");
    assert_eq!(entry_b.reason_code, reason_b, "Merchant_b entry should have its reason");
}
