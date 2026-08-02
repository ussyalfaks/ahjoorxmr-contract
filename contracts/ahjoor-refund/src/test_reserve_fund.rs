#![cfg(test)]
extern crate std;

use soroban_sdk::{testutils::Address as _, Address, Env};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;

use crate::{AhjoorRefundContract, AhjoorRefundContractClient, RefundInitConfig};

fn setup(env: &Env) -> (AhjoorRefundContractClient<'static>, Address, Address) {
    let contract_id = env.register(AhjoorRefundContract, ());
    let client = AhjoorRefundContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let payment_contract = Address::generate(env); // mock
    client.initialize(
        &admin,
        &payment_contract,
        &86400u64,
        &None::<RefundInitConfig>,
    );
    (client, admin, payment_contract)
}

fn make_token<'a>(env: &'a Env, admin: &Address) -> (Address, TokenAdminClient<'a>) {
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin = TokenAdminClient::new(env, &token_addr);
    (token_addr, token_admin)
}

#[test]
fn test_deposit_and_withdraw_reserve() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);
    let (token_addr, token_admin) = make_token(&env, &admin);

    token_admin.mint(&merchant, &2000i128);

    // Set ratio
    client.set_reserve_ratio_bps(&admin, &200u32);

    // Deposit
    client.deposit_reserve(&merchant, &token_addr, &1000i128);
    assert_eq!(client.get_merchant_reserve(&merchant), 1000i128);

    // Withdraw within allowed amount (no volume recorded, required = 0)
    client.withdraw_reserve(&merchant, &token_addr, &500i128);
    assert_eq!(client.get_merchant_reserve(&merchant), 500i128);
}

#[test]
#[should_panic(expected = "WithdrawalWouldBreachMinimum")]
fn test_withdraw_below_minimum_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);
    let (token_addr, token_admin) = make_token(&env, &admin);

    token_admin.mint(&merchant, &2000i128);

    client.set_reserve_ratio_bps(&admin, &200u32);
    // Deposit 100 tokens
    client.deposit_reserve(&merchant, &token_addr, &100i128);
    // Record volume=10000 → required=200, but balance=100 → already below required
    // Attempting to withdraw 50 more should breach minimum
    client.record_payment_volume(&merchant, &10_000i128);
    client.withdraw_reserve(&merchant, &token_addr, &50i128);
}

#[test]
fn test_compliance_check_flags_merchant() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);

    client.set_reserve_ratio_bps(&admin, &200u32);
    // Volume = 10000, required = 200, reserve = 0 → non-compliant
    client.record_payment_volume(&merchant, &10_000i128);
    let compliant = client.check_reserve_compliance(&admin, &merchant);
    assert!(!compliant);
}

#[test]
fn test_compliance_check_passes_when_funded() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);

    client.set_reserve_ratio_bps(&admin, &200u32);
    // No volume → required = 0 → always compliant
    let compliant = client.check_reserve_compliance(&admin, &merchant);
    assert!(compliant);
}

// --- #585: Reserve subsystem reconciliation ---

#[test]
fn test_migrate_merchant_reserve_moves_legacy_into_canonical() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);
    let (token_addr, token_admin) = make_token(&env, &admin);
    let token_client = TokenClient::new(&env, &token_addr);

    token_admin.mint(&merchant, &2000i128);

    // Legacy (#274) balance via deposit_reserve.
    client.deposit_reserve(&merchant, &token_addr, &300i128);
    assert_eq!(client.get_merchant_reserve(&merchant), 300i128);

    // Canonical (#334) balance via deposit_merchant_reserve. Reserve config is
    // seeded directly rather than via `set_reserve_config`, which calls
    // `admin.require_auth()` twice (once directly, once inside `require_admin`)
    // and trips soroban-sdk's mock_all_auths duplicate-authorization check —
    // a pre-existing issue in that setter, unrelated to #585.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&crate::DataKey2::ReserveToken, &token_addr);
    });
    token_client.approve(
        &merchant,
        &client.address,
        &200i128,
        &(env.ledger().sequence() + 1000),
    );
    client.deposit_merchant_reserve(&merchant, &200i128);

    let migrated = client.migrate_merchant_reserve(&admin, &merchant);

    // Total preserved: 300 (legacy) + 200 (canonical) = 500, all now canonical.
    assert_eq!(migrated, 300i128);
    assert_eq!(client.get_merchant_reserve(&merchant), 0i128);

    let key = crate::DataKey2::MerchantReserveBalance(merchant.clone());
    let canonical_balance: i128 = env.as_contract(&client.address, || {
        env.storage().persistent().get(&key).unwrap_or(0)
    });
    assert_eq!(canonical_balance, 500i128);
}

#[test]
fn test_migrate_merchant_reserve_no_legacy_balance_is_noop() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _) = setup(&env);
    let merchant = Address::generate(&env);

    let migrated = client.migrate_merchant_reserve(&admin, &merchant);
    assert_eq!(migrated, 0i128);
}
