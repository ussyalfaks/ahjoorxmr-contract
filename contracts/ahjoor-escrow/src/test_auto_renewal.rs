#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
}

fn setup<'a>() -> Setup<'a> {
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

    Setup {
        env,
        client,
        token_addr,
        token_client,
        token_admin_client,
    }
}

/// Approve the contract to pull `amount` tokens from `buyer` on their behalf.
fn approve_allowance(s: &Setup, buyer: &Address, amount: i128) {
    let expiration = s.env.ledger().sequence() + 200_000;
    s.token_client.approve(buyer, &s.client.address, &amount, &expiration);
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

/// create_escrow_with_auto_renew stores AutoRenewConfig correctly.
#[test]
fn test_create_stores_auto_renew_config() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.extensions.renewals_completed, 0);
    let stored_max_renewals = escrow.extensions.auto_renew_max_renewals.expect("AutoRenewConfig should be stored");
    assert_eq!(stored_max_renewals, 3);
    assert_eq!(escrow.extensions.auto_renew_interval_ledgers.unwrap(), 100);
}

/// Normal release triggers renewal when renewals_completed < max_renewals.
#[test]
fn test_release_triggers_renewal() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    // Mint enough for original + 3 renewals
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100, // 100 ledgers * 5 s = 500 s
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Pre-approve enough for 3 renewals
    approve_allowance(&s, &buyer, 200 * 3);

    // Release the first escrow — should trigger renewal #1
    s.client.release_escrow(&buyer, &escrow_id);

    let original = s.client.get_escrow(&escrow_id);
    assert_eq!(original.status, EscrowStatus::Released);

    // Seller received funds from the original escrow
    assert_eq!(s.token_client.balance(&seller), 200);

    // A new escrow should have been created (id = 1)
    let renewed = s.client.get_escrow(&1u32);
    assert_eq!(renewed.status, EscrowStatus::Active);
    assert_eq!(renewed.amount, 200);
    assert_eq!(renewed.buyer, buyer);
    assert_eq!(renewed.seller, seller);
    assert_eq!(renewed.extensions.renewals_completed, 1);

    // Contract holds the renewed escrow funds
    assert_eq!(s.token_client.balance(&s.client.address), 200);

    // Renewal history should contain the new escrow id
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap(), 1u32);
}

/// Full renewal chain: 3 renewals complete, then no more.
#[test]
fn test_full_renewal_chain_respects_max() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    // Mint enough for original + 3 renewals
    s.token_admin_client.mint(&buyer, &2_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Pre-approve enough for all 3 renewals
    approve_allowance(&s, &buyer, 200 * 3);

    // Release chain: escrow 0 → 1 → 2 → 3, then escrow 3 should NOT renew
    let mut current_id = escrow_id;
    for expected_renewal_index in 1u32..=3 {
        s.client.release_escrow(&buyer, &current_id);
        let next_id = current_id + 1;
        let renewed = s.client.get_escrow(&next_id);
        assert_eq!(renewed.extensions.renewals_completed, expected_renewal_index);
        current_id = next_id;
    }

    // Release the 3rd renewal — max_renewals reached, no further renewal
    let escrow_before_last = s.client.get_escrow(&current_id);
    assert_eq!(escrow_before_last.extensions.renewals_completed, 3);

    s.client.release_escrow(&buyer, &current_id);

    // No escrow with id = current_id + 1 should exist
    let result = s.client.try_get_escrow(&(current_id + 1));
    assert!(result.is_err(), "No escrow should be created after max_renewals");

    // Renewal history of original escrow should have exactly 3 entries
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 3);
}

/// Insufficient allowance emits RenewalFailed and does not panic.
#[test]
fn test_renewal_failed_on_insufficient_allowance() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &500);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 2,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Do NOT approve any allowance — renewal should fail gracefully
    // Release should succeed (seller gets paid) but renewal is skipped
    s.client.release_escrow(&buyer, &escrow_id);

    let original = s.client.get_escrow(&escrow_id);
    assert_eq!(original.status, EscrowStatus::Released);

    // Seller received funds
    assert_eq!(s.token_client.balance(&seller), 200);

    // No renewed escrow should exist
    let result = s.client.try_get_escrow(&1u32);
    assert!(result.is_err(), "No renewal should occur without allowance");

    // Renewal history should be empty
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 0);
}

/// cancel_auto_renewal prevents further renewals.
#[test]
fn test_cancel_auto_renewal_prevents_renewal() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Approve allowance for renewals
    approve_allowance(&s, &buyer, 200 * 3);

    // Buyer cancels future renewals before release
    s.client.cancel_auto_renewal(&buyer, &escrow_id);

    // Release should succeed but NOT trigger renewal
    s.client.release_escrow(&buyer, &escrow_id);

    let original = s.client.get_escrow(&escrow_id);
    assert_eq!(original.status, EscrowStatus::Released);

    // No renewed escrow
    let result = s.client.try_get_escrow(&1u32);
    assert!(result.is_err(), "Renewal should be cancelled");

    // History is empty
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 0);
}

/// Mid-chain cancellation: first renewal succeeds, then buyer cancels, second renewal skipped.
#[test]
fn test_mid_chain_cancellation() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Approve enough for all renewals
    approve_allowance(&s, &buyer, 200 * 3);

    // Release escrow 0 → renewal #1 (escrow 1) created
    s.client.release_escrow(&buyer, &escrow_id);
    let renewed_1 = s.client.get_escrow(&1u32);
    assert_eq!(renewed_1.extensions.renewals_completed, 1);

    // Buyer cancels on the renewed escrow (escrow 1)
    s.client.cancel_auto_renewal(&buyer, &1u32);

    // Release escrow 1 — should NOT trigger renewal #2
    s.client.release_escrow(&buyer, &1u32);

    let result = s.client.try_get_escrow(&2u32);
    assert!(result.is_err(), "Renewal should be cancelled after mid-chain cancel");

    // History of original escrow has 1 entry (escrow 1)
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap(), 1u32);
}

/// get_renewal_history returns ordered list of successor IDs.
#[test]
fn test_get_renewal_history_ordered() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 2,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 2);

    // Release escrow 0 → escrow 1
    s.client.release_escrow(&buyer, &escrow_id);
    // Release escrow 1 → escrow 2
    s.client.release_escrow(&buyer, &1u32);

    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 2);
    assert_eq!(history.get(0).unwrap(), 1u32);
    assert_eq!(history.get(1).unwrap(), 2u32);
}

/// cancel_auto_renewal panics if caller is not the buyer.
#[test]
fn test_cancel_auto_renewal_non_buyer_panics() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &500);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 2,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Seller tries to cancel — should panic
    let __typed_err_result = s.client.try_cancel_auto_renewal(&seller, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::OnlyBuyerCanCancelAutoRenewal.into());
}

/// cancel_auto_renewal panics if no AutoRenewConfig is set.
#[test]
fn test_cancel_auto_renewal_no_config_panics() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &500);

    let deadline = s.env.ledger().timestamp() + 500;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None,
        &Vec::new(&s.env), &false, &0u32,
    );
    let __typed_err_result = s.client.try_cancel_auto_renewal(&buyer, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::NoAutoRenewConfigSetEscrow.into());
}

/// get_auto_renewal_cancelled reflects cancellation state and a cancelled
/// escrow's auto-renewal is skipped on release.
#[test]
fn test_get_auto_renewal_cancelled() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Initially false.
    assert_eq!(s.client.get_auto_renewal_cancelled(&escrow_id), false);

    approve_allowance(&s, &buyer, 200 * 3);

    s.client.cancel_auto_renewal(&buyer, &escrow_id);

    // True after cancellation.
    assert_eq!(s.client.get_auto_renewal_cancelled(&escrow_id), true);

    // Release should succeed but NOT trigger renewal, matching the flag.
    s.client.release_escrow(&buyer, &escrow_id);
    let result = s.client.try_get_escrow(&1u32);
    assert!(result.is_err(), "Renewal should be skipped once cancelled flag is set");
}

/// Max renewals cap: renewals_completed == max_renewals means no further renewal.
#[test]
fn test_max_renewals_cap_enforced() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 1,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 2);

    // Release escrow 0 → renewal #1 (escrow 1)
    s.client.release_escrow(&buyer, &escrow_id);
    let renewed = s.client.get_escrow(&1u32);
    assert_eq!(renewed.extensions.renewals_completed, 1);

    // Release escrow 1 — max_renewals = 1 reached, no escrow 2
    s.client.release_escrow(&buyer, &1u32);
    let result = s.client.try_get_escrow(&2u32);
    assert!(result.is_err(), "No renewal after max_renewals reached");
}

/// Zero max_renewals: no renewal should occur on first release.
#[test]
fn test_zero_max_renewals_no_renewal() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 0,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200);

    // Release should succeed but NOT create any renewal
    s.client.release_escrow(&buyer, &escrow_id);

    let original = s.client.get_escrow(&escrow_id);
    assert_eq!(original.status, EscrowStatus::Released);

    // No renewed escrow
    let result = s.client.try_get_escrow(&1u32);
    assert!(result.is_err(), "Zero max_renewals should prevent any renewal");

    // History is empty
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 0);
}

/// Refund on auto-renew enabled escrow should not trigger renewal.
#[test]
fn test_refund_does_not_trigger_renewal() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 3);

    // Refund the escrow instead of releasing (full dispute resolved to buyer)
    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Refund requested"),
        &200,
    );
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32, &soroban_sdk::BytesN::from_array(&s.client.env, &[0u8; 32]));

    let original = s.client.get_escrow(&escrow_id);
    assert_eq!(original.status, EscrowStatus::Refunded);

    // Buyer received funds back
    assert_eq!(s.token_client.balance(&buyer), 1_000);

    // No renewed escrow should exist
    let result = s.client.try_get_escrow(&1u32);
    assert!(result.is_err(), "Refund should not trigger renewal");

    // Renewal history should be empty
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 0);
}

/// Dispute resolution (resolved_to_buyer) should not trigger renewal.
#[test]
fn test_dispute_resolved_to_buyer_no_renewal() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 3);

    // Dispute the escrow
    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Item not received"),
        &200,
    );

    // Resolve to buyer (refund scenario)
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32, &soroban_sdk::BytesN::from_array(&s.client.env, &[0u8; 32]));

    let original = s.client.get_escrow(&escrow_id);
    assert_eq!(original.status, EscrowStatus::Refunded);

    // No renewal should occur
    let result = s.client.try_get_escrow(&1u32);
    assert!(result.is_err(), "Dispute resolution to buyer should not trigger renewal");

    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 0);
}

/// Dispute resolution (resolved_to_seller) should not trigger renewal.
#[test]
fn test_dispute_resolved_to_seller_no_renewal() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 3);

    // Dispute the escrow
    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Item not received"),
        &200,
    );

    // Resolve to seller
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32, &soroban_sdk::BytesN::from_array(&s.client.env, &[0u8; 32]));

    let original = s.client.get_escrow(&escrow_id);
    assert_eq!(original.status, EscrowStatus::Released);

    // No renewal should occur even though status is Released
    let result = s.client.try_get_escrow(&1u32);
    assert!(result.is_err(), "Dispute resolution to seller should not trigger renewal");

    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 0);
}

/// Renewed escrows inherit correct buyer, seller, arbiter, and token.
#[test]
fn test_renewed_escrow_inherits_parties_and_token() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 2,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 2);

    // Release to trigger first renewal
    s.client.release_escrow(&buyer, &escrow_id);

    let renewed = s.client.get_escrow(&1u32);
    assert_eq!(renewed.buyer, buyer);
    assert_eq!(renewed.seller, seller);
    assert_eq!(renewed.arbiter, arbiter);
    assert_eq!(renewed.token, s.token_addr);
    assert_eq!(renewed.amount, 200);
}

/// Renewed escrow deadline is correctly calculated from renewal_interval_ledgers.
#[test]
fn test_renewed_escrow_deadline_calculation() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    s.env.ledger().with_mut(|info| info.sequence_number = 100);
    
    let original_deadline = s.env.ledger().timestamp() + 500;
    let renewal_interval_ledgers = 50u32;

    let cfg = AutoRenewConfig {
        max_renewals: 2,
        renewal_interval_ledgers,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &original_deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 2);

    // Capture current ledger before release
    let current_ledger = s.env.ledger().sequence();

    // Release to trigger renewal
    s.client.release_escrow(&buyer, &escrow_id);

    let renewed = s.client.get_escrow(&1u32);
    
    // New deadline should be based on renewal_interval_ledgers
    // Expected: current_timestamp + (renewal_interval_ledgers * 5)
    let expected_deadline = s.env.ledger().timestamp() + (renewal_interval_ledgers as u64 * 5);
    assert_eq!(renewed.deadline, expected_deadline);
}

/// Multiple cancellations on same escrow should not panic.
#[test]
fn test_multiple_cancel_calls_idempotent() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 3,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // First cancellation
    s.client.cancel_auto_renewal(&buyer, &escrow_id);
    assert_eq!(s.client.get_auto_renewal_cancelled(&escrow_id), true);

    // Second cancellation should not panic
    s.client.cancel_auto_renewal(&buyer, &escrow_id);
    assert_eq!(s.client.get_auto_renewal_cancelled(&escrow_id), true);

    // Third cancellation should also not panic
    s.client.cancel_auto_renewal(&buyer, &escrow_id);
    assert_eq!(s.client.get_auto_renewal_cancelled(&escrow_id), true);
}

/// Renewal with very large max_renewals value.
#[test]
fn test_large_max_renewals_value() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &10_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 1000,
        renewal_interval_ledgers: 100,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    // Approve for 5 renewals only
    approve_allowance(&s, &buyer, 200 * 5);

    // Execute 5 renewals successfully
    let mut current_id = escrow_id;
    for i in 1..=5 {
        s.client.release_escrow(&buyer, &current_id);
        let next_id = current_id + 1;
        let renewed = s.client.get_escrow(&next_id);
        assert_eq!(renewed.extensions.renewals_completed, i);
        current_id = next_id;
    }

    // 6th renewal should fail due to insufficient allowance, not max_renewals
    s.client.release_escrow(&buyer, &current_id);
    let result = s.client.try_get_escrow(&(current_id + 1));
    assert!(result.is_err(), "Should fail due to insufficient allowance");

    // History should have 5 entries
    let history = s.client.get_renewal_history(&escrow_id);
    assert_eq!(history.len(), 5);
}

/// get_renewal_history on non-existent escrow returns empty vector.
#[test]
fn test_get_renewal_history_non_existent_escrow() {
    let s = setup();
    
    // Query history for an escrow that was never created
    let history = s.client.get_renewal_history(&999u32);
    assert_eq!(history.len(), 0);
}

/// Renewed escrow maintains same AutoRenewConfig structure.
#[test]
fn test_renewed_escrow_preserves_auto_renew_config() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    s.env.ledger().set_timestamp(1_000);
    let deadline = s.env.ledger().timestamp() + 500;

    let cfg = AutoRenewConfig {
        max_renewals: 5,
        renewal_interval_ledgers: 200,
    };

    let escrow_id = s.client.create_escrow_with_auto_renew(
        &buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &cfg,
    );

    approve_allowance(&s, &buyer, 200 * 5);

    // Release to create first renewal
    s.client.release_escrow(&buyer, &escrow_id);

    let renewed = s.client.get_escrow(&1u32);
    
    // Verify AutoRenewConfig is preserved (with renewals_completed incremented)
    assert_eq!(renewed.extensions.auto_renew_max_renewals.unwrap(), 5);
    assert_eq!(renewed.extensions.auto_renew_interval_ledgers.unwrap(), 200);
    assert_eq!(renewed.extensions.renewals_completed, 1);
}
