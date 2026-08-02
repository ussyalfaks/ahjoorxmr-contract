#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env};

fn setup_referral<'a>() -> (Env, AhjoorPaymentsContractClient<'a>, Address, Address, Address, TokenClient<'a>, TokenAdminClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    // Initialize with 100 bps (1%) fee so commission can accrue
    client.initialize(&admin, &fee_recipient, &100u32);
    client.set_min_collateral(&0i128);

    (env, client, admin, fee_recipient, token_addr, token_client, token_admin_client)
}

// ---------------------------------------------------------------------------
// Test: commission accrues on referred merchant payment
// ---------------------------------------------------------------------------
#[test]
fn test_commission_accrues() {
    let (env, client, admin, _fee_recipient, token_addr, _tc, tac) = setup_referral();

    let referrer = Address::generate(&env);
    let referred = Address::generate(&env);
    let customer = Address::generate(&env);

    // Approve referrer and referred
    client.approve_merchant(&referrer);
    // Set referral config: 1000 bps (10% of fee), 10000 ledger window
    client.set_referral_config(&admin, &1000u32, &10_000u32);
    // Register referral before approving referred
    client.register_referral(&referrer, &referred);
    client.approve_merchant(&referred);

    tac.mint(&customer, &10_000);
    let pid = client.create_payment(&customer, &referred, &1000, &token_addr, &None, &None, &None);
    client.complete_payment(&pid);

    // fee = 1000 * 100 / 10000 = 10; commission = 10 * 1000 / 10000 = 1
    let pending = client.get_pending_commission(&referrer);
    assert!(pending > 0);
}

// ---------------------------------------------------------------------------
// Test: window expiry stops accrual
// ---------------------------------------------------------------------------
#[test]
fn test_window_expiry_stops_accrual() {
    let (env, client, admin, _fee_recipient, token_addr, _tc, tac) = setup_referral();

    let referrer = Address::generate(&env);
    let referred = Address::generate(&env);
    let customer = Address::generate(&env);

    client.approve_merchant(&referrer);
    // Very short window: 1 ledger
    client.set_referral_config(&admin, &1000u32, &1u32);
    client.register_referral(&referrer, &referred);
    client.approve_merchant(&referred);

    // Advance ledger past window
    env.ledger().with_mut(|l| l.sequence_number += 100);

    tac.mint(&customer, &10_000);
    let pid = client.create_payment(&customer, &referred, &1000, &token_addr, &None, &None, &None);
    client.complete_payment(&pid);

    // No commission should have accrued
    assert_eq!(client.get_pending_commission(&referrer), 0);
}

// ---------------------------------------------------------------------------
// Test: claim transfers commission to referrer
// ---------------------------------------------------------------------------
#[test]
fn test_claim_commission() {
    let (env, client, admin, _fee_recipient, token_addr, _tc, tac) = setup_referral();

    let referrer = Address::generate(&env);
    let referred = Address::generate(&env);
    let customer = Address::generate(&env);

    client.approve_merchant(&referrer);
    client.set_referral_config(&admin, &1000u32, &10_000u32);
    client.register_referral(&referrer, &referred);
    client.approve_merchant(&referred);

    tac.mint(&customer, &10_000);
    let pid = client.create_payment(&customer, &referred, &1000, &token_addr, &None, &None, &None);
    client.complete_payment(&pid);

    let pending = client.get_pending_commission(&referrer);
    assert!(pending > 0);

    // Mint contract enough to pay out (fee was already transferred to fee_recipient, not contract)
    // In test: contract holds the commission amount from the fee transfer
    // We just verify claim clears the balance
    // (actual token transfer tested via integration; here we verify state)
    let before = pending;
    // Claim — will transfer from contract to referrer
    // Contract must hold the tokens; in real flow fee goes to fee_recipient not contract
    // For test purposes, mint to contract address
    tac.mint(&client.address, &before);
    client.claim_referral_commission(&referrer, &token_addr);
    assert_eq!(client.get_pending_commission(&referrer), 0);
}

// ---------------------------------------------------------------------------
// Test: double-referral rejected
// ---------------------------------------------------------------------------
#[test]
fn test_double_referral_rejected() {
    let (_env, client, admin, _fee_recipient, _token_addr, _tc, _tac) = setup_referral();

    let referrer = Address::generate(&_env);
    let referred = Address::generate(&_env);

    client.approve_merchant(&referrer);
    client.set_referral_config(&admin, &1000u32, &10_000u32);
    // Approve referred first — now they have a merchant record
    client.approve_merchant(&referred);

    // Registering referral for an already-approved merchant should fail
    let result = client.try_register_referral(&referrer, &referred);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Test: zero commission config accrues nothing
// ---------------------------------------------------------------------------
#[test]
fn test_zero_commission_config() {
    let (env, client, admin, _fee_recipient, token_addr, _tc, tac) = setup_referral();

    let referrer = Address::generate(&env);
    let referred = Address::generate(&env);
    let customer = Address::generate(&env);

    client.approve_merchant(&referrer);
    client.set_referral_config(&admin, &0u32, &10_000u32); // 0 bps
    client.register_referral(&referrer, &referred);
    client.approve_merchant(&referred);

    tac.mint(&customer, &10_000);
    let pid = client.create_payment(&customer, &referred, &1000, &token_addr, &None, &None, &None);
    client.complete_payment(&pid);

    assert_eq!(client.get_pending_commission(&referrer), 0);
}

// ---------------------------------------------------------------------------
// Test: commission summary reflects multiple referrals with partial claims
// ---------------------------------------------------------------------------
#[test]
fn test_referral_commission_summary() {
    let (env, client, admin, _fee_recipient, token_addr, _tc, tac) = setup_referral();

    let referrer = Address::generate(&env);
    let referred_a = Address::generate(&env);
    let referred_b = Address::generate(&env);
    let customer = Address::generate(&env);

    // Referrer with no activity returns (0, 0)
    assert_eq!(
        client.get_referral_commission_summary(&referrer),
        (0i128, 0i128)
    );

    client.approve_merchant(&referrer);
    client.set_referral_config(&admin, &1000u32, &10_000u32);
    client.register_referral(&referrer, &referred_a);
    client.approve_merchant(&referred_a);
    client.register_referral(&referrer, &referred_b);
    client.approve_merchant(&referred_b);

    tac.mint(&customer, &100_000);

    // Payment through first referred merchant
    let pid_a = client.create_payment(&customer, &referred_a, &1000, &token_addr, &None, &None, &None);
    client.complete_payment(&pid_a);

    // Payment through second referred merchant
    let pid_b = client.create_payment(&customer, &referred_b, &1000, &token_addr, &None, &None, &None);
    client.complete_payment(&pid_b);

    let (total_earned, total_claimed) = client.get_referral_commission_summary(&referrer);
    assert!(total_earned > 0);
    assert_eq!(total_claimed, 0);
    assert_eq!(total_earned, client.get_pending_commission(&referrer));

    // Partial claim: fund the contract and claim once
    let pending = client.get_pending_commission(&referrer);
    tac.mint(&client.address, &pending);
    client.claim_referral_commission(&referrer, &token_addr);

    let (total_earned_after_claim, total_claimed_after_claim) =
        client.get_referral_commission_summary(&referrer);
    assert_eq!(total_earned_after_claim, total_earned);
    assert_eq!(total_claimed_after_claim, pending);
    assert_eq!(client.get_pending_commission(&referrer), 0);

    // Accrue more commission after the claim
    let pid_c = client.create_payment(&customer, &referred_a, &1000, &token_addr, &None, &None, &None);
    client.complete_payment(&pid_c);

    let (total_earned_final, total_claimed_final) =
        client.get_referral_commission_summary(&referrer);
    assert!(total_earned_final > total_earned_after_claim);
    assert_eq!(total_claimed_final, total_claimed_after_claim);
}
