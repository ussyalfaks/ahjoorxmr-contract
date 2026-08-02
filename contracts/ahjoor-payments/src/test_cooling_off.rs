#![cfg(test)]
extern crate alloc;
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

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
    env.mock_all_auths();

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
}

#[test]
fn test_cooling_off_cancellation() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    // Set max cooling-off to 1000 ledgers
    s.client.set_max_cooling_off_ledgers(&s.admin, &1000);

    // Create payment with cooling-off
    let payment_id = s.client.create_payment_with_cooling_off(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &500, // 500 ledgers cooling-off
    );

    // Complete the payment
    s.client.complete_payment(&payment_id);

    // Verify payment is in cooling-off status
    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::CoolingOff);

    // Cancel during cooling-off
    s.client.cancel_during_cooling_off(&payment_id);

    // Verify payment is cancelled
    let payment = s.client.get_payment(&payment_id);
    assert_eq!(payment.status, PaymentStatus::CancelledInCoolingOff);
}

#[test]
fn test_cooling_off_expired() {
    let s = setup();
    s.init();

    let customer = Address::generate(&s.env);
    let merchant = Address::generate(&s.env);
    s.token_admin_client.mint(&customer, &1000);

    s.client.set_max_cooling_off_ledgers(&s.admin, &1000);

    let payment_id = s.client.create_payment_with_cooling_off(
        &customer,
        &merchant,
        &100,
        &s.token_addr,
        &100, // 100 ledgers cooling-off
    );

    s.client.complete_payment(&payment_id);

    // Advance ledger past cooling-off period
    s.env.ledger().with_mut(|li| {
        li.sequence += 150;
    });

    // Try to cancel (should fail)
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        s.client.cancel_during_cooling_off(&payment_id);
    }));
    assert!(result.is_err());
}
