use soroban_sdk::{
    testutils::{Address as _, Events, Ledger, LedgerInfo},
    token, Address, Env, Map, String, Symbol, TryFromVal, Val,
};

use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient, PaymentStatus};
use ahjoor_refund::{AhjoorRefundContract, AhjoorRefundContractClient, RefundStatus};

/// Shared test harness: deploys ahjoor-payments and ahjoor-refund side by
/// side, wires the refund contract to the real payments contract, and funds
/// a customer so each test can drive a payment through to a refund.
struct TestEnvironment<'a> {
    env: Env,
    payments: AhjoorPaymentsContractClient<'a>,
    refund: AhjoorRefundContractClient<'a>,
    token_client: token::Client<'a>,
    admin: Address,
    customer: Address,
    merchant: Address,
}

impl<'a> TestEnvironment<'a> {
    fn setup(customer_funds: i128) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1_000_000,
            protocol_version: 23,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });

        let admin = Address::generate(&env);
        let customer = Address::generate(&env);
        let merchant = Address::generate(&env);

        let token_admin_addr = Address::generate(&env);
        let token_contract_id = env.register_stellar_asset_contract_v2(token_admin_addr);
        let token_client = token::Client::new(&env, &token_contract_id.address());
        let token_admin = token::StellarAssetClient::new(&env, &token_contract_id.address());
        token_admin.mint(&customer, &customer_funds);

        let payments_id = env.register(AhjoorPaymentsContract, ());
        let payments = AhjoorPaymentsContractClient::new(&env, &payments_id);
        payments.initialize(&admin, &admin, &0u32);

        let refund_id = env.register(AhjoorRefundContract, ());
        let refund = AhjoorRefundContractClient::new(&env, &refund_id);
        // 1 day dispute window, default config.
        refund.initialize(&admin, &payments_id, &86_400u64, &None);

        TestEnvironment {
            env,
            payments,
            refund,
            token_client,
            admin,
            customer,
            merchant,
        }
    }

    /// Creates a payment in ahjoor-payments and completes it via the admin.
    fn create_completed_payment(&self, amount: i128) -> u32 {
        let pid = self.payments.create_payment(
            &self.customer,
            &self.merchant,
            &amount,
            &self.token_client.address,
            &None,
            &None,
            &None,
        );
        self.payments.complete_payment(&pid);
        pid
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token_client.balance(who)
    }

    /// Returns the data map of the event named `name` published by `contract`
    /// during the most recent top-level invocation, if any.
    fn find_event(&self, contract: &Address, name: &str) -> Option<Map<Symbol, Val>> {
        let topic = Symbol::new(&self.env, name);
        self.env.events().all().iter().find_map(|(addr, topics, data)| {
            let first = topics.get(0)?;
            let is_match = &addr == contract
                && Symbol::try_from_val(&self.env, &first).ok().as_ref() == Some(&topic);
            if is_match {
                Map::<Symbol, Val>::try_from_val(&self.env, &data).ok()
            } else {
                None
            }
        })
    }

    fn event_field<T: TryFromVal<Env, Val>>(&self, data: &Map<Symbol, Val>, field: &str) -> T {
        let raw = data
            .get(Symbol::new(&self.env, field))
            .unwrap_or_else(|| panic!("event field `{}` missing", field));
        T::try_from_val(&self.env, &raw)
            .unwrap_or_else(|_| panic!("event field `{}` has unexpected type", field))
    }
}

/// End-to-end: a payment is created and completed in ahjoor-payments, then a
/// partial refund is requested, approved and processed in ahjoor-refund
/// against that real payment. Asserts token movements, storage state in both
/// contracts, and the events each contract emits along the way.
#[test]
fn test_refund_against_completed_payment_moves_funds_and_keeps_state_consistent() {
    const PAYMENT: i128 = 1_000;
    const REFUND: i128 = 400;
    const FUNDS: i128 = 5_000;

    let t = TestEnvironment::setup(FUNDS);
    let payments_addr = t.payments.address.clone();
    let refund_addr = t.refund.address.clone();

    // --- ahjoor-payments: create + complete -----------------------------
    let pid = t.create_completed_payment(PAYMENT);

    let completed = t
        .find_event(&payments_addr, "payment_completed")
        .expect("payments contract should emit payment_completed");
    assert_eq!(t.event_field::<u32>(&completed, "payment_id"), pid);
    assert_eq!(t.event_field::<i128>(&completed, "amount"), PAYMENT);
    assert_eq!(t.event_field::<Address>(&completed, "merchant"), t.merchant);

    let payment = t.payments.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Completed);
    assert_eq!(payment.amount, PAYMENT);
    assert_eq!(payment.customer, t.customer);
    assert_eq!(payment.merchant, t.merchant);
    assert_eq!(t.balance(&t.customer), FUNDS - PAYMENT);

    // The refund contract reads the payment through the cross-contract client.
    assert_eq!(t.refund.get_refundable_remaining(&pid), PAYMENT);

    // --- ahjoor-refund: request -----------------------------------------
    let customer_before_request = t.balance(&t.customer);
    let reason = String::from_str(&t.env, "item damaged on arrival");
    let rid = t
        .refund
        .request_refund(&t.customer, &pid, &REFUND, &reason, &0u32);

    let requested = t
        .find_event(&refund_addr, "refund_requested")
        .expect("refund contract should emit refund_requested");
    assert_eq!(t.event_field::<u32>(&requested, "refund_id"), rid);
    assert_eq!(t.event_field::<Address>(&requested, "customer"), t.customer);
    assert_eq!(t.event_field::<i128>(&requested, "amount"), REFUND);
    assert_eq!(
        t.event_field::<Address>(&requested, "token"),
        t.token_client.address
    );

    let refund = t.refund.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Requested);
    assert_eq!(refund.payment_id, pid);
    assert_eq!(refund.amount, REFUND);
    assert_eq!(refund.customer, t.customer);
    // Merchant and token are cached from the payments contract's record.
    assert_eq!(refund.merchant, payment.merchant);
    assert_eq!(refund.token, payment.token);
    assert_eq!(t.refund.get_refunds_by_payment(&pid).len(), 1);
    assert_eq!(t.refund.get_pending_refund_count(), 1);

    // Requested funds are escrowed in the refund contract.
    assert_eq!(t.balance(&t.customer), customer_before_request - REFUND);
    assert_eq!(t.balance(&refund_addr), REFUND);

    // --- ahjoor-refund: approve -----------------------------------------
    t.refund.approve_refund(&t.admin, &rid);

    let approved = t
        .find_event(&refund_addr, "refund_approved")
        .expect("refund contract should emit refund_approved");
    assert_eq!(t.event_field::<u32>(&approved, "refund_id"), rid);
    assert_eq!(t.event_field::<Address>(&approved, "approved_by"), t.admin);

    assert_eq!(t.refund.get_refund(&rid).status, RefundStatus::Approved);
    assert_eq!(t.refund.get_pending_refund_count(), 0);
    // Approval alone (no merchant reserve) does not move funds.
    assert_eq!(t.balance(&refund_addr), REFUND);

    // --- ahjoor-refund: process -----------------------------------------
    t.refund.process_refund(&t.admin, &rid);

    let processed = t
        .find_event(&refund_addr, "refund_processed")
        .expect("refund contract should emit refund_processed");
    assert_eq!(t.event_field::<u32>(&processed, "refund_id"), rid);
    assert_eq!(t.event_field::<Address>(&processed, "customer"), t.customer);
    assert_eq!(t.event_field::<i128>(&processed, "amount"), REFUND);
    assert!(
        t.find_event(&refund_addr, "partial_refund_processed").is_some(),
        "refund contract should emit partial_refund_processed"
    );

    let refund = t.refund.get_refund(&rid);
    assert_eq!(refund.status, RefundStatus::Processed);
    assert!(refund.processed_at.is_some());
    assert_eq!(refund.fee_amount, None);

    // Funds leave the refund contract and return to the customer in full.
    assert_eq!(t.balance(&refund_addr), 0);
    assert_eq!(t.balance(&t.customer), customer_before_request);

    // --- Cross-contract consistency --------------------------------------
    // The payments record is untouched by the refund flow, and the refund
    // contract's remaining view reflects the processed amount against it.
    let payment_after = t.payments.get_payment(&pid);
    assert_eq!(payment_after.status, PaymentStatus::Completed);
    assert_eq!(payment_after.amount, PAYMENT);
    assert_eq!(t.refund.get_refundable_remaining(&pid), PAYMENT - REFUND);

    let stats = t.refund.get_merchant_refund_stats(&t.merchant);
    assert_eq!(stats.total_requested, 1);
    assert_eq!(stats.total_approved, 1);
    assert_eq!(stats.total_processed, 1);
    assert_eq!(stats.total_amount_refunded, REFUND);
}

/// Once refunds processed against a payment reach its amount, the refund
/// contract (reading the real payment) rejects any further request.
#[test]
fn test_cumulative_refunds_capped_by_real_payment_amount() {
    const PAYMENT: i128 = 1_000;
    let t = TestEnvironment::setup(10_000);
    let pid = t.create_completed_payment(PAYMENT);
    let reason = String::from_str(&t.env, "full refund");

    let rid = t
        .refund
        .request_refund(&t.customer, &pid, &PAYMENT, &reason, &1u32);
    t.refund.approve_refund(&t.admin, &rid);
    t.refund.process_refund(&t.admin, &rid);
    assert_eq!(t.refund.get_refundable_remaining(&pid), 0);

    let counter_before = t.refund.get_refund_counter();
    let result = t
        .refund
        .try_request_refund(&t.customer, &pid, &1, &reason, &1u32);
    assert!(result.is_err(), "refund beyond payment amount must be rejected");
    assert_eq!(t.refund.get_refund_counter(), counter_before);
    assert_eq!(t.balance(&t.refund.address), 0);
}

/// A refund cannot be requested against a payment that exists in
/// ahjoor-payments but has not been completed.
#[test]
fn test_refund_against_pending_payment_is_rejected() {
    let t = TestEnvironment::setup(5_000);
    let pid = t.payments.create_payment(
        &t.customer,
        &t.merchant,
        &500,
        &t.token_client.address,
        &None,
        &None,
        &None,
    );
    assert_eq!(t.payments.get_payment(&pid).status, PaymentStatus::Pending);

    let result = t.refund.try_request_refund(
        &t.customer,
        &pid,
        &100,
        &String::from_str(&t.env, "not completed"),
        &0u32,
    );
    assert!(result.is_err());
    assert_eq!(t.refund.get_refund_counter(), 0);
    assert_eq!(t.balance(&t.refund.address), 0);
}
