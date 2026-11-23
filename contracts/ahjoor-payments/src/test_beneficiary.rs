#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup<'a>() -> (
    Env,
    AhjoorPaymentsContractClient<'a>,
    Address,
    Address,
    Address,
    TokenClient<'a>,
    TokenAdminClient<'a>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin, &admin, &0u32);
    client.set_min_collateral(&0i128);
    client.approve_merchant(&merchant);
    // 1 point per 1_000_000 units
    client.configure_loyalty(&admin, &1u32, &100u32, &0i128, &0u32);

    (env, client, admin, merchant, token_addr, token_client, token_admin_client)
}

fn pay_for(
    client: &AhjoorPaymentsContractClient,
    payer: &Address,
    merchant: &Address,
    token: &Address,
    amount: i128,
    beneficiary: Option<Address>,
) -> u32 {
    client.create_payment_with_options(
        payer,
        merchant,
        &amount,
        token,
        &None,
        &None,
        &None,
        &None,
        &None,
        &beneficiary,
    )
}

#[test]
fn test_beneficiary_gets_loyalty_points_and_record() {
    let (env, client, _admin, merchant, token, _tc, tac) = setup();
    let payer = Address::generate(&env);
    let child = Address::generate(&env);
    tac.mint(&payer, &10_000_000);

    let pid = pay_for(&client, &payer, &merchant, &token, 5_000_000, Some(child.clone()));
    client.complete_payment(&pid);

    assert_eq!(client.get_payment_beneficiary(&pid), child);
    assert_eq!(client.get_payment(&pid).customer, payer);
    assert_eq!(client.get_loyalty_points_balance(&child), 5);
    assert_eq!(client.get_loyalty_points_balance(&payer), 0);
    assert_eq!(client.get_customer_payments(&child).len(), 1);
    assert_eq!(client.get_customer_payments(&payer).len(), 0);
}

#[test]
fn test_no_beneficiary_defaults_to_payer() {
    let (env, client, _admin, merchant, token, _tc, tac) = setup();
    let payer = Address::generate(&env);
    tac.mint(&payer, &10_000_000);

    let pid = pay_for(&client, &payer, &merchant, &token, 2_000_000, None);
    client.complete_payment(&pid);

    assert_eq!(client.get_payment_beneficiary(&pid), payer);
    assert_eq!(client.get_loyalty_points_balance(&payer), 2);
    assert_eq!(client.get_customer_payments(&payer).len(), 1);
}

#[test]
fn test_beneficiary_equal_to_payer_is_same_as_none() {
    let (env, client, _admin, merchant, token, _tc, tac) = setup();
    let payer = Address::generate(&env);
    tac.mint(&payer, &10_000_000);

    let pid = pay_for(&client, &payer, &merchant, &token, 2_000_000, Some(payer.clone()));
    client.complete_payment(&pid);

    assert_eq!(client.get_payment_beneficiary(&pid), payer);
    assert_eq!(client.get_customer_payments(&payer).len(), 1);
}

#[test]
fn test_refund_goes_to_payer_and_reverses_beneficiary_points() {
    let (env, client, _admin, merchant, token, tc, tac) = setup();
    let payer = Address::generate(&env);
    let child = Address::generate(&env);
    tac.mint(&payer, &10_000_000);

    // Earn points on a completed payment for the beneficiary.
    let done = pay_for(&client, &payer, &merchant, &token, 4_000_000, Some(child.clone()));
    client.complete_payment(&done);
    assert_eq!(client.get_loyalty_points_balance(&child), 4);

    // Pending payment refunded: tokens return to the payer.
    let pid = pay_for(&client, &payer, &merchant, &token, 2_000_000, Some(child.clone()));
    let before = tc.balance(&payer);
    client.partial_refund(&pid, &2_000_000);
    assert_eq!(tc.balance(&payer), before + 2_000_000);
    assert_eq!(tc.balance(&child), 0);
    // Points are reversed from the beneficiary, never the payer.
    assert_eq!(client.get_loyalty_points_balance(&payer), 0);
    assert!(client.get_loyalty_points_balance(&child) <= 4);
}

#[test]
fn test_spend_limit_applies_to_payer_not_beneficiary() {
    let (env, client, _admin, merchant, token, _tc, tac) = setup();
    let payer = Address::generate(&env);
    let child = Address::generate(&env);
    tac.mint(&payer, &10_000_000);

    // Limit on the payer blocks the payment even though the beneficiary has none.
    client.set_customer_spend_limit(&merchant, &payer, &1_000, &3600u64);
    let pid = pay_for(&client, &payer, &merchant, &token, 2_000, Some(child.clone()));
    assert!(client.try_complete_payment(&pid).is_err());
}

#[test]
fn test_beneficiary_spend_limit_does_not_affect_payer_payment() {
    let (env, client, _admin, merchant, token, _tc, tac) = setup();
    let payer = Address::generate(&env);
    let child = Address::generate(&env);
    tac.mint(&payer, &10_000_000);

    // A limit on the beneficiary is irrelevant: the payer is who spends.
    client.set_customer_spend_limit(&merchant, &child, &1_000, &3600u64);
    let pid = pay_for(&client, &payer, &merchant, &token, 2_000, Some(child));
    client.complete_payment(&pid);
    assert_eq!(client.get_payment(&pid).status, PaymentStatus::Completed);
}
