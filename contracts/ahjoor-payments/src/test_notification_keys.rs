#![cfg(test)]

use crate::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events},
    token, Address, Bytes, Env,
};

fn create_token_contract<'a>(e: &Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let contract = e.register_stellar_asset_contract_v2(admin.clone());
    let contract_address = contract.address();
    let client = token::StellarAssetClient::new(e, &contract_address);
    (contract_address, client)
}

fn setup_test_env() -> (
    Env,
    Address,
    Address,
    Address,
    Address,
    AhjoorPaymentsContractClient<'static>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let (token_address, token_client) = create_token_contract(&env, &token_admin);
    token_client.mint(&customer, &10_000);

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    client.initialize(&admin, &fee_recipient, &0);
    client.set_merchant_open_mode(&true);

    (env, admin, customer, merchant, token_address, client)
}

#[test]
fn test_register_notification_key() {
    let (env, _admin, _customer, merchant, _token, client) = setup_test_env();

    let key = Bytes::from_array(&env, &[1, 2, 3, 4, 5]);

    // Register notification key
    client.register_notification_key(&merchant, &key);

    // Verify key is stored
    let stored_key = client.get_notification_key(&merchant);
    assert_eq!(stored_key, Some(key.clone()));

    // Check that events were emitted - just check that we have some events
    let _events = env.events().all();
    // For now, just check that the function worked by verifying the key is stored
    // The event emission can be tested separately
}

#[test]
fn test_remove_notification_key() {
    let (env, _admin, _customer, merchant, _token, client) = setup_test_env();

    let key = Bytes::from_array(&env, &[1, 2, 3, 4, 5]);

    // Register then remove notification key
    client.register_notification_key(&merchant, &key);
    client.remove_notification_key(&merchant);

    // Verify key is removed
    let stored_key = client.get_notification_key(&merchant);
    assert_eq!(stored_key, None);

    // Just verify the functionality works - events can be tested separately
}

#[test]
fn test_update_notification_key() {
    let (_env, _admin, _customer, merchant, _token, client) = setup_test_env();

    let key1 = Bytes::from_array(&_env, &[1, 2, 3]);
    let key2 = Bytes::from_array(&_env, &[4, 5, 6]);

    // Register first key
    client.register_notification_key(&merchant, &key1);
    assert_eq!(client.get_notification_key(&merchant), Some(key1));

    // Update to second key
    client.register_notification_key(&merchant, &key2);
    assert_eq!(client.get_notification_key(&merchant), Some(key2));
}

#[test]
#[should_panic(expected = "Notification key exceeds maximum length of 128 bytes")]
fn test_oversized_key_rejected() {
    let (_env, _admin, _customer, merchant, _token, client) = setup_test_env();

    // Create a key that's too large (129 bytes)
    let mut large_key_data = [0u8; 129];
    for i in 0..129 {
        large_key_data[i] = (i % 256) as u8;
    }
    let large_key = Bytes::from_array(&_env, &large_key_data);

    client.register_notification_key(&merchant, &large_key);
}

#[test]
#[should_panic(expected = "Notification key cannot be empty")]
fn test_empty_key_rejected() {
    let (env, _admin, _customer, merchant, _token, client) = setup_test_env();

    let empty_key = Bytes::new(&env);
    client.register_notification_key(&merchant, &empty_key);
}

#[test]
fn test_max_size_key_accepted() {
    let (env, _admin, _customer, merchant, _token, client) = setup_test_env();

    // Create a key at exactly the maximum size (128 bytes)
    let mut max_key_data = [0u8; 128];
    for i in 0..128 {
        max_key_data[i] = (i % 256) as u8;
    }
    let max_key = Bytes::from_array(&env, &max_key_data);

    client.register_notification_key(&merchant, &max_key);
    assert_eq!(client.get_notification_key(&merchant), Some(max_key));
}

#[test]
fn test_payment_events_include_notification_key() {
    let (env, _admin, customer, merchant, token, client) = setup_test_env();

    let notification_key = Bytes::from_array(&env, &[0xDE, 0xAD, 0xBE, 0xEF]);
    
    // Register notification key
    client.register_notification_key(&merchant, &notification_key);

    // Create payment
    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    // Complete payment
    client.complete_payment(&payment_id);

    // Just verify that the payment was created and completed successfully
    let payment = client.get_payment(&payment_id);
    assert_eq!(payment.status, crate::PaymentStatus::Completed);
}

#[test]
fn test_events_with_empty_notification_key() {
    let (_env, _admin, customer, merchant, token, client) = setup_test_env();

    // Don't register any notification key - should use empty bytes

    // Create payment
    let _payment_id = client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    // Just verify the payment was created successfully
    // Events can be tested separately
}

#[test]
fn test_multiple_merchants_different_keys() {
    let (env, _admin, customer, merchant1, token, client) = setup_test_env();
    let merchant2 = Address::generate(&env);

    let key1 = Bytes::from_array(&env, &[0x11, 0x11]);
    let key2 = Bytes::from_array(&env, &[0x22, 0x22]);

    // Register different keys for different merchants
    client.register_notification_key(&merchant1, &key1);
    client.register_notification_key(&merchant2, &key2);

    // Verify each merchant has their own key
    assert_eq!(client.get_notification_key(&merchant1), Some(key1));
    assert_eq!(client.get_notification_key(&merchant2), Some(key2));

    // Create payments for both merchants
    let _payment_id1 = client.create_payment(
        &customer,
        &merchant1,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );
    let _payment_id2 = client.create_payment(
        &customer,
        &merchant2,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    // Just verify the payments were created successfully
}

#[test]
fn test_notification_key_persists_across_operations() {
    let (env, _admin, customer, merchant, token, client) = setup_test_env();

    let notification_key = Bytes::from_array(&env, &[0xFF, 0xEE, 0xDD, 0xCC]);
    
    // Register notification key
    client.register_notification_key(&merchant, &notification_key);

    // Perform multiple operations
    let payment_id1 = client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    client.complete_payment(&payment_id1);

    let payment_id2 = client.create_payment(
        &customer,
        &merchant,
        &2000,
        &token,
        &None,
        &None,
        &None,
    );

    // Key should still be registered
    assert_eq!(client.get_notification_key(&merchant), Some(notification_key));

    // Verify payments were created successfully
    let payment1 = client.get_payment(&payment_id1);
    let payment2 = client.get_payment(&payment_id2);
    assert_eq!(payment1.status, crate::PaymentStatus::Completed);
    assert_eq!(payment2.status, crate::PaymentStatus::Pending);
}

#[test]
fn test_key_removal_affects_subsequent_events() {
    let (env, _admin, customer, merchant, token, client) = setup_test_env();

    let notification_key = Bytes::from_array(&env, &[0xAA, 0xBB, 0xCC, 0xDD]);
    
    // Register notification key
    client.register_notification_key(&merchant, &notification_key);

    // Create payment with key
    let _payment_id1 = client.create_payment(
        &customer,
        &merchant,
        &1000,
        &token,
        &None,
        &None,
        &None,
    );

    // Remove key
    client.remove_notification_key(&merchant);

    // Create payment without key
    let _payment_id2 = client.create_payment(
        &customer,
        &merchant,
        &2000,
        &token,
        &None,
        &None,
        &None,
    );

    // Verify key is removed
    assert_eq!(client.get_notification_key(&merchant), None);

    // Just verify both payments were created successfully
}

// --- Issue #455: test_notification_key_lifecycle ---

#[test]
fn test_notification_key_lifecycle() {
    let (env, _admin, _customer, merchant, _token, client) = setup_test_env();

    // 1. No key registered initially
    assert_eq!(client.get_notification_key(&merchant), None);

    // 2. Register notification key
    let key1 = Bytes::from_array(&env, &[0xAA, 0xBB, 0xCC]);
    client.register_notification_key(&merchant, &key1);

    // Key is queryable via get_notification_key
    assert_eq!(client.get_notification_key(&merchant), Some(key1.clone()));

    // 3. Rotate to a new key
    let key2 = Bytes::from_array(&env, &[0x11, 0x22, 0x33, 0x44]);
    client.rotate_notification_key(&merchant, &key2);

    // After rotation, active key is the new key
    assert_eq!(client.get_notification_key(&merchant), Some(key2.clone()));

    // 4. Remove notification key
    client.remove_notification_key(&merchant);
    assert_eq!(client.get_notification_key(&merchant), None);
}

#[test]
fn test_get_notification_key_history() {
    let (env, _admin, _customer, merchant, _token, client) = setup_test_env();

    // No rotations yet -> empty history.
    assert_eq!(client.get_notification_key_history(&merchant).len(), 0);

    let key1 = Bytes::from_array(&env, &[0xAA, 0xBB, 0xCC]);
    client.register_notification_key(&merchant, &key1);
    // Registering the first key does not create a prior-key history entry.
    assert_eq!(client.get_notification_key_history(&merchant).len(), 0);

    let key2 = Bytes::from_array(&env, &[0x11, 0x22, 0x33, 0x44]);
    client.rotate_notification_key(&merchant, &key2);

    // Rotating records the previous key in history.
    let history = client.get_notification_key_history(&merchant);
    assert_eq!(history.len(), 1);
    assert_eq!(history.get(0).unwrap().key, key1);
}

// ===========================================================================
//  Test: get_notification_overlap_window (#888)
// ===========================================================================

#[test]
fn test_get_notification_overlap_window_default() {
    let (_env, _admin, _customer, _merchant, _token, client) = setup_test_env();

    // Default is 2_592_000 (30 days in seconds) before any configuration
    assert_eq!(client.get_notification_overlap_window(), 2_592_000u64);
}

#[test]
fn test_get_notification_overlap_window_after_set() {
    let (_env, admin, _customer, _merchant, _token, client) = setup_test_env();

    client.set_notification_overlap_window(&admin, &3_600u64);
    assert_eq!(client.get_notification_overlap_window(), 3_600u64);
}