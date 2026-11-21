#![cfg(test)]
//! #1183: payments consults the whitelist's swap pair allowlist before routing.
use crate::{AhjoorPaymentsContract, AhjoorPaymentsContractClient, PaymentStatus};
use ahjoor_token_whitelist::{TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::{
    testutils::Address as _,
    token::{Client as TokenClient, StellarAssetClient as TokenAdminClient},
    Address, Env,
};

struct Setup<'a> {
    env: Env,
    admin: Address,
    customer: Address,
    merchant: Address,
    router: Address,
    client: AhjoorPaymentsContractClient<'a>,
    whitelist: TokenWhitelistContractClient<'a>,
    token_a: Address,
    token_b: Address,
}

fn new_token(env: &Env, holder: &Address) -> Address {
    let addr = env
        .register_stellar_asset_contract_v2(Address::generate(env))
        .address();
    TokenAdminClient::new(env, &addr).mint(holder, &10_000);
    addr
}

/// Merchant prefers `token_b`; customer pays in `token_a`; router holds `token_b`.
fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let router = Address::generate(&env);

    let client = AhjoorPaymentsContractClient::new(&env, &env.register(AhjoorPaymentsContract, ()));
    client.initialize(&admin, &Address::generate(&env), &0);
    let whitelist =
        TokenWhitelistContractClient::new(&env, &env.register(TokenWhitelistContract, ()));
    whitelist.initialize(&admin);
    client.set_token_whitelist_contract(&admin, &whitelist.address);

    let token_a = new_token(&env, &customer);
    let token_b = new_token(&env, &router);
    whitelist.add_token(&admin, &token_a);
    whitelist.add_token(&admin, &token_b);

    client.set_preferred_token(&merchant, &token_b);
    client.set_swap_router(&admin, &router);

    Setup { env, admin, customer, merchant, router, client, whitelist, token_a, token_b }
}

fn pay(s: &Setup) -> u32 {
    s.client
        .create_payment(&s.customer, &s.merchant, &100, &s.token_a, &None, &None, &None)
}

#[test]
fn test_swap_refused_when_pair_not_allowlisted() {
    let s = setup();
    let id = pay(&s);
    s.client.complete_payment(&id);

    assert_eq!(s.client.get_payment(&id).status, PaymentStatus::Refunded);
    assert_eq!(TokenClient::new(&s.env, &s.token_a).balance(&s.customer), 10_000);
    assert_eq!(TokenClient::new(&s.env, &s.token_a).balance(&s.router), 0);
    assert_eq!(TokenClient::new(&s.env, &s.token_b).balance(&s.merchant), 0);
}

/// Routing decision only: the downstream `finalize_payment` settlement path is
/// exercised by the refusal tests above.
fn route(s: &Setup) -> Result<i128, soroban_sdk::Symbol> {
    // Fund the payments contract as if it already escrowed the customer's payment.
    TokenAdminClient::new(&s.env, &s.token_a).mint(&s.client.address, &100);
    s.env.as_contract(&s.client.address, || {
        AhjoorPaymentsContract::execute_token_swap(
            &s.env, 0, &s.customer, &s.token_a, &s.token_b, 100,
        )
    })
}

#[test]
fn test_swap_routed_when_pair_allowlisted() {
    let s = setup();
    s.env.mock_all_auths_allowing_non_root_auth();
    s.whitelist.allow_swap_pair(&s.admin, &s.token_b, &s.token_a, &500);
    assert_eq!(route(&s), Ok(100));
    assert_eq!(TokenClient::new(&s.env, &s.token_a).balance(&s.router), 100);
    assert_eq!(TokenClient::new(&s.env, &s.token_b).balance(&s.client.address), 100);
}

#[test]
fn test_route_returns_pair_not_allowed_error() {
    let s = setup();
    assert_eq!(
        route(&s),
        Err(soroban_sdk::Symbol::new(&s.env, "SwapPairNotAllowed"))
    );
    // Nothing moved to the router.
    assert_eq!(TokenClient::new(&s.env, &s.token_a).balance(&s.router), 0);
}

#[test]
fn test_swap_refused_after_pair_disallowed() {
    let s = setup();
    s.whitelist.allow_swap_pair(&s.admin, &s.token_a, &s.token_b, &500);
    s.whitelist.disallow_swap_pair(&s.admin, &s.token_a, &s.token_b);
    let id = pay(&s);
    s.client.complete_payment(&id);
    assert_eq!(s.client.get_payment(&id).status, PaymentStatus::Refunded);
}
