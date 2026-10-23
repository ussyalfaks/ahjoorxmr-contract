#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

struct TestSetup<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    sponsor: Address,
}

fn setup<'a>() -> TestSetup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);

    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let sponsor = Address::generate(&env);
    token_admin_client.mint(&buyer, &1_000);
    token_admin_client.mint(&sponsor, &1_000);

    TestSetup {
        env,
        client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
        buyer,
        seller,
        arbiter,
        sponsor,
    }
}

fn create_escrow(s: &TestSetup) -> u32 {
    let deadline = s.env.ledger().timestamp() + 1_000;
    s.client.create_escrow(
        &s.buyer,
        &s.seller,
        &s.arbiter,
        &1_000,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    )
}

fn set_fee(s: &TestSetup, bps: u32) {
    let fee_recipient = Address::generate(&s.env);
    s.client.update_protocol_fee(&s.admin, &bps, &fee_recipient);
}

#[test]
fn test_sponsor_escrow_fee_records_sponsorship() {
    let s = setup();
    set_fee(&s, 100); // 1%
    let escrow_id = create_escrow(&s);

    let amount = s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);
    assert_eq!(amount, 10);
    assert_eq!(s.token_client.balance(&s.sponsor), 990);

    let sponsorship = s.client.get_fee_sponsorship(&escrow_id).unwrap();
    assert_eq!(sponsorship.sponsor, s.sponsor);
    assert_eq!(sponsorship.amount, 10);
    assert!(!sponsorship.settled);
}

#[test]
fn test_sponsoring_twice_rejected() {
    let s = setup();
    set_fee(&s, 100);
    let escrow_id = create_escrow(&s);

    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);
    let other = Address::generate(&s.env);
    s.token_admin_client.mint(&other, &1_000);
    assert!(s.client.try_sponsor_escrow_fee(&other, &escrow_id).is_err());
}

#[test]
fn test_sponsoring_without_fee_rejected() {
    let s = setup();
    let escrow_id = create_escrow(&s);
    assert!(s.client.try_sponsor_escrow_fee(&s.sponsor, &escrow_id).is_err());
}

#[test]
fn test_sponsored_release_pays_seller_full_amount() {
    let s = setup();
    set_fee(&s, 100);
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);

    s.client.release_escrow(&s.buyer, &escrow_id);

    assert_eq!(s.token_client.balance(&s.seller), 1_000);
    assert_eq!(s.client.get_accrued_fees(&s.token_addr), 10);
    assert!(s.client.get_fee_sponsorship(&escrow_id).unwrap().settled);
}

#[test]
fn test_sponsored_dispute_verdict_takes_fee_from_sponsorship() {
    let s = setup();
    set_fee(&s, 100);
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);

    s.client.dispute_escrow(
        &s.buyer,
        &escrow_id,
        &String::from_str(&s.env, "Issue"),
        &1_000,
    );
    s.client.resolve_dispute(&s.arbiter, &escrow_id, &0u32, &soroban_sdk::BytesN::from_array(&s.client.env, &[0u8; 32]));

    assert_eq!(s.token_client.balance(&s.seller), 1_000);
    assert_eq!(s.client.get_accrued_fees(&s.token_addr), 10);
}

#[test]
fn test_partial_release_keeps_sponsorship_until_final_release() {
    let s = setup();
    set_fee(&s, 100);
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);

    s.client.partial_release(&s.buyer, &escrow_id, &400);
    assert_eq!(s.token_client.balance(&s.seller), 400);
    assert!(!s.client.get_fee_sponsorship(&escrow_id).unwrap().settled);

    s.client.partial_release(&s.buyer, &escrow_id, &600);
    assert_eq!(s.token_client.balance(&s.seller), 1_000);
    assert!(s.client.get_fee_sponsorship(&escrow_id).unwrap().settled);
    assert_eq!(s.client.get_accrued_fees(&s.token_addr), 10);
}

#[test]
fn test_sponsor_refunded_on_expired_refund() {
    let s = setup();
    set_fee(&s, 100);
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);

    s.env.ledger().with_mut(|li| li.timestamp += 1_001);
    s.client.auto_release_expired(&escrow_id);

    assert_eq!(s.token_client.balance(&s.sponsor), 1_000);
    assert_eq!(s.token_client.balance(&s.buyer), 1_000);
    assert!(s.client.get_fee_sponsorship(&escrow_id).unwrap().settled);
}

#[test]
fn test_sponsor_refunded_on_mutual_cancellation() {
    let s = setup();
    set_fee(&s, 100);
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);

    s.client.request_cancellation(
        &s.buyer,
        &escrow_id,
        &BytesN::from_array(&s.env, &[1u8; 32]),
    );
    s.client.accept_cancellation(&s.seller, &escrow_id);

    assert_eq!(s.token_client.balance(&s.sponsor), 1_000);
}

#[test]
fn test_sponsor_refunded_on_full_buyer_verdict() {
    let s = setup();
    set_fee(&s, 100);
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);

    s.client.dispute_escrow(
        &s.buyer,
        &escrow_id,
        &String::from_str(&s.env, "Not delivered"),
        &1_000,
    );
    s.client.resolve_dispute(&s.arbiter, &escrow_id, &100u32, &soroban_sdk::BytesN::from_array(&s.client.env, &[0u8; 32]));

    assert_eq!(s.token_client.balance(&s.sponsor), 1_000);
}

#[test]
fn test_fee_decrease_returns_excess_to_sponsor() {
    let s = setup();
    set_fee(&s, 200); // 2% → sponsor pays 20
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);
    assert_eq!(s.token_client.balance(&s.sponsor), 980);

    set_fee(&s, 100); // fee drops to 10
    s.client.release_escrow(&s.buyer, &escrow_id);

    assert_eq!(s.token_client.balance(&s.seller), 1_000);
    assert_eq!(s.client.get_accrued_fees(&s.token_addr), 10);
    assert_eq!(s.token_client.balance(&s.sponsor), 990);
}

#[test]
fn test_fee_increase_shortfall_falls_back_to_escrow_deduction() {
    let s = setup();
    set_fee(&s, 100); // sponsor pays 10
    let escrow_id = create_escrow(&s);
    s.client.sponsor_escrow_fee(&s.sponsor, &escrow_id);

    set_fee(&s, 200); // fee rises to 20; sponsor is not charged more
    s.client.dispute_escrow(
        &s.buyer,
        &escrow_id,
        &String::from_str(&s.env, "Issue"),
        &1_000,
    );
    s.client.resolve_dispute(&s.arbiter, &escrow_id, &0u32, &soroban_sdk::BytesN::from_array(&s.client.env, &[0u8; 32]));

    // 10 covered by sponsor, 10 shortfall deducted from the escrow amount.
    assert_eq!(s.token_client.balance(&s.sponsor), 990);
    assert_eq!(s.token_client.balance(&s.seller), 990);
    assert_eq!(s.client.get_accrued_fees(&s.token_addr), 20);
}
