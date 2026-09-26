#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, Env, Vec};

fn setup_veto<'a>() -> (Env, AhjoorEscrowContractClient<'a>, Address, Address, Address, Address, Address, TokenClient<'a>, TokenAdminClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);
    token_admin_client.mint(&buyer, &1000);

    let deadline = env.ledger().timestamp() + 100_000;
    client.create_escrow(
        &buyer, &seller, &arbiter, &500, &token_addr, &deadline,
        &None, &Vec::new(&env), &false, &0u32,
    );

    (env, client, admin, buyer, seller, arbiter, token_addr, token_client, token_admin_client)
}

#[test]
fn test_buyer_veto_refunds_buyer() {
    let (env, client, admin, buyer, seller, _arbiter, _token_addr, token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    let buyer_bal_before = token_client.balance(&buyer);

    client.transfer_seller_role(&seller, &escrow_id, &new_seller);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::AwaitingBuyerVetoDecision);

    client.veto_seller_transfer(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
    assert_eq!(token_client.balance(&buyer), buyer_bal_before + 500);
}

#[test]
fn test_buyer_approval_finalises_transfer() {
    let (env, client, _admin, buyer, seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    client.transfer_seller_role(&seller, &escrow_id, &new_seller);
    client.approve_seller_transfer(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.seller, new_seller);
}

#[test]
fn test_window_expiry_auto_approves() {
    let (env, client, admin, _buyer, seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    // Set short window
    client.set_seller_transfer_veto_window(&admin, &10u32);
    client.transfer_seller_role(&seller, &escrow_id, &new_seller);

    // Advance past veto window
    env.ledger().with_mut(|l| l.sequence_number += 20);

    client.expire_seller_transfer_veto(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.seller, new_seller);
}

#[test]
fn test_veto_window_configurable() {
    let (env, client, admin, _buyer, _seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    // Just verify admin can set the window without panic
    client.set_seller_transfer_veto_window(&admin, &200u32);
}

#[test]
fn test_only_seller_can_initiate_transfer() {
    let (env, client, _admin, buyer, _seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    // Buyer tries to initiate — should fail
    let result = client.try_transfer_seller_role(&buyer, &escrow_id, &new_seller);
    assert!(result.is_err());
}

// ── get_seller_transfer_proposal Tests ───────────────────────────────────────

#[test]
fn test_get_seller_transfer_proposal_none_before_transfer() {
    let (env, client, _admin, _buyer, _seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let _ = &env;
    assert_eq!(client.get_seller_transfer_proposal(&escrow_id), None);
}

#[test]
fn test_get_seller_transfer_proposal_readable_after_transfer() {
    let (env, client, _admin, _buyer, seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    client.transfer_seller_role(&seller, &escrow_id, &new_seller);

    let proposal = client
        .get_seller_transfer_proposal(&escrow_id)
        .expect("proposal should be readable after transfer_seller_role");
    assert_eq!(proposal.original_seller, seller);
    assert_eq!(proposal.new_seller, new_seller);
    assert!(proposal.veto_deadline > 0);
}

#[test]
fn test_get_seller_transfer_proposal_cleared_after_veto() {
    let (env, client, _admin, buyer, seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    client.transfer_seller_role(&seller, &escrow_id, &new_seller);
    assert!(client.get_seller_transfer_proposal(&escrow_id).is_some());

    client.veto_seller_transfer(&buyer, &escrow_id);
    assert_eq!(client.get_seller_transfer_proposal(&escrow_id), None);
}

#[test]
fn test_get_seller_transfer_proposal_cleared_after_approve() {
    let (env, client, _admin, buyer, seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    client.transfer_seller_role(&seller, &escrow_id, &new_seller);
    assert!(client.get_seller_transfer_proposal(&escrow_id).is_some());

    client.approve_seller_transfer(&buyer, &escrow_id);
    assert_eq!(client.get_seller_transfer_proposal(&escrow_id), None);
}

#[test]
fn test_get_seller_transfer_proposal_cleared_after_expiry() {
    let (env, client, admin, _buyer, seller, _arbiter, _token_addr, _token_client, _) = setup_veto();
    let escrow_id = 0u32;
    let new_seller = Address::generate(&env);

    client.set_seller_transfer_veto_window(&admin, &10u32);
    client.transfer_seller_role(&seller, &escrow_id, &new_seller);
    assert!(client.get_seller_transfer_proposal(&escrow_id).is_some());

    env.ledger().with_mut(|l| l.sequence_number += 20);
    client.expire_seller_transfer_veto(&escrow_id);

    assert_eq!(client.get_seller_transfer_proposal(&escrow_id), None);
}

// ── #420: Seller Veto Cooldown Tests ─────────────────────────────────────────

/// Helper: creates a fresh env + escrow ready for veto tests.
/// Returns (env, client, admin, buyer, seller, arbiter, escrow_id).
fn setup_veto_cooldown<'a>() -> (
    Env,
    AhjoorEscrowContractClient<'a>,
    Address,
    Address,
    Address,
    Address,
    u32,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let token_admin_client = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);
    token_admin_client.mint(&buyer, &2000);

    let deadline = env.ledger().timestamp() + 200_000;
    let escrow_id = client.create_escrow(
        &buyer, &seller, &arbiter, &500, &token_addr, &deadline,
        &None, &Vec::new(&env), &false, &0u32,
    );

    (env, client, admin, buyer, seller, arbiter, escrow_id)
}

/// Acceptance criterion 1 & 2:
/// - First raise_seller_veto succeeds and emits SellerVetoRaised.
/// - Second raise_seller_veto within cooldown returns VetoCooldownActive.
/// - After cooldown elapses a second veto is accepted.
#[test]
fn test_veto_cooldown_prevents_rapid_reuse() {
    let (env, client, _admin, _buyer, seller, _arbiter, escrow_id) = setup_veto_cooldown();

    // Reduce cooldown to 100 seconds so we can time-travel in tests.
    let admin = _admin;
    client.set_veto_cooldown_seconds(&admin, &100u64);

    // First veto — should succeed.
    client.raise_seller_veto(&seller, &escrow_id);

    // Immediately attempt a second veto — should fail with VetoCooldownActive.
    let result = client.try_raise_seller_veto(&seller, &escrow_id);
    assert!(
        result.is_err(),
        "Second raise_seller_veto within cooldown must fail"
    );

    // Advance time past the cooldown window.
    env.ledger().with_mut(|l| l.timestamp += 101);

    // Now the cooldown has elapsed — veto should succeed again.
    client.raise_seller_veto(&seller, &escrow_id);
}

/// Acceptance criterion 3:
/// Admin override resets the cooldown so the next veto after the new window is accepted.
#[test]
fn test_admin_override_resets_cooldown() {
    let (env, client, admin, _buyer, seller, _arbiter, escrow_id) = setup_veto_cooldown();

    // Short cooldown for test convenience.
    client.set_veto_cooldown_seconds(&admin, &100u64);

    // Seller raises a veto.
    client.raise_seller_veto(&seller, &escrow_id);

    // Admin overrides — resets the timestamp to now.
    client.admin_override_veto(&admin, &escrow_id);

    // Seller cannot immediately re-veto (cooldown restarted by override).
    let result = client.try_raise_seller_veto(&seller, &escrow_id);
    assert!(
        result.is_err(),
        "Seller must not re-veto immediately after admin override"
    );

    // After the window elapses, seller can veto again.
    env.ledger().with_mut(|l| l.timestamp += 101);
    client.raise_seller_veto(&seller, &escrow_id);
}

/// Acceptance criterion: release is blocked while veto is active; proceeds after override.
#[test]
fn test_release_blocked_by_veto_cleared_by_override() {
    let (env, client, admin, buyer, seller, _arbiter, escrow_id) = setup_veto_cooldown();

    // Long cooldown so the veto stays active throughout the test.
    client.set_veto_cooldown_seconds(&admin, &86400u64);

    // Seller raises a veto.
    client.raise_seller_veto(&seller, &escrow_id);

    // Buyer tries to release — must fail.
    let result = client.try_release_escrow(&buyer, &escrow_id);
    assert!(result.is_err(), "Release must be blocked while veto is active");

    // Admin overrides the veto and advances past the cooldown.
    client.admin_override_veto(&admin, &escrow_id);
    env.ledger().with_mut(|l| l.timestamp += 86401);

    // Release should now succeed.
    client.release_escrow(&buyer, &escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}
// ── Getter Tests ───────────────────────────────────────────────────────────────

#[test]
fn test_get_seller_transfer_veto_window_default() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Default should be 200 ledgers
    assert_eq!(client.get_seller_transfer_veto_window(), 200);
}

#[test]
fn test_get_seller_transfer_veto_window_after_set() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    client.initialize(&admin);

    // Set custom value
    client.set_seller_transfer_veto_window(&admin, &500);

    // Verify getter returns the set value
    assert_eq!(client.get_seller_transfer_veto_window(), 500);
}