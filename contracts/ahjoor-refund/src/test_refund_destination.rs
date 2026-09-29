#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};

struct TestSetup<'a> {
    env: Env,
    refund_client: AhjoorRefundContractClient<'a>,
    payment_client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
    customer: Address,
    merchant: Address,
}

fn setup<'a>() -> TestSetup<'a> {
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
    refund_client.initialize(&admin, &payment_id, &86_400u64, &None);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    TestSetup {
        env,
        refund_client,
        payment_client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
        customer,
        merchant,
    }
}

impl<'a> TestSetup<'a> {
    fn completed_payment(&self, amount: i128) -> u32 {
        self.token_admin_client.mint(&self.customer, &(amount * 2));
        let pid = self.payment_client.create_payment(
            &self.customer,
            &self.merchant,
            &amount,
            &self.token_addr,
            &None,
            &None,
            &None,
        );
        self.payment_client.complete_payment(&pid);
        pid
    }

    fn reason(&self) -> String {
        String::from_str(&self.env, "Lost wallet")
    }
}

#[test]
fn test_default_refund_goes_to_payer() {
    let s = setup();
    let pid = s.completed_payment(500);
    let refund_id = s
        .refund_client
        .request_refund(&s.customer, &pid, &500, &s.reason(), &0);
    assert_eq!(s.refund_client.get_refund(&refund_id).destination, None);

    let before = s.token_client.balance(&s.customer);
    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.process_refund(&s.admin, &refund_id);
    assert_eq!(s.token_client.balance(&s.customer), before + 500);
}

#[test]
fn test_alternate_destination_receives_funds() {
    let s = setup();
    let new_wallet = Address::generate(&s.env);
    let pid = s.completed_payment(500);
    let refund_id = s.refund_client.request_refund_to(
        &s.customer,
        &pid,
        &500,
        &s.reason(),
        &0,
        &new_wallet,
    );

    // Visible to the merchant before approval.
    let refund = s.refund_client.get_refund(&refund_id);
    assert_eq!(refund.status, RefundStatus::Requested);
    assert_eq!(refund.destination, Some(new_wallet.clone()));

    let payer_before = s.token_client.balance(&s.customer);
    s.refund_client.approve_refund(&s.admin, &refund_id);
    s.refund_client.process_refund(&s.admin, &refund_id);

    assert_eq!(s.token_client.balance(&new_wallet), 500);
    assert_eq!(s.token_client.balance(&s.customer), payer_before);
}

#[test]
fn test_alternate_destination_receives_store_credit() {
    let s = setup();
    let new_wallet = Address::generate(&s.env);
    let pid = s.completed_payment(500);
    let refund_id = s.refund_client.request_refund_to(
        &s.customer,
        &pid,
        &500,
        &s.reason(),
        &0,
        &new_wallet,
    );

    let expiry = s.env.ledger().sequence() as u64 + 1_000;
    s.refund_client
        .approve_refund_as_voucher(&s.admin, &refund_id, &500, &expiry);

    let credit = s.refund_client.get_store_credit(&s.merchant, &new_wallet);
    assert_eq!(credit.customer, new_wallet);
    assert_eq!(credit.credit_amount, 500);
}

#[test]
fn test_destination_equal_to_payer_is_not_stored() {
    let s = setup();
    let pid = s.completed_payment(500);
    let refund_id = s.refund_client.request_refund_to(
        &s.customer,
        &pid,
        &500,
        &s.reason(),
        &0,
        &s.customer,
    );
    assert_eq!(s.refund_client.get_refund(&refund_id).destination, None);
}

#[test]
#[should_panic(expected = "OnlyOriginalPayerCanRedirect")]
fn test_only_original_payer_can_set_destination() {
    let s = setup();
    let pid = s.completed_payment(500);
    let impostor = Address::generate(&s.env);
    s.token_admin_client.mint(&impostor, &500);
    s.refund_client.request_refund_to(
        &impostor,
        &pid,
        &500,
        &s.reason(),
        &0,
        &impostor,
    );
}

#[test]
#[should_panic(expected = "InvalidRefundDestination")]
fn test_contract_cannot_be_destination() {
    let s = setup();
    let pid = s.completed_payment(500);
    s.refund_client.request_refund_to(
        &s.customer,
        &pid,
        &500,
        &s.reason(),
        &0,
        &s.refund_client.address,
    );
}

#[test]
fn test_opted_out_merchant_rejects_alternate_destination() {
    let s = setup();
    assert!(s.refund_client.get_allow_alternate_destination(&s.merchant));
    s.refund_client
        .set_allow_alternate_destination(&s.merchant, &false);
    assert!(!s.refund_client.get_allow_alternate_destination(&s.merchant));

    let pid = s.completed_payment(500);
    let new_wallet = Address::generate(&s.env);
    let res = s.refund_client.try_request_refund_to(
        &s.customer,
        &pid,
        &500,
        &s.reason(),
        &0,
        &new_wallet,
    );
    assert!(res.is_err());

    // Plain refunds to the payer are unaffected.
    let refund_id = s
        .refund_client
        .request_refund(&s.customer, &pid, &500, &s.reason(), &0);
    assert_eq!(s.refund_client.get_refund(&refund_id).destination, None);
}

#[test]
fn test_rejected_refund_escrow_returned_to_destination() {
    let s = setup();
    let new_wallet = Address::generate(&s.env);
    let pid = s.completed_payment(500);
    let refund_id = s.refund_client.request_refund_to(
        &s.customer,
        &pid,
        &500,
        &s.reason(),
        &0,
        &new_wallet,
    );
    s.refund_client.cancel_refund_request(&s.customer, &refund_id);
    assert_eq!(s.token_client.balance(&new_wallet), 500);
}
