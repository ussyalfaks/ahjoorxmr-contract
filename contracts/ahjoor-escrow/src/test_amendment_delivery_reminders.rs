#![cfg(test)]
//! Tests for escrow amendments, delivery proof auto-release and deadline
//! reminders.
//!
//! Covers `apply_amendment`, `submit_delivery_proof`, `emit_deadline_reminders`
//! and `set_deadline_reminder_window`.
//!
//! There is no public setter for `delivery_proof_hash`, so it is injected
//! directly into the stored escrow via `env.as_contract`.

use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Bytes, BytesN, Env, Vec,
};

// ---------------------------------------------------------------------------
//  Shared setup
// ---------------------------------------------------------------------------

const ESCROW_AMOUNT: i128 = 500;
const BUYER_FUNDS: i128 = 10_000;

struct Setup<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    contract_id: Address,
    admin: Address,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let buyer = Address::generate(&env);
    let seller = Address::generate(&env);
    let arbiter = Address::generate(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);
    token_admin_client.mint(&buyer, &BUYER_FUNDS);

    Setup {
        env,
        client,
        contract_id,
        admin,
        buyer,
        seller,
        arbiter,
        token_addr,
        token_client,
    }
}

fn make_escrow_with_deadline(s: &Setup, deadline: u64) -> u32 {
    s.client.create_escrow(
        &s.buyer,
        &s.seller,
        &s.arbiter,
        &ESCROW_AMOUNT,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    )
}

fn make_escrow(s: &Setup) -> u32 {
    let deadline = s.env.ledger().timestamp() + 100_000;
    make_escrow_with_deadline(s, deadline)
}

/// Number of events emitted by the escrow contract during the most recent
/// top-level invocation.
fn contract_event_count(s: &Setup) -> u32 {
    let mut count = 0u32;
    for event in s.env.events().all().iter() {
        if event.0 == s.contract_id {
            count += 1;
        }
    }
    count
}

// ---------------------------------------------------------------------------
//  apply_amendment
// ---------------------------------------------------------------------------

#[test]
fn test_fully_signed_amendment_is_applied() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    let new_deadline = s.env.ledger().timestamp() + 200_000;
    let new_metadata = BytesN::from_array(&s.env, &[7u8; 32]);
    let nonce = s.client.propose_amendment(
        &s.buyer,
        &escrow_id,
        &Some(800),
        &Some(new_deadline),
        &Some(new_metadata.clone()),
    );

    // Seller's signature completes the pair and applies the amendment.
    s.client.sign_amendment(&s.seller, &escrow_id, &nonce);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.amount, 800);
    assert_eq!(escrow.deadline, new_deadline);
    assert_eq!(escrow.metadata_hash, Some(new_metadata));

    // Buyer topped up the 300 delta into the contract.
    assert_eq!(s.token_client.balance(&s.buyer), BUYER_FUNDS - 800);
    assert_eq!(s.token_client.balance(&s.contract_id), 800);
}

#[test]
fn test_fully_signed_amendment_reducing_amount_refunds_buyer() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    let nonce = s
        .client
        .propose_amendment(&s.seller, &escrow_id, &Some(200), &None, &None);
    s.client.sign_amendment(&s.buyer, &escrow_id, &nonce);

    assert_eq!(s.client.get_escrow(&escrow_id).amount, 200);
    assert_eq!(s.token_client.balance(&s.buyer), BUYER_FUNDS - 200);
    assert_eq!(s.token_client.balance(&s.contract_id), 200);
}

#[test]
fn test_applied_amendment_cannot_be_applied_twice() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    let nonce = s
        .client
        .propose_amendment(&s.buyer, &escrow_id, &Some(600), &None, &None);
    s.client.sign_amendment(&s.seller, &escrow_id, &nonce);

    // The proposal is consumed once applied.
    let result = s.client.try_apply_amendment(&escrow_id, &nonce);
    assert!(result.is_err());
    assert_eq!(s.client.get_escrow(&escrow_id).amount, 600);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_partly_signed_amendment_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    // Only the buyer (proposer) has signed.
    let nonce = s
        .client
        .propose_amendment(&s.buyer, &escrow_id, &Some(900), &None, &None);

    // AmendmentRequiresBuyerAndSellerSignatures
    s.client.apply_amendment(&escrow_id, &nonce);
}

#[test]
fn test_partly_signed_amendment_leaves_escrow_unchanged() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let before = s.client.get_escrow(&escrow_id);

    let nonce = s.client.propose_amendment(
        &s.seller,
        &escrow_id,
        &Some(900),
        &Some(before.deadline + 50_000),
        &None,
    );

    let result = s.client.try_apply_amendment(&escrow_id, &nonce);
    assert!(result.is_err());

    let after = s.client.get_escrow(&escrow_id);
    assert_eq!(after.amount, before.amount);
    assert_eq!(after.deadline, before.deadline);
    assert_eq!(s.token_client.balance(&s.buyer), BUYER_FUNDS - ESCROW_AMOUNT);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_apply_amendment_wrong_nonce_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    let nonce = s
        .client
        .propose_amendment(&s.buyer, &escrow_id, &Some(900), &None, &None);

    // AmendmentNonceMismatch
    s.client.apply_amendment(&escrow_id, &(nonce + 1));
}

// ---------------------------------------------------------------------------
//  submit_delivery_proof
// ---------------------------------------------------------------------------

fn proof_and_hash(env: &Env, seed: u8) -> (BytesN<32>, BytesN<32>) {
    let proof = BytesN::from_array(env, &[seed; 32]);
    let hash: BytesN<32> = env.crypto().sha256(&Bytes::from(proof.clone())).into();
    (proof, hash)
}

fn set_delivery_proof_hash(s: &Setup, escrow_id: u32, hash: BytesN<32>) {
    s.env.as_contract(&s.contract_id, || {
        let mut escrow: Escrow = s
            .env
            .storage()
            .persistent()
            .get(&DataKey::Escrow(escrow_id))
            .unwrap();
        escrow.extensions.delivery_proof_hash = Some(hash);
        s.env
            .storage()
            .persistent()
            .set(&DataKey::Escrow(escrow_id), &escrow);
    });
}

#[test]
fn test_valid_delivery_proof_auto_releases_to_seller() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let (proof, hash) = proof_and_hash(&s.env, 42);
    set_delivery_proof_hash(&s, escrow_id, hash);

    s.client.submit_delivery_proof(&s.seller, &escrow_id, &proof);

    assert_eq!(s.client.get_escrow(&escrow_id).status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&s.seller), ESCROW_AMOUNT);
    assert_eq!(s.token_client.balance(&s.contract_id), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_invalid_delivery_proof_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let (_, hash) = proof_and_hash(&s.env, 42);
    set_delivery_proof_hash(&s, escrow_id, hash);

    let (wrong_proof, _) = proof_and_hash(&s.env, 43);
    // InvalidDeliveryProof
    s.client.submit_delivery_proof(&s.seller, &escrow_id, &wrong_proof);
}

#[test]
fn test_invalid_delivery_proof_does_not_release() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let (_, hash) = proof_and_hash(&s.env, 42);
    set_delivery_proof_hash(&s, escrow_id, hash);

    let (wrong_proof, _) = proof_and_hash(&s.env, 43);
    let result = s
        .client
        .try_submit_delivery_proof(&s.seller, &escrow_id, &wrong_proof);
    assert!(result.is_err());

    assert_eq!(s.client.get_escrow(&escrow_id).status, EscrowStatus::Active);
    assert_eq!(s.token_client.balance(&s.seller), 0);
    assert_eq!(s.token_client.balance(&s.contract_id), ESCROW_AMOUNT);
}

#[test]
#[should_panic(expected = "Error(Contract, #23)")]
fn test_delivery_proof_from_non_seller_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let (proof, hash) = proof_and_hash(&s.env, 42);
    set_delivery_proof_hash(&s, escrow_id, hash);

    // OnlySellerCanSubmitDeliveryProof
    s.client.submit_delivery_proof(&s.buyer, &escrow_id, &proof);
}

#[test]
#[should_panic(expected = "No delivery proof hash set on this escrow")]
fn test_delivery_proof_without_hash_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let (proof, _) = proof_and_hash(&s.env, 42);
    s.client.submit_delivery_proof(&s.seller, &escrow_id, &proof);
}

#[test]
fn test_delivery_proof_after_release_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let (proof, hash) = proof_and_hash(&s.env, 42);
    set_delivery_proof_hash(&s, escrow_id, hash);

    s.client.submit_delivery_proof(&s.seller, &escrow_id, &proof);
    let result = s
        .client
        .try_submit_delivery_proof(&s.seller, &escrow_id, &proof);
    assert!(result.is_err());
    assert_eq!(s.token_client.balance(&s.seller), ESCROW_AMOUNT);
}

// ---------------------------------------------------------------------------
//  set_deadline_reminder_window
// ---------------------------------------------------------------------------

#[test]
fn test_default_deadline_reminder_window_is_48_hours() {
    let s = setup();
    assert_eq!(s.client.get_deadline_reminder_window(), 48 * 60 * 60);
}

#[test]
fn test_set_deadline_reminder_window() {
    let s = setup();
    s.client.set_deadline_reminder_window(&s.admin, &3_600);
    assert_eq!(s.client.get_deadline_reminder_window(), 3_600);
}

#[test]
fn test_set_deadline_reminder_window_zero_rejected() {
    let s = setup();
    let result = s.client.try_set_deadline_reminder_window(&s.admin, &0);
    assert!(result.is_err());
    assert_eq!(s.client.get_deadline_reminder_window(), 48 * 60 * 60);
}

#[test]
fn test_set_deadline_reminder_window_non_admin_rejected() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    let result = s.client.try_set_deadline_reminder_window(&outsider, &3_600);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
//  emit_deadline_reminders
// ---------------------------------------------------------------------------

#[test]
fn test_reminder_emitted_once_per_window() {
    let s = setup();
    let escrow_id = make_escrow(&s); // deadline = now + 100_000 (< 48h)

    let mut ids = Vec::new(&s.env);
    ids.push_back(escrow_id as u64);

    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 1);

    // A second keeper run inside the same window emits nothing.
    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 0);

    // Still nothing later in the window.
    s.env.ledger().with_mut(|l| l.timestamp += 50_000);
    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 0);
}

#[test]
fn test_reminder_not_emitted_before_window_opens() {
    let s = setup();
    s.client.set_deadline_reminder_window(&s.admin, &3_600);
    let start = s.env.ledger().timestamp();
    let escrow_id = make_escrow(&s); // deadline = start + 100_000

    let mut ids = Vec::new(&s.env);
    ids.push_back(escrow_id as u64);

    // Deadline is further away than the 1 h window.
    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 0);

    // Move inside the window: exactly one reminder.
    s.env
        .ledger()
        .with_mut(|l| l.timestamp = start + 100_000 - 1_800);
    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 1);

    // Deduplicated on subsequent runs.
    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 0);
}

#[test]
fn test_reminder_not_emitted_after_deadline() {
    let s = setup();
    let start = s.env.ledger().timestamp();
    let escrow_id = make_escrow(&s);

    let mut ids = Vec::new(&s.env);
    ids.push_back(escrow_id as u64);

    s.env
        .ledger()
        .with_mut(|l| l.timestamp = start + 100_000);
    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 0);
}

#[test]
fn test_reminders_emitted_once_per_escrow_in_batch() {
    let s = setup();
    let e1 = make_escrow(&s);
    let e2 = make_escrow(&s);

    let mut ids = Vec::new(&s.env);
    ids.push_back(e1 as u64);
    ids.push_back(e2 as u64);

    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 2);

    // Adding a new escrow later only reminds the new one.
    let e3 = make_escrow(&s);
    ids.push_back(e3 as u64);
    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 1);
}

#[test]
fn test_reminder_skips_unknown_and_released_escrows() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    let (proof, hash) = proof_and_hash(&s.env, 9);
    set_delivery_proof_hash(&s, escrow_id, hash);
    s.client.submit_delivery_proof(&s.seller, &escrow_id, &proof);

    let mut ids = Vec::new(&s.env);
    ids.push_back(escrow_id as u64);
    ids.push_back(9_999u64);

    s.client.emit_deadline_reminders(&ids);
    assert_eq!(contract_event_count(&s), 0);
}
