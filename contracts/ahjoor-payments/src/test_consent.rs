#![cfg(test)]
extern crate alloc;
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    vec, Address, BytesN, Env, String,
};

struct TestSetup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    fee_recipient: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
}

fn setup<'a>() -> TestSetup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    TestSetup {
        env,
        client,
        admin,
        fee_recipient,
        token_addr,
        token_client,
        token_admin_client,
    }
}

impl<'a> TestSetup<'a> {
    fn init(&self) {
        self.client.initialize(&self.admin, &self.fee_recipient, &0);
    }
}

fn make_bytes32(env: &Env, seed: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = seed;
    BytesN::from_array(env, &bytes)
}

fn make_string(env: &Env, s: &str) -> String {
    String::from_slice(env, s)
}

// ===========================================================================
//  Consent Record Tests (#307)
// ===========================================================================

#[test]
fn test_create_consent_record() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 1);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    assert_eq!(consent_id, 1);

    // Verify consent record was created
    let consent = s.client.get_consent_record(&consent_id);
    assert_eq!(consent.id, consent_id);
    assert_eq!(consent.merchant, merchant);
    assert_eq!(consent.customer, customer);
    assert_eq!(consent.terms_hash, terms_hash);
    assert_eq!(consent.terms_version, terms_version);
    assert_eq!(consent.is_signed, false);
    assert_eq!(consent.is_revoked, false);
}

#[test]
fn test_sign_consent() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 2);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    // Advance timestamp so signed_at is non-zero
    s.env.ledger().with_mut(|l| l.timestamp = 1000);

    // Customer signs consent
    s.client.sign_consent(&customer, &consent_id);

    // Verify consent is signed
    let consent = s.client.get_consent_record(&consent_id);
    assert_eq!(consent.is_signed, true);
    assert!(consent.signed_at > 0);
}

#[test]
fn test_is_consent_valid() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 3);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    // Before signing, consent is not valid
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version),
        false
    );

    // After signing, consent is valid
    s.client.sign_consent(&customer, &consent_id);
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version),
        true
    );
}

#[test]
fn test_consent_expiry() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 4);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 100;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    // Sign consent
    s.client.sign_consent(&customer, &consent_id);
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version),
        true
    );

    // Advance ledger past expiry
    s.env.ledger().with_mut(|l| {
        l.sequence_number = (expiry_ledger + 1) as u32;
    });

    // Consent should now be invalid
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version),
        false
    );
}

#[test]
fn test_revoke_consent() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 5);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    // Sign consent
    s.client.sign_consent(&customer, &consent_id);
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version),
        true
    );

    // Merchant revokes consent
    s.client.revoke_consent(&merchant, &consent_id);

    // Consent should now be invalid
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version),
        false
    );

    // Verify revoked flag
    let consent = s.client.get_consent_record(&consent_id);
    assert_eq!(consent.is_revoked, true);
}

#[test]
#[should_panic(expected = "Consent record already signed")]
fn test_duplicate_sign_consent_panics() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 6);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    // Sign consent twice
    s.client.sign_consent(&customer, &consent_id);
    s.client.sign_consent(&customer, &consent_id); // Should panic
}

#[test]
#[should_panic(expected = "Consent record has been revoked")]
fn test_sign_revoked_consent_panics() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 7);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    // Revoke consent
    s.client.revoke_consent(&merchant, &consent_id);

    // Try to sign revoked consent
    s.client.sign_consent(&customer, &consent_id); // Should panic
}

#[test]
fn test_get_consent_id_by_triple() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash = make_bytes32(&s.env, 8);
    let terms_version = make_string(&s.env, "v1.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    let consent_id = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash,
        &terms_version,
        &expiry_ledger,
    );

    // Retrieve consent ID by triple
    let retrieved_id = s
        .client
        .get_consent_id(&merchant, &customer, &terms_version);
    assert_eq!(retrieved_id, Some(consent_id));
}

#[test]
fn test_multiple_consent_records() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let terms_hash_v1 = make_bytes32(&s.env, 9);
    let terms_hash_v2 = make_bytes32(&s.env, 10);
    let terms_version_v1 = make_string(&s.env, "v1.0");
    let terms_version_v2 = make_string(&s.env, "v2.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    // Create two consent records for different versions
    let consent_id_v1 = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash_v1,
        &terms_version_v1,
        &expiry_ledger,
    );

    let consent_id_v2 = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash_v2,
        &terms_version_v2,
        &expiry_ledger,
    );

    assert_ne!(consent_id_v1, consent_id_v2);

    // Sign both
    s.client.sign_consent(&customer, &consent_id_v1);
    s.client.sign_consent(&customer, &consent_id_v2);

    // Both should be valid
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version_v1),
        true
    );
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version_v2),
        true
    );

    // Revoke v1
    s.client.revoke_consent(&merchant, &consent_id_v1);

    // v1 should be invalid, v2 should still be valid
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version_v1),
        false
    );
    assert_eq!(
        s.client.is_consent_valid(&merchant, &customer, &terms_version_v2),
        true
    );
}

#[test]
fn test_list_active_consent_records() {
    let s = setup();
    s.init();

    let merchant = Address::generate(&s.env);
    let customer = Address::generate(&s.env);
    let other_customer = Address::generate(&s.env);
    let terms_hash_v1 = make_bytes32(&s.env, 1);
    let terms_hash_v2 = make_bytes32(&s.env, 2);
    let terms_hash_v3 = make_bytes32(&s.env, 3);
    let terms_version_v1 = make_string(&s.env, "v1.0");
    let terms_version_v2 = make_string(&s.env, "v2.0");
    let terms_version_v3 = make_string(&s.env, "v3.0");
    let expiry_ledger: u64 = u64::from(s.env.ledger().sequence()) + 1000;

    // Create 3 consent records for customer: one active, one revoked, one unsigned
    let consent_active = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash_v1,
        &terms_version_v1,
        &expiry_ledger,
    );
    let consent_revoked = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash_v2,
        &terms_version_v2,
        &expiry_ledger,
    );
    let consent_unsigned = s.client.create_consent_record(
        &merchant,
        &customer,
        &terms_hash_v3,
        &terms_version_v3,
        &expiry_ledger,
    );

    // Also create a record for a different customer
    let consent_other = s.client.create_consent_record(
        &merchant,
        &other_customer,
        &terms_hash_v1,
        &terms_version_v1,
        &expiry_ledger,
    );

    // Sign the active and revoked record; leave unsigned unsigned
    s.client.sign_consent(&customer, &consent_active);
    s.client.sign_consent(&customer, &consent_revoked);

    // Revoke the second record
    s.client.revoke_consent(&merchant, &consent_revoked);

    // List active records for customer
    let active = s.client.list_active_consent_records(&customer);
    assert_eq!(active.len(), 1);
    assert_eq!(active.get(0).unwrap().id, consent_active);

    // The other customer's record should not appear
    for record in active.iter() {
        assert_eq!(record.customer, customer);
    }

    // List active records for the other customer
    let active_other = s.client.list_active_consent_records(&other_customer);
    assert_eq!(active_other.len(), 1);
    assert_eq!(active_other.get(0).unwrap().id, consent_other);
}
