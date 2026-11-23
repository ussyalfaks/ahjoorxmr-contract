#![cfg(test)]

use crate::{AhjoorEscrowContract, AhjoorEscrowContractClient, DisputeDefaultWinner, EscrowErrorExt};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token::Client as TokenClient,
    token::StellarAssetClient as TokenAdminClient,
    Address, Env, String,
};

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    Address,
    AhjoorEscrowContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);
    token_admin_client.mint(&buyer, &10_000);

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);

    (env, admin, buyer, seller, arbiter, token_addr, client)
}

#[test]
fn test_arbiter_resolves_before_timeout() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Raise dispute
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Arbiter resolves before timeout (within 7 days)
    env.ledger().with_mut(|li| {
        li.timestamp += 3 * 24 * 60 * 60; // 3 days
    });

    client.resolve_dispute(&arbiter, &escrow_id, &0u32, &soroban_sdk::BytesN::from_array(&client.env, &[0u8; 32]));

    // Verify dispute is resolved
    let dispute = client.get_dispute(&escrow_id);
    assert!(dispute.resolved);

    // Verify arbiter timeout count is still 0
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 0);
}

#[test]
fn test_enforce_dispute_timeout_buyer_default() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Set default winner to Buyer
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Buyer);

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Raise dispute
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Advance time past default timeout (7 days)
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60; // 8 days
    });

    // Anyone can enforce timeout
    client.enforce_dispute_timeout(&escrow_id);

    // Verify funds released to buyer (default winner)
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::Refunded);

    // Verify arbiter timeout counter incremented
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 1);

    // Verify dispute marked as resolved
    let dispute = client.get_dispute(&escrow_id);
    assert!(dispute.resolved);
}

#[test]
fn test_enforce_dispute_timeout_seller_default() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Set default winner to Seller
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Seller);

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Raise dispute
    client.dispute_escrow(&seller, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Advance time past default timeout (7 days)
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60; // 8 days
    });

    // Enforce timeout
    client.enforce_dispute_timeout(&escrow_id);

    // Verify funds released to seller (default winner)
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::Released);

    // Verify arbiter timeout counter incremented
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 1);
}

#[test]
fn test_enforce_timeout_before_deadline() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Raise dispute
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Try to enforce timeout before deadline (only 3 days)
    env.ledger().with_mut(|li| {
        li.timestamp += 3 * 24 * 60 * 60; // 3 days
    });
    let __typed_err_result = client.try_enforce_dispute_timeout(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::DisputeTimeoutDeadlineHasNotPassedYet.into());
}

#[test]
fn test_enforce_timeout_on_non_disputed_escrow() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Try to enforce timeout without dispute
    let __typed_err_result = client.try_enforce_dispute_timeout(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::EscrowIsNotDisputed.into());
}

#[test]
fn test_enforce_timeout_on_resolved_dispute() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Raise and resolve dispute
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &1000);
    client.resolve_dispute(&arbiter, &escrow_id, &0u32, &soroban_sdk::BytesN::from_array(&client.env, &[0u8; 32]));

    // Advance time past timeout
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60; // 8 days
    });

    // Try to enforce timeout on already resolved dispute
    let __typed_err_result = client.try_enforce_dispute_timeout(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::DisputeAlreadyResolved.into());
}

#[test]
fn test_per_escrow_timeout_override() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Create escrow with custom 2-day timeout
    let custom_timeout = 2 * 24 * 60 * 60; // 2 days
    let escrow_id = client.create_escrow_w_timeout(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &0,
        &custom_timeout,
    );

    // Raise dispute
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Advance time past custom timeout (2 days)
    env.ledger().with_mut(|li| {
        li.timestamp += 3 * 24 * 60 * 60; // 3 days
    });

    // Enforce timeout
    client.enforce_dispute_timeout(&escrow_id);

    // Verify timeout was enforced
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::Refunded);
}

#[test]
fn test_arbiter_timeout_counter_increments() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Create and timeout first escrow
    let escrow_id1 = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );
    client.dispute_escrow(&buyer, &escrow_id1, &String::from_str(&env, "test1"), &1000);
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });
    client.enforce_dispute_timeout(&escrow_id1);

    // Verify counter is 1
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 1);

    // Create and timeout second escrow
    let escrow_id2 = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );
    client.dispute_escrow(&buyer, &escrow_id2, &String::from_str(&env, "test2"), &1000);
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });
    client.enforce_dispute_timeout(&escrow_id2);

    // Verify counter is 2
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 2);
}

#[test]
fn test_partial_dispute_timeout() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Set default winner to Buyer
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Buyer);

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Raise partial dispute (dispute 600, release 400 to seller)
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &600);

    // Verify status is PartiallyDisputed
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::PartiallyDisputed);
    assert_eq!(escrow.amount, 600); // Only disputed amount remains

    // Advance time past timeout
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });

    // Enforce timeout
    client.enforce_dispute_timeout(&escrow_id);

    // Verify disputed portion released to buyer (default winner)
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::Refunded);
}

#[test]
fn test_get_set_default_dispute_winner() {
    let (_env, admin, _buyer, _seller, _arbiter, _token, client) = setup_test_env();

    // Default should be Buyer
    let default_winner = client.get_default_dispute_winner();
    assert_eq!(default_winner, DisputeDefaultWinner::Buyer);

    // Set to Seller
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Seller);
    let default_winner = client.get_default_dispute_winner();
    assert_eq!(default_winner, DisputeDefaultWinner::Seller);

    // Set back to Buyer
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Buyer);
    let default_winner = client.get_default_dispute_winner();
    assert_eq!(default_winner, DisputeDefaultWinner::Buyer);
}

#[test]
fn test_update_default_dispute_timeout() {
    let (_env, admin, _buyer, _seller, _arbiter, _token, client) = setup_test_env();

    // Default should be 7 days
    let default_timeout = client.get_default_dispute_timeout();
    assert_eq!(default_timeout, 7 * 24 * 60 * 60);

    // Update to 3 days
    let new_timeout = 3 * 24 * 60 * 60;
    client.update_default_dispute_timeout(&admin, &new_timeout);
    let timeout = client.get_default_dispute_timeout();
    assert_eq!(timeout, new_timeout);
}

#[test]
fn test_timeout_exactly_at_deadline() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Set default winner to Buyer
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Buyer);

    // Create escrow
    let escrow_id = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );

    // Raise dispute
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Advance time to exactly 7 days (the boundary condition)
    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60; // exactly 7 days
    });

    // Should still fail - needs to be PAST the deadline
    let __typed_err_result = client.try_enforce_dispute_timeout(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::DisputeTimeoutDeadlineHasNotPassedYet.into());

    // Add 1 second more
    env.ledger().with_mut(|li| {
        li.timestamp += 1;
    });

    // Now should succeed
    client.enforce_dispute_timeout(&escrow_id);

    // Verify timeout was enforced
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::Refunded);
}

#[test]
fn test_multiple_arbiters_timeout_counts_independently() {
    let (env, _admin, buyer, seller, _arbiter, token, client) = setup_test_env();

    // Create two different arbiters
    let arbiter1 = Address::generate(&env);
    let arbiter2 = Address::generate(&env);

    // First escrow with arbiter1 - timeout
    let escrow_id1 = client.create_escrow(
        &buyer,
        &seller,
        &arbiter1,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );
    client.dispute_escrow(&buyer, &escrow_id1, &String::from_str(&env, "test1"), &1000);
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });
    client.enforce_dispute_timeout(&escrow_id1);

    // Second escrow with arbiter2 - timeout
    let escrow_id2 = client.create_escrow(
        &buyer,
        &seller,
        &arbiter2,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );
    client.dispute_escrow(&buyer, &escrow_id2, &String::from_str(&env, "test2"), &1000);
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });
    client.enforce_dispute_timeout(&escrow_id2);

    // Third escrow with arbiter1 again - timeout
    let escrow_id3 = client.create_escrow(
        &buyer,
        &seller,
        &arbiter1,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );
    client.dispute_escrow(&buyer, &escrow_id3, &String::from_str(&env, "test3"), &1000);
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });
    client.enforce_dispute_timeout(&escrow_id3);

    // Verify arbiter1 has 2 timeouts and arbiter2 has 1
    let timeout_count1 = client.get_arbiter_timeout_count(&arbiter1);
    let timeout_count2 = client.get_arbiter_timeout_count(&arbiter2);
    assert_eq!(timeout_count1, 2);
    assert_eq!(timeout_count2, 1);
}

#[test]
fn test_timeout_does_not_increment_if_resolved_in_time() {
    let (env, _admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // First escrow - timeout
    let escrow_id1 = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );
    client.dispute_escrow(&buyer, &escrow_id1, &String::from_str(&env, "test1"), &1000);
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });
    client.enforce_dispute_timeout(&escrow_id1);

    // Verify counter is 1
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 1);

    // Second escrow - resolved in time
    let escrow_id2 = client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &false,
        &0,
    );
    client.dispute_escrow(&buyer, &escrow_id2, &String::from_str(&env, "test2"), &1000);
    env.ledger().with_mut(|li| {
        li.timestamp += 3 * 24 * 60 * 60; // Only 3 days
    });
    client.resolve_dispute(&arbiter, &escrow_id2, &0u32, &soroban_sdk::BytesN::from_array(&client.env, &[0u8; 32]));

    // Verify counter is still 1 (not incremented)
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 1);
}

#[test]
fn test_custom_timeout_shorter_than_default() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Set default winner to Seller
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Seller);

    // Create escrow with 1-day custom timeout (shorter than default 7 days)
    let custom_timeout = 1 * 24 * 60 * 60; // 1 day
    let escrow_id = client.create_escrow_w_timeout(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &0,
        &custom_timeout,
    );

    // Raise dispute
    client.dispute_escrow(&seller, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Advance time by 2 days (past custom 1-day timeout, but before default 7-day)
    env.ledger().with_mut(|li| {
        li.timestamp += 2 * 24 * 60 * 60;
    });

    // Should succeed with custom timeout
    client.enforce_dispute_timeout(&escrow_id);

    // Verify timeout was enforced
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::Released);

    // Verify arbiter timeout counter incremented
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 1);
}

#[test]
fn test_custom_timeout_longer_than_default() {
    let (env, admin, buyer, seller, arbiter, token, client) = setup_test_env();

    // Set default winner to Buyer
    client.set_default_dispute_winner(&admin, &DisputeDefaultWinner::Buyer);

    // Create escrow with 14-day custom timeout (longer than default 7 days)
    let custom_timeout = 14 * 24 * 60 * 60; // 14 days
    let escrow_id = client.create_escrow_w_timeout(
        &buyer,
        &seller,
        &arbiter,
        &1000,
        &token,
        &(env.ledger().timestamp() + 86400),
        &None,
        &soroban_sdk::Vec::new(&env),
        &0,
        &custom_timeout,
    );

    // Raise dispute
    client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&env, "test"), &1000);

    // Advance time by 8 days (past default 7-day timeout, but before custom 14-day)
    env.ledger().with_mut(|li| {
        li.timestamp += 8 * 24 * 60 * 60;
    });

    // Should fail - custom timeout not reached yet
    let __typed_err_result = client.try_enforce_dispute_timeout(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::DisputeTimeoutDeadlineHasNotPassedYet.into());

    // Advance to 15 days (past custom 14-day timeout)
    env.ledger().with_mut(|li| {
        li.timestamp += 7 * 24 * 60 * 60; // 7 more days = 15 total
    });

    // Now should succeed
    client.enforce_dispute_timeout(&escrow_id);

    // Verify timeout was enforced
    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, crate::EscrowStatus::Refunded);

    // Verify arbiter timeout counter incremented
    let timeout_count = client.get_arbiter_timeout_count(&arbiter);
    assert_eq!(timeout_count, 1);
}
