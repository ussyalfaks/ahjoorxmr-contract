#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env, Vec};

fn setup_cooling_off<'a>() -> (Env, AhjoorEscrowContractClient<'a>, Address, Address, Address, Address, Address, TokenClient<'a>, TokenAdminClient<'a>) {
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

    let deadline = env.ledger().timestamp() + 10_000;
    let escrow_id = client.create_escrow(
        &buyer, &seller, &arbiter, &500, &token_addr, &deadline,
        &None, &Vec::new(&env), &false, &0u32,
    );
    // Raise dispute
    client.dispute_escrow(&buyer, &escrow_id, &soroban_sdk::String::from_str(&env, "bad delivery"), &500);

    (env, client, admin, buyer, seller, arbiter, token_addr, token_client, token_admin_client)
}

fn make_reason_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[1u8; 32])
}

// ---------------------------------------------------------------------------
// Test: normal finalization after cooling-off window expires
// ---------------------------------------------------------------------------
#[test]
fn test_normal_finalization_after_cooling_off() {
    let (env, client, admin, buyer, seller, arbiter, _token_addr, token_client, _) = setup_cooling_off();
    let escrow_id = 0u32;
    let cooling_off = 3600u64; // 1 hour

    client.set_resolution_cooloff_secs(&admin, &cooling_off);

    // Arbiter resolves — should enter CoolingOff, NOT transfer funds
    let buyer_bal_before = token_client.balance(&buyer);
    let seller_bal_before = token_client.balance(&seller);
    client.resolve_dispute(&arbiter, &escrow_id, &100u32); // full buyer win

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::CoolingOff);
    // Funds NOT moved yet
    assert_eq!(token_client.balance(&buyer), buyer_bal_before);
    assert_eq!(token_client.balance(&seller), seller_bal_before);

    // Advance time past cooling-off window
    env.ledger().with_mut(|l| l.timestamp += cooling_off + 1);

    // Anyone can finalize
    client.finalize_resolution(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded); // buyer_percent=100 → Refunded
    assert_eq!(token_client.balance(&buyer), buyer_bal_before + 500);
}

// ---------------------------------------------------------------------------
// Test: flag during cooling-off blocks finalization; admin clears flag
// ---------------------------------------------------------------------------
#[test]
fn test_flagged_then_reviewed() {
    let (env, client, admin, buyer, _seller, arbiter, _token_addr, _token_client, _) = setup_cooling_off();
    let escrow_id = 0u32;
    let cooling_off = 3600u64;

    client.set_resolution_cooloff_secs(&admin, &cooling_off);
    client.resolve_dispute(&arbiter, &escrow_id, &0u32); // full seller win

    // Buyer (losing party) flags the resolution
    client.flag_resolution_error(&buyer, &escrow_id, &make_reason_hash(&env));

    // Advance past cooling-off
    env.ledger().with_mut(|l| l.timestamp += cooling_off + 1);

    // Admin reviews and clears the flag
    client.clear_resolution_flag(&admin, &escrow_id);

    // Now finalization succeeds
    client.finalize_resolution(&escrow_id);
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released); // buyer_percent=0 → Released
}

// ---------------------------------------------------------------------------
// Test: flag after window expires is rejected
// ---------------------------------------------------------------------------
#[test]
fn test_flag_after_window_expired() {
    let (env, client, admin, buyer, _seller, arbiter, _token_addr, _token_client, _) = setup_cooling_off();
    let escrow_id = 0u32;
    let cooling_off = 3600u64;

    client.set_resolution_cooloff_secs(&admin, &cooling_off);
    client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    // Advance past cooling-off
    env.ledger().with_mut(|l| l.timestamp += cooling_off + 1);

    // Attempt to flag after window — should panic
    let __typed_err_result = client.try_flag_resolution_error(&buyer, &escrow_id, &make_reason_hash(&env));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::CoolingOffWindowHasExpired.into());
}

// ---------------------------------------------------------------------------
// Test: finalize before window expires is rejected
// ---------------------------------------------------------------------------
#[test]
fn test_finalize_before_window_elapsed() {
    let (env, client, admin, _buyer, _seller, arbiter, _token_addr, _token_client, _) = setup_cooling_off();
    let escrow_id = 0u32;
    let cooling_off = 3600u64;

    client.set_resolution_cooloff_secs(&admin, &cooling_off);
    client.resolve_dispute(&arbiter, &escrow_id, &50u32);

    // Try to finalize immediately — should panic
    let __typed_err_result = client.try_finalize_resolution(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::CoolingOffWindowHasNotElapsed.into());
}

// ---------------------------------------------------------------------------
// Test: no cooling-off configured → immediate fund release (legacy path)
// ---------------------------------------------------------------------------
#[test]
fn test_no_cooling_off_immediate_release() {
    let (_env, client, _admin, buyer, _seller, arbiter, _token_addr, token_client, _) = setup_cooling_off();
    let escrow_id = 0u32;

    // cooling_off = 0 (default) → immediate execution
    let buyer_bal_before = token_client.balance(&buyer);
    client.resolve_dispute(&arbiter, &escrow_id, &100u32);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
    assert_eq!(token_client.balance(&buyer), buyer_bal_before + 500);
}
