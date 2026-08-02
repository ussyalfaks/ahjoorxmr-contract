#![cfg(test)]
use super::*;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, AhjoorPaymentsContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    client.initialize(&admin, &fee_recipient, &0);

    (env, client, admin, token_addr)
}

fn disputed_payment(
    env: &Env,
    client: &AhjoorPaymentsContractClient<'static>,
    token_addr: &Address,
    customer: &Address,
    merchant: &Address,
) -> u32 {
    let token_admin = TokenAdminClient::new(env, token_addr);
    token_admin.mint(customer, &1_000_000);

    let payment_id = client.create_payment(
        customer, merchant, &500_000, token_addr, &None, &None, &None,
    );
    client.dispute_payment(
        customer,
        &payment_id,
        &String::from_str(env, "Goods not received"),
    );
    payment_id
}

#[test]
fn test_returns_only_given_merchants_disputes() {
    let (env, client, _admin, token_addr) = setup();
    let customer = Address::generate(&env);
    let merchant_a = Address::generate(&env);
    let merchant_b = Address::generate(&env);

    let pid_a = disputed_payment(&env, &client, &token_addr, &customer, &merchant_a);
    let _pid_b = disputed_payment(&env, &client, &token_addr, &customer, &merchant_b);

    let disputes_a = client.list_disputes_by_merchant(&merchant_a, &0, &10);
    assert_eq!(disputes_a.len(), 1);
    assert_eq!(disputes_a.get(0).unwrap().payment_id, pid_a);
}

#[test]
fn test_merchant_with_no_disputes_returns_empty_vec() {
    let (env, client, _admin, _token_addr) = setup();
    let merchant = Address::generate(&env);

    let disputes = client.list_disputes_by_merchant(&merchant, &0, &10);
    assert_eq!(disputes.len(), 0);
}

#[test]
fn test_pagination_spans_multiple_pages() {
    let (env, client, _admin, token_addr) = setup();
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let mut expected_ids = soroban_sdk::Vec::new(&env);
    for _ in 0..5 {
        expected_ids.push_back(disputed_payment(
            &env,
            &client,
            &token_addr,
            &customer,
            &merchant,
        ));
    }

    // Page through 2 at a time.
    let page_0 = client.list_disputes_by_merchant(&merchant, &0, &2);
    let page_1 = client.list_disputes_by_merchant(&merchant, &2, &2);
    let page_2 = client.list_disputes_by_merchant(&merchant, &4, &2);

    assert_eq!(page_0.len(), 2);
    assert_eq!(page_1.len(), 2);
    assert_eq!(page_2.len(), 1);

    let mut collected: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(&env);
    for d in page_0.iter() {
        collected.push_back(d.payment_id);
    }
    for d in page_1.iter() {
        collected.push_back(d.payment_id);
    }
    for d in page_2.iter() {
        collected.push_back(d.payment_id);
    }

    assert_eq!(collected, expected_ids);
}
