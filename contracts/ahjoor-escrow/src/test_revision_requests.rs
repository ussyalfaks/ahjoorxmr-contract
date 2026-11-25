#![cfg(test)]
//! Tests for buyer revision requests: `request_revision`,
//! `acknowledge_revision`, `reject_revision`, `escalate_revision_to_dispute`,
//! `set_max_revision_requests` and `set_revision_response_deadline`.

use super::*;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, BytesN, Env, String, Vec,
};

const DEFAULT_RESPONSE_DEADLINE: u64 = 7 * 24 * 60 * 60;
const DEFAULT_MAX_REVISIONS: u32 = 3;

// ---------------------------------------------------------------------------
//  Shared setup
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    admin: Address,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    token_addr: Address,
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
    let token_admin_client = TokenAdminClient::new(&env, &token_addr);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);
    token_admin_client.mint(&buyer, &10_000);

    Setup {
        env,
        client,
        admin,
        buyer,
        seller,
        arbiter,
        token_addr,
    }
}

/// Create a basic active escrow and return its ID.
fn make_escrow(s: &Setup) -> u32 {
    let deadline = s.env.ledger().timestamp() + 30 * 24 * 60 * 60;
    s.client.create_escrow(
        &s.buyer,
        &s.seller,
        &s.arbiter,
        &500,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    )
}

fn notes(s: &Setup, byte: u8) -> BytesN<32> {
    BytesN::from_array(&s.env, &[byte; 32])
}

fn advance(s: &Setup, seconds: u64) {
    let now = s.env.ledger().timestamp();
    s.env.ledger().set_timestamp(now + seconds);
}

/// Request a revision and have the seller acknowledge it, consuming one slot.
fn request_and_ack(s: &Setup, escrow_id: u32) {
    s.client.request_revision(&s.buyer, &escrow_id, &notes(s, 1));
    let req = s.client.get_revision_request(&escrow_id).unwrap();
    s.client
        .acknowledge_revision(&s.seller, &escrow_id, &req.request_id);
}

// ---------------------------------------------------------------------------
//  request_revision
// ---------------------------------------------------------------------------

#[test]
fn test_request_revision_stores_pending_request() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.env.ledger().set_timestamp(1_000);

    assert!(s.client.get_revision_request(&escrow_id).is_none());
    assert_eq!(s.client.get_revision_count(&escrow_id), 0);

    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 7));

    let req = s.client.get_revision_request(&escrow_id).unwrap();
    assert_eq!(req.request_id, 1);
    assert_eq!(req.requested_by, s.buyer);
    assert_eq!(req.revision_notes_hash, notes(&s, 7));
    assert_eq!(req.created_at, 1_000);
    assert_eq!(req.response_deadline, 1_000 + DEFAULT_RESPONSE_DEADLINE);
    assert_eq!(s.client.get_revision_count(&escrow_id), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_request_revision_by_non_buyer_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.seller, &escrow_id, &notes(&s, 1));
}

#[test]
#[should_panic(expected = "Error(Contract, #16)")]
fn test_request_revision_while_pending_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 2));
}

#[test]
fn test_request_ids_increment_per_escrow() {
    let s = setup();
    let first = make_escrow(&s);
    let second = make_escrow(&s);

    request_and_ack(&s, first);
    s.client
        .request_revision(&s.buyer, &first, &notes(&s, 2));
    assert_eq!(s.client.get_revision_request(&first).unwrap().request_id, 2);

    // Counters are tracked independently per escrow.
    s.client
        .request_revision(&s.buyer, &second, &notes(&s, 3));
    assert_eq!(s.client.get_revision_request(&second).unwrap().request_id, 1);
    assert_eq!(s.client.get_revision_count(&first), 2);
    assert_eq!(s.client.get_revision_count(&second), 1);
}

// ---------------------------------------------------------------------------
//  acknowledge_revision
// ---------------------------------------------------------------------------

#[test]
fn test_acknowledge_revision_clears_pending_request() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client.acknowledge_revision(&s.seller, &escrow_id, &1);

    assert!(s.client.get_revision_request(&escrow_id).is_none());
    // Acknowledging does not refund the slot.
    assert_eq!(s.client.get_revision_count(&escrow_id), 1);
    assert_eq!(s.client.get_escrow(&escrow_id).status, EscrowStatus::Active);
}

#[test]
#[should_panic(expected = "Error(Contract, #28)")]
fn test_acknowledge_revision_by_non_seller_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client.acknowledge_revision(&s.buyer, &escrow_id, &1);
}

#[test]
#[should_panic(expected = "Error(Contract, #29)")]
fn test_acknowledge_revision_wrong_id_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client.acknowledge_revision(&s.seller, &escrow_id, &2);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_acknowledge_revision_without_pending_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client.acknowledge_revision(&s.seller, &escrow_id, &1);
}

// ---------------------------------------------------------------------------
//  reject_revision
// ---------------------------------------------------------------------------

#[test]
fn test_reject_revision_clears_pending_request() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client
        .reject_revision(&s.buyer, &escrow_id, &1, &notes(&s, 9));

    assert!(s.client.get_revision_request(&escrow_id).is_none());
    assert_eq!(s.client.get_revision_count(&escrow_id), 1);
    assert_eq!(s.client.get_escrow(&escrow_id).status, EscrowStatus::Active);

    // A fresh request can be raised after rejection.
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 2));
    assert_eq!(s.client.get_revision_request(&escrow_id).unwrap().request_id, 2);
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_reject_revision_by_non_buyer_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client
        .reject_revision(&s.seller, &escrow_id, &1, &notes(&s, 9));
}

#[test]
#[should_panic(expected = "Error(Contract, #29)")]
fn test_reject_revision_wrong_id_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client
        .reject_revision(&s.buyer, &escrow_id, &5, &notes(&s, 9));
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_reject_revision_without_pending_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .reject_revision(&s.buyer, &escrow_id, &1, &notes(&s, 9));
}

// ---------------------------------------------------------------------------
//  Revision cap
// ---------------------------------------------------------------------------

#[test]
fn test_default_revision_cap_allows_three_requests() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    for _ in 0..DEFAULT_MAX_REVISIONS {
        request_and_ack(&s, escrow_id);
    }
    assert_eq!(s.client.get_revision_count(&escrow_id), DEFAULT_MAX_REVISIONS);

    assert!(s
        .client
        .try_request_revision(&s.buyer, &escrow_id, &notes(&s, 4))
        .is_err());
    assert!(s.client.get_revision_request(&escrow_id).is_none());
}

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_request_beyond_default_cap_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);

    for _ in 0..DEFAULT_MAX_REVISIONS {
        request_and_ack(&s, escrow_id);
    }
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 4));
}

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_custom_revision_cap_enforced() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client.set_max_revision_requests(&s.admin, &escrow_id, &1);

    request_and_ack(&s, escrow_id);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 2));
}

#[test]
#[should_panic(expected = "Error(Contract, #26)")]
fn test_zero_revision_cap_blocks_all_requests() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client.set_max_revision_requests(&s.admin, &escrow_id, &0);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
}

#[test]
fn test_raising_revision_cap_allows_more_requests() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client.set_max_revision_requests(&s.admin, &escrow_id, &5);

    for _ in 0..5 {
        request_and_ack(&s, escrow_id);
    }
    assert_eq!(s.client.get_revision_count(&escrow_id), 5);
    assert!(s
        .client
        .try_request_revision(&s.buyer, &escrow_id, &notes(&s, 6))
        .is_err());
}

#[test]
fn test_revision_cap_is_per_escrow() {
    let s = setup();
    let capped = make_escrow(&s);
    let other = make_escrow(&s);
    s.client.set_max_revision_requests(&s.admin, &capped, &1);

    request_and_ack(&s, capped);
    assert!(s
        .client
        .try_request_revision(&s.buyer, &capped, &notes(&s, 2))
        .is_err());

    // The other escrow keeps the default cap.
    for _ in 0..DEFAULT_MAX_REVISIONS {
        request_and_ack(&s, other);
    }
    assert_eq!(s.client.get_revision_count(&other), DEFAULT_MAX_REVISIONS);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_set_max_revision_requests_by_non_admin_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client.set_max_revision_requests(&s.buyer, &escrow_id, &10);
}

// ---------------------------------------------------------------------------
//  Response deadline
// ---------------------------------------------------------------------------

#[test]
fn test_custom_response_deadline_applies_to_new_requests() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.env.ledger().set_timestamp(5_000);

    s.client.set_revision_response_deadline(&s.admin, &3_600);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));

    let req = s.client.get_revision_request(&escrow_id).unwrap();
    assert_eq!(req.response_deadline, 5_000 + 3_600);
}

#[test]
fn test_response_deadline_change_does_not_affect_pending_request() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.env.ledger().set_timestamp(5_000);

    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));
    s.client.set_revision_response_deadline(&s.admin, &60);

    let req = s.client.get_revision_request(&escrow_id).unwrap();
    assert_eq!(req.response_deadline, 5_000 + DEFAULT_RESPONSE_DEADLINE);

    // The original (longer) deadline still governs escalation.
    advance(&s, 61);
    assert!(s
        .client
        .try_escalate_revision_to_dispute(&s.buyer, &escrow_id)
        .is_err());
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_zero_response_deadline_rejected() {
    let s = setup();
    s.client.set_revision_response_deadline(&s.admin, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_set_response_deadline_by_non_admin_rejected() {
    let s = setup();
    s.client.set_revision_response_deadline(&s.buyer, &3_600);
}

// ---------------------------------------------------------------------------
//  Escalation to dispute
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #30)")]
fn test_escalate_before_deadline_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client.set_revision_response_deadline(&s.admin, &3_600);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));

    advance(&s, 3_599);
    s.client.escalate_revision_to_dispute(&s.buyer, &escrow_id);
}

#[test]
fn test_escalate_at_deadline_opens_dispute() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.env.ledger().set_timestamp(10_000);
    s.client.set_revision_response_deadline(&s.admin, &3_600);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));

    // Deadline expires exactly at created_at + 3_600.
    s.env.ledger().set_timestamp(10_000 + 3_600);
    s.client.escalate_revision_to_dispute(&s.buyer, &escrow_id);

    assert!(s.client.get_revision_request(&escrow_id).is_none());
    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);

    let dispute = s.client.get_dispute(&escrow_id);
    assert_eq!(dispute.escrow_id, escrow_id);
    assert_eq!(
        dispute.reason,
        String::from_str(&s.env, "Revision request escalation")
    );
    assert_eq!(dispute.created_at, 10_000 + 3_600);
    assert!(!dispute.resolved);
    assert_eq!(dispute.dispute_amount, escrow.amount);
}

#[test]
fn test_escalate_after_default_deadline_expiry() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));

    advance(&s, DEFAULT_RESPONSE_DEADLINE + 1);
    s.client.escalate_revision_to_dispute(&s.buyer, &escrow_id);
    assert_eq!(s.client.get_escrow(&escrow_id).status, EscrowStatus::Disputed);
}

#[test]
#[should_panic(expected = "Error(Contract, #24)")]
fn test_escalate_by_non_buyer_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));

    advance(&s, DEFAULT_RESPONSE_DEADLINE);
    s.client.escalate_revision_to_dispute(&s.seller, &escrow_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_escalate_without_pending_request_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client.escalate_revision_to_dispute(&s.buyer, &escrow_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #27)")]
fn test_escalate_after_seller_acknowledged_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    request_and_ack(&s, escrow_id);

    advance(&s, DEFAULT_RESPONSE_DEADLINE + 1);
    s.client.escalate_revision_to_dispute(&s.buyer, &escrow_id);
}

#[test]
#[should_panic(expected = "Error(Contract, #25)")]
fn test_request_revision_after_escalation_rejected() {
    let s = setup();
    let escrow_id = make_escrow(&s);
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 1));

    advance(&s, DEFAULT_RESPONSE_DEADLINE);
    s.client.escalate_revision_to_dispute(&s.buyer, &escrow_id);

    // The escrow is no longer Active, so further revisions are refused.
    s.client
        .request_revision(&s.buyer, &escrow_id, &notes(&s, 2));
}
