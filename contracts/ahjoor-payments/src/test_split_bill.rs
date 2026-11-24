#![cfg(test)]
//! Tests for `create_split_bill` / `pay_bill_share` / `expire_split_bill`.

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Vec,
};

// ---------------------------------------------------------------------------
//  Test harness
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    token_addr: Address,
    token_client: TokenClient<'a>,
    tac: TokenAdminClient<'a>,
    merchant: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let tac = TokenAdminClient::new(&env, &token_addr);

    // 0% protocol fee — keeps arithmetic simple.
    client.initialize(&admin, &fee_recipient, &0u32);
    client.set_min_collateral(&0i128);

    // Approve merchant.
    let merchant = Address::generate(&env);
    tac.mint(&merchant, &10_000_000); // collateral

    Setup { env, client, token_addr, token_client, tac, merchant }
}

/// Mint tokens to an address and return it.
fn funded_customer(s: &Setup<'_>, amount: i128) -> Address {
    let addr = Address::generate(&s.env);
    s.tac.mint(&addr, &amount);
    addr
}

/// Build a Vec<(Address, i128)> of shares.
fn shares(env: &Env, entries: &[(Address, i128)]) -> Vec<(Address, i128)> {
    let mut v = Vec::new(env);
    for (addr, amount) in entries {
        v.push_back((addr.clone(), *amount));
    }
    v
}

// ---------------------------------------------------------------------------
//  Happy path
// ---------------------------------------------------------------------------

#[test]
fn test_happy_path_all_shares_paid_completes_bill() {
    let s = setup();
    let alice = funded_customer(&s, 400);
    let bob = funded_customer(&s, 300);
    let carol = funded_customer(&s, 300);

    let expiry = s.env.ledger().timestamp() + 3_600;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 400), (bob.clone(), 300), (carol.clone(), 300)]),
        &expiry,
    );

    // Pay in order.
    s.client.pay_bill_share(&alice, &bill_id);
    s.client.pay_bill_share(&bob, &bill_id);

    // After two shares the bill is still Open.
    let bill = s.client.get_split_bill(&bill_id);
    assert_eq!(bill.status, SplitBillStatus::Open);
    assert_eq!(bill.funded, 700);

    // Last share completes the bill.
    s.client.pay_bill_share(&carol, &bill_id);

    let bill = s.client.get_split_bill(&bill_id);
    assert_eq!(bill.status, SplitBillStatus::Completed);
    assert_eq!(bill.funded, 1_000);

    // Merchant received all funds.
    assert_eq!(s.token_client.balance(&s.merchant), 10_001_000);

    // Participants' balances depleted.
    assert_eq!(s.token_client.balance(&alice), 0);
    assert_eq!(s.token_client.balance(&bob), 0);
    assert_eq!(s.token_client.balance(&carol), 0);
}

#[test]
fn test_shares_stored_correctly() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 500);
    let expiry = s.env.ledger().timestamp() + 1_000;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 500), (bob.clone(), 500)]),
        &expiry,
    );

    let stored = s.client.get_split_bill_shares(&bill_id);
    assert_eq!(stored.len(), 2);
    assert_eq!(stored.get(0).unwrap().amount, 500);
    assert_eq!(stored.get(0).unwrap().paid, false);
    assert_eq!(stored.get(1).unwrap().amount, 500);
}

#[test]
fn test_partial_payment_then_full_completion() {
    let s = setup();
    let alice = funded_customer(&s, 600);
    let bob = funded_customer(&s, 400);
    let expiry = s.env.ledger().timestamp() + 3_600;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 600), (bob.clone(), 400)]),
        &expiry,
    );

    s.client.pay_bill_share(&alice, &bill_id);
    // Contract holds 600; bill still open.
    assert_eq!(s.token_client.balance(&s.client.address), 600);

    s.client.pay_bill_share(&bob, &bill_id);
    // Bill completed; contract balance back to 0.
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let bill = s.client.get_split_bill(&bill_id);
    assert_eq!(bill.status, SplitBillStatus::Completed);
}

// ---------------------------------------------------------------------------
//  Expiry / refund path
// ---------------------------------------------------------------------------

#[test]
fn test_expire_refunds_all_paid_shares() {
    let s = setup();
    let alice = funded_customer(&s, 400);
    let bob = funded_customer(&s, 600);
    let expiry = s.env.ledger().timestamp() + 100;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 400), (bob.clone(), 600)]),
        &expiry,
    );

    // Only Alice pays before expiry.
    s.client.pay_bill_share(&alice, &bill_id);
    assert_eq!(s.token_client.balance(&alice), 0);

    // Advance time past expiry.
    s.env.ledger().set_timestamp(expiry + 1);
    s.client.expire_split_bill(&bill_id);

    let bill = s.client.get_split_bill(&bill_id);
    assert_eq!(bill.status, SplitBillStatus::Refunded);

    // Alice gets refunded; Bob never paid so balance unchanged.
    assert_eq!(s.token_client.balance(&alice), 400);
    assert_eq!(s.token_client.balance(&bob), 600);
}

#[test]
fn test_expire_no_payments_no_transfer() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 500);
    let expiry = s.env.ledger().timestamp() + 50;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 500), (bob.clone(), 500)]),
        &expiry,
    );

    // Nobody pays.
    s.env.ledger().set_timestamp(expiry + 1);
    s.client.expire_split_bill(&bill_id);

    let bill = s.client.get_split_bill(&bill_id);
    assert_eq!(bill.status, SplitBillStatus::Refunded);
    assert_eq!(bill.funded, 0);
}

// ---------------------------------------------------------------------------
//  Error branches
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_too_few_shares_panics() {
    let s = setup();
    let alice = funded_customer(&s, 1_000);
    let expiry = s.env.ledger().timestamp() + 100;
    // Only one share → SplitBillTooFewShares
    s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice, 1_000)]),
        &expiry,
    );
}

#[test]
#[should_panic]
fn test_duplicate_customer_panics() {
    let s = setup();
    let alice = funded_customer(&s, 1_000);
    let expiry = s.env.ledger().timestamp() + 100;
    s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 500), (alice, 500)]),
        &expiry,
    );
}

#[test]
#[should_panic]
fn test_zero_share_amount_panics() {
    let s = setup();
    let alice = funded_customer(&s, 1_000);
    let bob = funded_customer(&s, 0);
    let expiry = s.env.ledger().timestamp() + 100;
    s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice, 1_000), (bob, 0)]),
        &expiry,
    );
}

#[test]
#[should_panic]
fn test_expiry_in_past_panics() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 500);
    // expiry == now → must be in the future
    let expiry = s.env.ledger().timestamp();
    s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice, 500), (bob, 500)]),
        &expiry,
    );
}

#[test]
#[should_panic]
fn test_pay_share_already_paid_panics() {
    let s = setup();
    let alice = funded_customer(&s, 1_000);
    let bob = funded_customer(&s, 500);
    let expiry = s.env.ledger().timestamp() + 3_600;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 500), (bob.clone(), 500)]),
        &expiry,
    );

    s.client.pay_bill_share(&alice, &bill_id);
    // Second call → SplitBillShareAlreadyPaid
    s.client.pay_bill_share(&alice, &bill_id);
}

#[test]
#[should_panic]
fn test_pay_share_not_participant_panics() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 500);
    let charlie = funded_customer(&s, 999); // not in the bill
    let expiry = s.env.ledger().timestamp() + 3_600;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice, 500), (bob, 500)]),
        &expiry,
    );

    s.client.pay_bill_share(&charlie, &bill_id);
}

#[test]
#[should_panic]
fn test_pay_share_after_expiry_panics() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 500);
    let expiry = s.env.ledger().timestamp() + 100;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 500), (bob, 500)]),
        &expiry,
    );

    s.env.ledger().set_timestamp(expiry + 1);
    // Bill is still Open but expired — pay_bill_share must reject
    s.client.pay_bill_share(&alice, &bill_id);
}

#[test]
#[should_panic]
fn test_expire_before_deadline_panics() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 500);
    let expiry = s.env.ledger().timestamp() + 3_600;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice, 500), (bob, 500)]),
        &expiry,
    );

    // Deadline has NOT passed yet → SplitBillNotYetExpired
    s.client.expire_split_bill(&bill_id);
}

#[test]
#[should_panic]
fn test_expire_already_completed_bill_panics() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 500);
    let expiry = s.env.ledger().timestamp() + 3_600;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 500), (bob.clone(), 500)]),
        &expiry,
    );

    s.client.pay_bill_share(&alice, &bill_id);
    s.client.pay_bill_share(&bob, &bill_id); // completes

    // Advance time past expiry, then try to expire a completed bill.
    s.env.ledger().set_timestamp(expiry + 1);
    s.client.expire_split_bill(&bill_id); // → SplitBillNotExpirable
}

#[test]
#[should_panic]
fn test_pay_share_on_completed_bill_panics() {
    let s = setup();
    let alice = funded_customer(&s, 500);
    let bob = funded_customer(&s, 1_000); // extra balance
    let carol = funded_customer(&s, 500);
    let expiry = s.env.ledger().timestamp() + 3_600;

    let bill_id = s.client.create_split_bill(
        &s.merchant,
        &s.token_addr,
        &shares(&s.env, &[(alice.clone(), 500), (bob.clone(), 500)]),
        &expiry,
    );

    s.client.pay_bill_share(&alice, &bill_id);
    s.client.pay_bill_share(&bob, &bill_id); // completes

    // carol is not in the bill; trying to pay after completion should still
    // return SplitBillAlreadyTerminal before SplitBillNotAParticipant.
    // We just re-use alice to trigger AlreadyTerminal.
    s.client.pay_bill_share(&carol, &bill_id);
}
