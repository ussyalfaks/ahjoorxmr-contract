#![cfg(test)]
extern crate alloc;
use super::*;
use proptest::prelude::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, BytesN, Env, IntoVal, String, Symbol,
};

const UPGRADE_WASM: &[u8] = include_bytes!("../../../fixtures/upgrade_contract.wasm");

// ---------------------------------------------------------------------------
//  Test Helpers
// ---------------------------------------------------------------------------

struct TestSetup<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
}

fn setup<'a>() -> TestSetup<'a> {
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

    TestSetup {
        env,
        client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
    }
}

// ===========================================================================
//  Create Escrow Tests
// ===========================================================================

#[test]
fn test_create_escrow() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    assert_eq!(escrow_id, 0);
    assert_eq!(s.token_client.balance(&buyer), 750);
    assert_eq!(s.token_client.balance(&s.client.address), 250);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.amount, 250);
    assert_eq!(escrow.buyer, buyer);
    assert_eq!(escrow.seller, seller);
    assert_eq!(escrow.arbiter, arbiter);
}

#[test]
fn test_receipt_transfer_and_release_to_new_holder() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let new_holder = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    let receipt = s.client.get_escrow_receipt(&escrow_id).expect("receipt exists");
    let receipt_id = receipt.receipt_id;

    s.client.transfer_escrow_receipt(&seller, &receipt_id, &new_holder);

    s.client.release_escrow(&buyer, &escrow_id);

    assert_eq!(s.token_client.balance(&new_holder), 250);
    assert!(s.client.get_escrow_receipt(&escrow_id).is_none());
}

#[test]
fn test_non_holder_transfer_rejected() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    let receipt = s.client.get_escrow_receipt(&escrow_id).expect("receipt exists");
    let receipt_id = receipt.receipt_id;

    // Arbiter is not the holder — should panic
    let __typed_err_result = s.client.try_transfer_escrow_receipt(&arbiter, &receipt_id, &Address::generate(&s.env));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::OnlyCurrentHolderCanTransferReceipt.into());
}

#[test]
fn test_mid_milestone_transfer_rejection() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_milestone_escrow(&buyer, &seller, &arbiter, &s.token_addr, &deadline, &make_milestones(&s.env, &[100, 100]));

    let receipt = s.client.get_escrow_receipt(&escrow_id).expect("receipt exists");
    let receipt_id = receipt.receipt_id;

    // Seller cannot transfer while milestones present
    let __typed_err_result = s.client.try_transfer_escrow_receipt(&seller, &receipt_id, &Address::generate(&s.env));
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::ActiveMilestoneInProgress.into());
}

#[test]
fn test_burn_on_cancel() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.request_cancellation(&seller, &escrow_id, &BytesN::from_array(&s.env, &[0u8; 32]));
    s.client.accept_cancellation(&buyer, &escrow_id);

    assert!(s.client.get_escrow_receipt(&escrow_id).is_none());
}

#[test]
fn test_create_escrow_zero_amount_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let __typed_err_result = s.client
        .try_create_escrow(&buyer, &seller, &arbiter, &0, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::EscrowAmountMustBePositive.into());
}

#[test]
fn test_create_escrow_past_deadline_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp();
    let __typed_err_result = s.client
        .try_create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::DeadlineMustBeFuture.into());
}

// ===========================================================================
//  Release Escrow Tests
// ===========================================================================

#[test]
fn test_release_escrow_by_buyer() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.release_escrow(&buyer, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&seller), 250);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_amendment_past_deadline_rejected() {
    let s = setup();
    s.env.ledger().with_mut(|li| li.timestamp = 1_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2_000);

    let deadline = s.env.ledger().timestamp() + 1_000;
    let escrow_id = s.client.create_escrow(&buyer, &seller, &arbiter, &500, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    let amended_deadline = s.env.ledger().timestamp() + 100;
    let nonce = s.client.propose_amendment(&buyer, &escrow_id, &None, &Some(amended_deadline), &None);

    s.env.ledger().with_mut(|li| li.timestamp = amended_deadline);
    let result = s.client.try_sign_amendment(&seller, &escrow_id, &nonce);
    assert_eq!(result.unwrap_err().unwrap(), EscrowError::InvalidDeadline.into());

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.deadline, deadline);
}

#[test]
fn test_future_deadline_amendment_applies() {
    let s = setup();
    s.env.ledger().with_mut(|li| li.timestamp = 1_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2_000);

    let deadline = s.env.ledger().timestamp() + 1_000;
    let escrow_id = s.client.create_escrow(&buyer, &seller, &arbiter, &500, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    let amended_deadline = s.env.ledger().timestamp() + 2_000;
    let nonce = s.client.propose_amendment(&buyer, &escrow_id, &None, &Some(amended_deadline), &None);
    s.client.sign_amendment(&seller, &escrow_id, &nonce);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.amount, 500);
    assert_eq!(escrow.deadline, amended_deadline);
}

#[test]
fn test_tranche_index_bounds_check() {
    let s = setup();
    s.env.ledger().with_mut(|li| li.timestamp = 1_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1_000);

    let mut schedule = Vec::new(&s.env);
    schedule.push_back(ReleaseTranche { unlock_at: 1_100, amount: 100, claimed: false });
    schedule.push_back(ReleaseTranche { unlock_at: 1_200, amount: 150, claimed: false });
    schedule.push_back(ReleaseTranche { unlock_at: 1_300, amount: 250, claimed: false });

    let escrow_id = s.client.create_escrow_with_schedule(&buyer, &seller, &arbiter, &s.token_addr, &2_000, &schedule);

    let bad_index = s.client.try_claim_scheduled_tranche(&seller, &escrow_id, &99u32);
    assert_eq!(bad_index.unwrap_err().unwrap(), EscrowError::InvalidTrancheIndex.into());

    s.env.ledger().with_mut(|li| li.timestamp = 1_250);
    s.client.claim_scheduled_tranche(&seller, &escrow_id, &1u32);
    assert_eq!(s.token_client.balance(&seller), 150);

    let duplicate = s.client.try_claim_scheduled_tranche(&seller, &escrow_id, &1u32);
    assert_eq!(duplicate.unwrap_err().unwrap(), EscrowError::TrancheAlreadyClaimed.into());
}

#[test]
fn test_release_escrow_by_arbiter() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.release_escrow(&arbiter, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&seller), 250);
}

#[test]
fn test_partial_release_by_buyer() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.partial_release(&buyer, &escrow_id, &150);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::PartiallyReleased);
    assert_eq!(escrow.amount, 100);
    assert_eq!(s.token_client.balance(&seller), 150);
    assert_eq!(s.token_client.balance(&s.client.address), 100);
}

#[test]
fn test_double_partial_release_then_full_release_remaining() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.partial_release(&buyer, &escrow_id, &100);
    s.client.partial_release(&arbiter, &escrow_id, &75);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::PartiallyReleased);
    assert_eq!(escrow.amount, 75);
    assert_eq!(s.token_client.balance(&seller), 175);
    assert_eq!(s.token_client.balance(&s.client.address), 75);

    s.client.release_escrow(&buyer, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(escrow.amount, 75);
    assert_eq!(s.token_client.balance(&seller), 250);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_partial_release_over_release_attempt_rejected() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    let result = s.client.try_partial_release(&buyer, &escrow_id, &251);
    assert!(result.is_err());

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
    assert_eq!(escrow.amount, 250);
    assert_eq!(s.token_client.balance(&seller), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 250);
}

#[test]
fn test_release_escrow_by_seller_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    let __typed_err_result = s.client.try_release_escrow(&seller, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::OnlyBuyerOrArbiterCanReleaseEscrow.into());
}

#[test]
fn test_release_escrow_already_released_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.release_escrow(&buyer, &escrow_id);
    let __typed_err_result = s.client.try_release_escrow(&buyer, &escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::EscrowIsNotActive.into());
}

// ===========================================================================
//  Dispute Escrow Tests
// ===========================================================================

#[test]
fn test_dispute_escrow_by_buyer() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Item not received"),
        &250,
    );

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);

    let dispute = s.client.get_dispute(&escrow_id);
    assert_eq!(dispute.resolved, false);
}

#[test]
fn test_dispute_escrow_by_seller() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &seller,
        &escrow_id,
        &String::from_str(&s.env, "Payment not received"),
        &250,
    );

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_dispute_escrow_by_arbiter_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    let __typed_err_result = s.client.try_dispute_escrow(
        &arbiter,
        &escrow_id,
        &String::from_str(&s.env, "Invalid"),
        &250,
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::OnlyBuyerOrSellerCanDisputeEscrow.into());
}

#[test]
fn test_check_escalation_uses_custom_timeout() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow_w_timeout(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &0u32,
        &300u64,
    );

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "custom-timeout"),
        &250,
    );

    s.env.ledger().set_timestamp(1250);
    assert!(!s.client.check_escalation(&escrow_id));

    s.env.ledger().set_timestamp(1401);
    assert!(s.client.check_escalation(&escrow_id));
}

#[test]
fn test_check_escalation_uses_default_timeout_when_missing() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.client
        .update_default_dispute_timeout(&s.admin, &200u64);

    s.env.ledger().set_timestamp(500);
    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "default-timeout"),
        &250,
    );

    s.env.ledger().set_timestamp(650);
    assert!(!s.client.check_escalation(&escrow_id));

    s.env.ledger().set_timestamp(710);
    assert!(s.client.check_escalation(&escrow_id));
}

#[test]
fn test_escalation_emits_event_with_effective_timeout() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow_w_timeout(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &0u32,
        &180u64,
    );

    s.client.dispute_escrow(
        &seller,
        &escrow_id,
        &String::from_str(&s.env, "event-check"),
        &250,
    );

    s.env.ledger().set_timestamp(290);
    assert!(s.client.check_escalation(&escrow_id));

    let events = s.env.events().all();
    let last = events.last().unwrap();
    let expected_topics = (Symbol::new(&s.env, "dispute_escalated"),).into_val(&s.env);
    assert_eq!(last.1, expected_topics);

    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = last.2.into_val(&s.env);
    let timeout_seconds: u64 = data
        .get(Symbol::new(&s.env, "timeout_seconds"))
        .unwrap()
        .into_val(&s.env);
    assert_eq!(timeout_seconds, 180u64);
}

#[test]
fn test_create_escrow_with_zero_dispute_timeout_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let __typed_err_result = s.client.try_create_escrow_w_timeout(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &0u32,
        &0u64,
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::DisputeTimeoutSecondsMustBePositive.into());
}

#[test]
fn test_update_default_dispute_timeout_zero_panics() {
    let s = setup();
    let __typed_err_result = s.client
        .try_update_default_dispute_timeout(&s.admin, &0u64);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::TimeoutMustBePositive.into());
}

// ===========================================================================
//  Partial Dispute Tests
// ===========================================================================

#[test]
fn test_partial_dispute_50_50_split() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    // Dispute only 100 (50%), undisputed 100 released to seller immediately
    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Half disputed"),
        &100,
    );

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::PartiallyDisputed);
    assert_eq!(escrow.amount, 100); // only disputed portion held
    assert_eq!(s.token_client.balance(&seller), 100); // undisputed released
    assert_eq!(s.token_client.balance(&s.client.address), 100);

    let dispute = s.client.get_dispute(&escrow_id);
    assert_eq!(dispute.dispute_amount, 100);
    assert_eq!(dispute.resolved, false);

    // Arbiter resolves the disputed portion to seller
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);
    assert_eq!(s.token_client.balance(&seller), 200);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_partial_dispute_80_20_split() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &100, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    // Dispute only 20 (20%), undisputed 80 released to seller immediately
    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Minor issue"),
        &20,
    );

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::PartiallyDisputed);
    assert_eq!(escrow.amount, 20);
    assert_eq!(s.token_client.balance(&seller), 80);
    assert_eq!(s.token_client.balance(&s.client.address), 20);

    // Arbiter refunds the disputed portion to buyer
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32);
    assert_eq!(s.token_client.balance(&buyer), 920); // 1000 - 100 deposited + 20 refunded
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_full_dispute_still_works() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    // Full dispute: dispute_amount == escrow amount
    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Full dispute"),
        &250,
    );

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
    assert_eq!(escrow.amount, 250);
    assert_eq!(s.token_client.balance(&seller), 0); // nothing released
    assert_eq!(s.token_client.balance(&s.client.address), 250);
}

#[test]
fn test_partial_dispute_amount_exceeds_escrow_rejected() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &100, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    let result = s.client.try_dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Over dispute"),
        &101,
    );
    assert!(result.is_err());

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
}

#[test]
fn test_partial_dispute_emits_partial_dispute_raised_event() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Partial"),
        &120,
    );

    let events = s.env.events().all();
    let last = events.last().unwrap();
    let expected_topics = (Symbol::new(&s.env, "partial_dispute_raised"),).into_val(&s.env);
    assert_eq!(last.1, expected_topics);

    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = last.2.into_val(&s.env);
    let dispute_amount: i128 = data
        .get(Symbol::new(&s.env, "dispute_amount"))
        .unwrap()
        .into_val(&s.env);
    let released_amount: i128 = data
        .get(Symbol::new(&s.env, "released_amount"))
        .unwrap()
        .into_val(&s.env);
    assert_eq!(dispute_amount, 120);
    assert_eq!(released_amount, 80);
}

// ===========================================================================
//  Resolve Dispute Tests
// ===========================================================================

#[test]
fn test_resolve_dispute_to_seller() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Item not received"),
        &250,
    );
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&seller), 250);
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let dispute = s.client.get_dispute(&escrow_id);
    assert_eq!(dispute.resolved, true);
}

#[test]
fn test_resolve_dispute_to_buyer() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &seller,
        &escrow_id,
        &String::from_str(&s.env, "Payment not received"),
        &250,
    );
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
    assert_eq!(s.token_client.balance(&buyer), 1000);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_resolve_dispute_by_buyer_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Item not received"),
        &250,
    );
    let __typed_err_result = s.client.try_resolve_dispute(&buyer, &escrow_id, &0u32);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::OnlyArbiterCanResolveDispute.into());
}

// ===========================================================================
//  Dispute Split Tests (Task 4)
// ===========================================================================

#[test]
fn test_resolve_dispute_50_50_split() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Partial issue"),
        &1000,
    );
    // 50% to buyer, 50% to seller
    s.client.resolve_dispute(&arbiter, &escrow_id, &50u32);

    assert_eq!(s.token_client.balance(&buyer), 500);  // 0 remaining + 500 refunded
    assert_eq!(s.token_client.balance(&seller), 500);
    assert_eq!(s.token_client.balance(&s.client.address), 0);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Resolved);
}

#[test]
fn test_resolve_dispute_split_with_fee() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let fee_recipient = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    // 100 bps = 1% protocol fee
    s.client.update_protocol_fee(&s.admin, &100, &fee_recipient);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Partial issue"),
        &1000,
    );
    // fee = 1000 * 100 / 10000 = 10; distributable = 990; 50% split → buyer=495, seller=495; fee accrued
    s.client.resolve_dispute(&arbiter, &escrow_id, &50u32);

    assert_eq!(s.token_client.balance(&fee_recipient), 0);
    assert_eq!(s.token_client.balance(&buyer), 495);
    assert_eq!(s.token_client.balance(&seller), 495);
    assert_eq!(s.token_client.balance(&s.client.address), 10);
}

#[test]
fn test_resolve_dispute_100_0_is_full_buyer_win() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &500, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "d"), &500);
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
    assert_eq!(s.token_client.balance(&buyer), 1000);
    assert_eq!(s.token_client.balance(&seller), 0);
}

#[test]
fn test_resolve_dispute_0_100_is_full_seller_win() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &500, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "d"), &500);
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&seller), 500);
    assert_eq!(s.token_client.balance(&buyer), 500); // 1000 - 500 deposited
}

#[test]
fn test_resolve_dispute_invalid_percent_panics() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &500, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "d"), &500);
    let __typed_err_result = s.client.try_resolve_dispute(&arbiter, &escrow_id, &101u32);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::BuyerPercentMustBeBetween0And100.into());
}

// ===========================================================================
//  Protocol Fee Tests
// ===========================================================================

#[test]
fn test_protocol_fee_deducted_on_resolve_to_seller() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let fee_recipient = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    // 100 bps = 1%
    s.client.update_protocol_fee(&s.admin, &100, &fee_recipient);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Dispute"),
        &1000,
    );
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    // fee = 1000 * 100 / 10000 = 10; seller gets 990; fee is accrued in contract
    assert_eq!(s.token_client.balance(&seller), 990);
    assert_eq!(s.token_client.balance(&fee_recipient), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 10);
}

#[test]
fn test_protocol_fee_deducted_on_resolve_to_buyer() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let fee_recipient = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    // 200 bps = 2%
    s.client.update_protocol_fee(&s.admin, &200, &fee_recipient);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &500, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &seller,
        &escrow_id,
        &String::from_str(&s.env, "Dispute"),
        &500,
    );
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32);

    // fee = 500 * 200 / 10000 = 10; buyer gets 490; fee is accrued in contract
    assert_eq!(s.token_client.balance(&buyer), 990); // 1000 - 500 deposited + 490 refunded
    assert_eq!(s.token_client.balance(&fee_recipient), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 10);
}

#[test]
fn test_zero_protocol_fee_skips_fee_transfer() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let fee_recipient = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    // fee_bps = 0, no fee transfer should happen
    s.client.update_protocol_fee(&s.admin, &0, &fee_recipient);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Dispute"),
        &250,
    );
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    assert_eq!(s.token_client.balance(&seller), 250);
    assert_eq!(s.token_client.balance(&fee_recipient), 0);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_protocol_fee_cap_enforced() {
    let s = setup();
    let fee_recipient = Address::generate(&s.env);

    // 201 bps exceeds the 200 bps cap
    let result = s
        .client
        .try_update_protocol_fee(&s.admin, &201, &fee_recipient);
    assert!(result.is_err());

    // 200 bps is exactly at the cap — should succeed
    s.client.update_protocol_fee(&s.admin, &200, &fee_recipient);
    let (fee_bps, _) = s.client.get_protocol_fee();
    assert_eq!(fee_bps, 200);
}

#[test]
fn test_non_admin_cannot_update_protocol_fee() {
    let s = setup();
    let non_admin = Address::generate(&s.env);
    let fee_recipient = Address::generate(&s.env);

    let result = s
        .client
        .try_update_protocol_fee(&non_admin, &100, &fee_recipient);
    assert!(result.is_err());
}

#[test]
fn test_protocol_fee_emits_event() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let fee_recipient = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.client.update_protocol_fee(&s.admin, &100, &fee_recipient);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Dispute"),
        &1000,
    );
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    let events = s.env.events().all();
    let fee_event = events
        .iter()
        .find(|e| e.1 == (Symbol::new(&s.env, "escrow_protocol_fee_paid"),).into_val(&s.env));
    assert!(fee_event.is_some());

    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = fee_event.unwrap().2.into_val(&s.env);
    let fee_amount: i128 = data
        .get(Symbol::new(&s.env, "fee_amount"))
        .unwrap()
        .into_val(&s.env);
    assert_eq!(fee_amount, 10);
}

// ===========================================================================
//  Auto-Release Expired Escrow Tests
// ===========================================================================

#[test]
fn test_auto_release_expired() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    // Advance time past deadline
    s.env.ledger().set_timestamp(deadline + 1);

    s.client.auto_release_expired(&escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Refunded);
    assert_eq!(s.token_client.balance(&buyer), 1000);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_auto_release_not_expired_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    let __typed_err_result = s.client.try_auto_release_expired(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::EscrowHasNotExpiredYet.into());
}

#[test]
fn test_auto_release_disputed_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Item not received"),
        &250,
    );

    // Advance time past deadline
    s.env.ledger().set_timestamp(deadline + 1);
    let __typed_err_result = s.client.try_auto_release_expired(&escrow_id);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::EscrowIsNotActive.into());
}

// ===========================================================================
//  Event Tests
// ===========================================================================

#[test]
fn test_escrow_created_emits_event() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    s.client
        .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    let events = s.env.events().all();
    assert!(events.len() > 0);
}

#[test]
fn test_escrow_released_emits_event() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.release_escrow(&buyer, &escrow_id);

    let events = s.env.events().all();
    assert!(events.len() > 1);
}

#[test]
fn test_partial_release_emits_event() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &250, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.partial_release(&buyer, &escrow_id, &150);

    let events = s.env.events().all();
    let last = events.last().unwrap();

    let expected_topics = (Symbol::new(&s.env, "partial_released"),).into_val(&s.env);
    assert_eq!(last.1, expected_topics);

    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = last.2.into_val(&s.env);
    let emitted_escrow_id: u32 = data
        .get(Symbol::new(&s.env, "escrow_id"))
        .unwrap()
        .into_val(&s.env);
    let released_amount: i128 = data
        .get(Symbol::new(&s.env, "released_amount"))
        .unwrap()
        .into_val(&s.env);
    let remaining_amount: i128 = data
        .get(Symbol::new(&s.env, "remaining_amount"))
        .unwrap()
        .into_val(&s.env);

    assert_eq!(emitted_escrow_id, escrow_id);
    assert_eq!(released_amount, 150);
    assert_eq!(remaining_amount, 100);
}

// ===========================================================================
//  Counter Tests
// ===========================================================================

#[test]
fn test_escrow_counter_increments() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;

    s.client
        .create_escrow(&buyer, &seller, &arbiter, &100, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    s.client
        .create_escrow(&buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    assert_eq!(s.client.get_escrow_counter(), 2);
}

#[test]
fn test_boundary_amount_i128_max_rejected_without_balance() {
    // TODO: Implement test
}

// ===========================================================================
//  Upgradeability Tests
// ===========================================================================

#[test]
fn test_admin_can_upgrade_and_version_increments() {
    let s = setup();

    assert_eq!(s.client.get_version(), 1);

    let wasm_hash = s.env.deployer().upload_contract_wasm(UPGRADE_WASM);
    s.client.upgrade(&s.admin, &wasm_hash);

    let version: u32 = s.env.as_contract(&s.client.address, || {
        s.env
            .storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap()
    });
    assert_eq!(version, 2);
}

#[test]
fn test_unauthorized_upgrade_fails() {
    let s = setup();

    let intruder = Address::generate(&s.env);
    let wasm_hash = s.env.deployer().upload_contract_wasm(UPGRADE_WASM);

    let result = s.client.try_upgrade(&intruder, &wasm_hash);
    assert!(result.is_err());
    assert_eq!(s.client.get_version(), 1);
}

#[test]
fn test_migration_runs_once_per_version() {
    let s = setup();

    s.client.migrate(&s.admin);

    let second = s.client.try_migrate(&s.admin);
    assert!(second.is_err());
}

#[test]
fn test_upgrade_atomicity_on_invalid_wasm_hash() {
    let s = setup();

    let invalid_hash = BytesN::from_array(&s.env, &[7u8; 32]);
    let result = s.client.try_upgrade(&s.admin, &invalid_hash);

    assert!(result.is_err());
    assert_eq!(s.client.get_version(), 1);
}

// ===========================================================================
//  Deadline Extension Tests
// ===========================================================================

#[test]
fn test_deadline_extension_two_party_flow() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1);

    let deadline = s.env.ledger().timestamp() + 10;
    let result = s.client.try_create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &i128::MAX,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );
    assert!(result.is_err());
}

#[test]
fn test_boundary_payment_id_u64_max_cast_not_found() {
    let s = setup();
    let id = u64::MAX as u32;
    let res = s.client.try_get_escrow(&id);
    assert!(res.is_err());
}

#[test]
fn test_auth_required_for_release_path() {
    let env = Env::default();
    let contract_id = env.register(AhjoorEscrowContract, ());
    let client = AhjoorEscrowContractClient::new(&env, &contract_id);
    let caller = Address::generate(&env);

    let res = client.try_release_escrow(&caller, &0);
    assert!(res.is_err());
}

#[test]
fn test_deadline_extension_buyer_proposes_seller_accepts() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let initial_deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &initial_deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let extended_deadline = initial_deadline + 3600;
    s.client
        .propose_deadline_extension(&buyer, &escrow_id, &extended_deadline);
    s.client.accept_deadline_extension(&seller, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.deadline, extended_deadline);
}

#[test]
fn test_deadline_extension_seller_can_propose_buyer_accepts() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let initial_deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &initial_deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let extended_deadline = initial_deadline + 7200;
    s.client
        .propose_deadline_extension(&seller, &escrow_id, &extended_deadline);
    s.client.accept_deadline_extension(&buyer, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.deadline, extended_deadline);
}

#[test]
fn test_deadline_extension_invalid_deadline_rejected() {
    // TODO: Implement test
}

// ===========================================================================
//  Pause Mechanism Tests
// ===========================================================================

#[test]
fn test_admin_can_pause_and_resume_contract() {
    let s = setup();

    let reason = String::from_str(&s.env, "Emergency maintenance");

    s.client.pause_contract(&s.admin, &reason);
    assert_eq!(s.client.is_paused(), true);
    assert_eq!(s.client.get_pause_reason(), reason);

    s.client.resume_contract(&s.admin);
    assert_eq!(s.client.is_paused(), false);
    assert_eq!(s.client.get_pause_reason(), String::from_str(&s.env, ""));
}

#[test]
fn test_non_admin_cannot_resume_contract() {
    let s = setup();

    let non_admin = Address::generate(&s.env);
    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "Incident"));

    let res = s.client.try_resume_contract(&non_admin);
    assert!(res.is_err());
}

#[test]
fn test_event_snapshot_for_dispute() {
    // TODO: Implement event snapshot test
}

#[test]
fn test_write_functions_blocked_when_paused_reads_still_work() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 500;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &200, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "snapshot"),
        &200,
    );

    let events = s.env.events().all();
    assert!(!events.is_empty());
    let snapshot = alloc::format!("{:?}", events);
    assert!(!snapshot.is_empty());
}

#[test]
fn test_fuzz_like_create_inputs_100_cases() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &10_000_000);

    let mut seed: u64 = 0xA11CE73;
    for _ in 0..100 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let amount = ((seed % 5000) as i128) + 1;
        let deadline = s.env.ledger().timestamp() + 1 + (seed % 1000);
        let _ = s.client.try_create_escrow(
            &buyer,
            &seller,
            &arbiter,
            &amount,
            &s.token_addr,
            &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );
    }

    assert!(s.client.get_escrow_counter() <= 100);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(120))]

    #[test]
    fn prop_escrow_conservation(deposit in 1i128..1_000_000, release in 0i128..1_000_000, refund in 0i128..1_000_000) {
        let released = core::cmp::min(release, deposit);
        let remaining_after_release = deposit - released;
        let refunded = core::cmp::min(refund, remaining_after_release);
        let remaining = deposit - released - refunded;

        prop_assert!(released + refunded <= deposit);
        prop_assert_eq!(released + refunded + remaining, deposit);
    }
}

#[test]
fn test_deadline_extension_cannot_be_same_as_current() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let initial_deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &initial_deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let result = s
        .client
        .try_propose_deadline_extension(&buyer, &escrow_id, &initial_deadline);
    assert!(result.is_err());
}

#[test]
fn test_deadline_extension_same_party_accept_rejected() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let initial_deadline = s.env.ledger().timestamp() + 1000;

    s.token_admin_client.mint(&buyer, &1000);

    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &initial_deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let extended_deadline = initial_deadline + 1800;
    s.client
        .propose_deadline_extension(&buyer, &escrow_id, &extended_deadline);

    let result = s.client.try_accept_deadline_extension(&buyer, &escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_deadline_extension_proposal_expiry_rejected() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let initial_deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &initial_deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let extended_deadline = initial_deadline + 1800;
    s.client
        .propose_deadline_extension(&buyer, &escrow_id, &extended_deadline);

    s.env
        .ledger()
        .set_timestamp(s.env.ledger().timestamp() + 24 * 60 * 60 + 1);

    let result = s.client.try_accept_deadline_extension(&seller, &escrow_id);
    assert!(result.is_err());

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.deadline, initial_deadline);
}

#[test]
fn test_dispute_blocks_deadline_extension() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let initial_deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &initial_deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "Need review"),
        &250,
    );

    let result =
        s.client
            .try_propose_deadline_extension(&buyer, &escrow_id, &(initial_deadline + 3600));
    assert!(result.is_err());
}

#[test]
fn test_recovery_after_resume() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id =
        s.client
            .create_escrow(&buyer, &seller, &arbiter, &100, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);

    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "Emergency"));

    let create_res =
        s.client
            .try_create_escrow(&buyer, &seller, &arbiter, &100, &s.token_addr, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    assert!(create_res.is_err());

    let release_res = s.client.try_release_escrow(&buyer, &escrow_id);
    assert!(release_res.is_err());

    s.client.resume_contract(&s.admin);

    s.client.release_escrow(&buyer, &escrow_id);
    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

// ===========================================================================
//  Multi-Token Allowlist Tests
// ===========================================================================

#[test]
fn test_admin_can_add_and_remove_allowed_token() {
    let s = setup();
    let new_token = Address::generate(&s.env);

    s.client.add_allowed_token(&s.admin, &new_token);

    s.client.remove_allowed_token(&s.admin, &new_token);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let deadline = s.env.ledger().timestamp() + 1000;

    let res = s
        .client
        .try_create_escrow(&buyer, &seller, &arbiter, &100, &new_token, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    assert!(res.is_err());
}

#[test]
fn test_non_admin_cannot_add_or_remove_allowed_token() {
    let s = setup();
    let non_admin = Address::generate(&s.env);
    let new_token = Address::generate(&s.env);

    let res = s.client.try_add_allowed_token(&non_admin, &new_token);
    assert!(res.is_err());

    let res = s.client.try_remove_allowed_token(&non_admin, &s.token_addr);
    assert!(res.is_err());
}

#[test]
fn test_create_escrow_with_disallowed_token_panics_token_not_allowed() {
    let s = setup();
    let unallowed_token = Address::generate(&s.env);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let __typed_err_result = s.client
        .try_create_escrow(&buyer, &seller, &arbiter, &250, &unallowed_token, &deadline, &None, &Vec::new(&s.env), &false, &0u32);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt2::TokenNotAllowed.into());
}

// ===========================================================================
//  Issue #145: Escrow Metadata Tests
// ===========================================================================

#[test]
fn test_create_escrow_with_metadata_hash() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let hash = BytesN::from_array(&s.env, &[1u8; 32]);
    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &Some(hash.clone()),
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let stored = s.client.get_metadata_hash(&escrow_id);
    assert_eq!(stored, Some(hash));
}

#[test]
fn test_create_escrow_without_metadata_hash() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let stored = s.client.get_metadata_hash(&escrow_id);
    assert_eq!(stored, None);
}

#[test]
fn test_update_metadata_by_buyer() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let new_hash = BytesN::from_array(&s.env, &[2u8; 32]);
    s.client.update_metadata(&buyer, &escrow_id, &new_hash);

    let stored = s.client.get_metadata_hash(&escrow_id);
    assert_eq!(stored, Some(new_hash));
}

#[test]
fn test_update_metadata_by_seller() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let new_hash = BytesN::from_array(&s.env, &[3u8; 32]);
    s.client.update_metadata(&seller, &escrow_id, &new_hash);

    let stored = s.client.get_metadata_hash(&escrow_id);
    assert_eq!(stored, Some(new_hash));
}

#[test]
fn test_update_metadata_multiple_times_only_latest_stored() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let hash1 = BytesN::from_array(&s.env, &[4u8; 32]);
    let hash2 = BytesN::from_array(&s.env, &[5u8; 32]);
    s.client.update_metadata(&buyer, &escrow_id, &hash1);
    s.client.update_metadata(&seller, &escrow_id, &hash2);

    let stored = s.client.get_metadata_hash(&escrow_id);
    assert_eq!(stored, Some(hash2));
}

#[test]
fn test_update_metadata_unauthorized_panics() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let outsider = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let hash = BytesN::from_array(&s.env, &[6u8; 32]);
    let __typed_err_result = s.client.try_update_metadata(&outsider, &escrow_id, &hash);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt2::OnlyBuyerOrSellerCanUpdateMetadata.into());
}

// ===========================================================================
//  Issue #148: Multi-Party Escrow Tests
// ===========================================================================

#[test]
fn test_create_multi_party_escrow_two_sellers() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller1 = Address::generate(&s.env);
    let seller2 = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let mut sellers: Vec<(Address, u32)> = Vec::new(&s.env);
    sellers.push_back((seller1.clone(), 6000u32));
    sellers.push_back((seller2.clone(), 4000u32));

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller1,
        &arbiter,
        &1000,
        &s.token_addr,
        &deadline,
        &None,
        &sellers,
        &false,
        &0u32,
    );

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.amount, 1000);
    assert_eq!(escrow.sellers.len(), 2);
}

#[test]
fn test_release_multi_party_escrow_two_sellers_proportional() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller1 = Address::generate(&s.env);
    let seller2 = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let mut sellers: Vec<(Address, u32)> = Vec::new(&s.env);
    sellers.push_back((seller1.clone(), 6000u32));
    sellers.push_back((seller2.clone(), 4000u32));

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller1,
        &arbiter,
        &1000,
        &s.token_addr,
        &deadline,
        &None,
        &sellers,
        &false,
        &0u32,
    );

    s.client.release_escrow(&buyer, &escrow_id);

    // seller1 gets 60% = 600, seller2 gets 40% = 400
    assert_eq!(s.token_client.balance(&seller1), 600);
    assert_eq!(s.token_client.balance(&seller2), 400);
}

#[test]
fn test_release_multi_party_escrow_five_sellers() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let s1 = Address::generate(&s.env);
    let s2 = Address::generate(&s.env);
    let s3 = Address::generate(&s.env);
    let s4 = Address::generate(&s.env);
    let s5 = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &10000);

    let mut sellers: Vec<(Address, u32)> = Vec::new(&s.env);
    sellers.push_back((s1.clone(), 2000u32));
    sellers.push_back((s2.clone(), 2000u32));
    sellers.push_back((s3.clone(), 2000u32));
    sellers.push_back((s4.clone(), 2000u32));
    sellers.push_back((s5.clone(), 2000u32));

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &s1,
        &arbiter,
        &10000,
        &s.token_addr,
        &deadline,
        &None,
        &sellers,
        &false,
        &0u32,
    );

    s.client.release_escrow(&buyer, &escrow_id);

    assert_eq!(s.token_client.balance(&s1), 2000);
    assert_eq!(s.token_client.balance(&s2), 2000);
    assert_eq!(s.token_client.balance(&s3), 2000);
    assert_eq!(s.token_client.balance(&s4), 2000);
    assert_eq!(s.token_client.balance(&s5), 2000);
}

#[test]
fn test_release_multi_party_dust_goes_to_first_seller() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller1 = Address::generate(&s.env);
    let seller2 = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1001);

    let mut sellers: Vec<(Address, u32)> = Vec::new(&s.env);
    sellers.push_back((seller1.clone(), 5000u32));
    sellers.push_back((seller2.clone(), 5000u32));

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller1,
        &arbiter,
        &1001,
        &s.token_addr,
        &deadline,
        &None,
        &sellers,
        &false,
        &0u32,
    );

    s.client.release_escrow(&buyer, &escrow_id);

    // seller2 gets floor(1001 * 5000 / 10000) = 500
    // seller1 gets remainder = 1001 - 500 = 501
    assert_eq!(s.token_client.balance(&seller2), 500);
    assert_eq!(s.token_client.balance(&seller1), 501);
}

#[test]
fn test_create_multi_party_escrow_invalid_bps_panics() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller1 = Address::generate(&s.env);
    let seller2 = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let mut sellers: Vec<(Address, u32)> = Vec::new(&s.env);
    sellers.push_back((seller1.clone(), 5000u32));
    sellers.push_back((seller2.clone(), 3000u32)); // only 8000 bps total

    let deadline = s.env.ledger().timestamp() + 1000;
    let __typed_err_result = s.client.try_create_escrow(
        &buyer,
        &seller1,
        &arbiter,
        &1000,
        &s.token_addr,
        &deadline,
        &None,
        &sellers,
        &false,
        &0u32,
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::SellerAllocationsMustSumTo10000Bps.into());
}

#[test]
fn test_create_multi_party_escrow_too_many_sellers_panics() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    // 6 sellers each with 1666 bps + 1 extra to reach 10000 — but 6 > 5 limit
    let mut sellers: Vec<(Address, u32)> = Vec::new(&s.env);
    sellers.push_back((Address::generate(&s.env), 2000u32));
    sellers.push_back((Address::generate(&s.env), 2000u32));
    sellers.push_back((Address::generate(&s.env), 2000u32));
    sellers.push_back((Address::generate(&s.env), 2000u32));
    sellers.push_back((Address::generate(&s.env), 1000u32));
    sellers.push_back((Address::generate(&s.env), 1000u32)); // 6th seller

    let deadline = s.env.ledger().timestamp() + 1000;
    let __typed_err_result = s.client.try_create_escrow(
        &buyer,
        &Address::generate(&s.env),
        &arbiter,
        &1000,
        &s.token_addr,
        &deadline,
        &None,
        &sellers,
        &false,
        &0u32,
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowError::Maximum5SellersAllowed.into());
}

// ===========================================================================
//  Issue #141: Evidence Anchoring Tests
// ===========================================================================

#[test]
fn test_submit_and_retrieve_evidence_hashes() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let evidence_hash = BytesN::from_array(&s.env, &[7u8; 32]);
    let uri_hash = BytesN::from_array(&s.env, &[8u8; 32]);

    s.client
        .submit_evidence(&buyer, &escrow_id, &evidence_hash, &uri_hash);

    let all = s.client.get_evidence(&escrow_id);
    assert_eq!(all.len(), 1);

    let (party, entries) = all.get(0).unwrap();
    assert_eq!(party, buyer);
    assert_eq!(entries.len(), 1);

    let entry = entries.get(0).unwrap();
    assert_eq!(entry.evidence_hash, evidence_hash);
    assert_eq!(entry.evidence_uri_hash, uri_hash);
}

#[test]
fn test_evidence_over_limit_rejected() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    for i in 0..5u8 {
        let hash = BytesN::from_array(&s.env, &[i; 32]);
        let uri = BytesN::from_array(&s.env, &[i + 1; 32]);
        s.client.submit_evidence(&buyer, &escrow_id, &hash, &uri);
    }

    let result = s.client.try_submit_evidence(
        &buyer,
        &escrow_id,
        &BytesN::from_array(&s.env, &[9u8; 32]),
        &BytesN::from_array(&s.env, &[10u8; 32]),
    );
    assert!(result.is_err());
}

#[test]
fn test_only_parties_can_submit_evidence() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let outsider = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let result = s.client.try_submit_evidence(
        &outsider,
        &escrow_id,
        &BytesN::from_array(&s.env, &[11u8; 32]),
        &BytesN::from_array(&s.env, &[12u8; 32]),
    );
    assert!(result.is_err());
}

// ===========================================================================
//  Issue #144: Auto-Renew Tests
// ===========================================================================

#[test]
fn test_auto_renew_creates_next_escrow_on_release() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &10_000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &500,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &true,
        &2u32,
    );

    s.client.set_renewal_allowance(&buyer, &escrow_id, &2u32);
    s.client.release_escrow(&buyer, &escrow_id);

    let next_id = escrow_id + 1;
    let renewed = s.client.get_escrow(&next_id);
    assert_eq!(renewed.status, EscrowStatus::Active);
    assert_eq!(renewed.amount, 500);
    assert_eq!(renewed.extensions.renewals_remaining, 1);
}

#[test]
fn test_auto_renew_stops_when_count_exhausted() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &10_000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &500,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &true,
        &1u32,
    );

    s.client.set_renewal_allowance(&buyer, &escrow_id, &1u32);
    s.client.release_escrow(&buyer, &escrow_id);

    let second_id = escrow_id + 1;
    s.client.release_escrow(&buyer, &second_id);

    let third_id = escrow_id + 2;
    let no_third = s.client.try_get_escrow(&third_id);
    assert!(no_third.is_err());
}

#[test]
fn test_buyer_can_cancel_auto_renew() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &10_000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &500,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &true,
        &2u32,
    );

    s.client.set_renewal_allowance(&buyer, &escrow_id, &2u32);
    s.client.cancel_auto_renew(&buyer, &escrow_id);
    s.client.release_escrow(&buyer, &escrow_id);

    let next = s.client.try_get_escrow(&(escrow_id + 1));
    assert!(next.is_err());
}

#[test]
fn test_auto_renew_fails_with_insufficient_allowance() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &10_000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &500,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &true,
        &2u32,
    );

    s.client.set_renewal_allowance(&buyer, &escrow_id, &1u32);
    s.client.release_escrow(&buyer, &escrow_id);
    let second_id = escrow_id + 1;

    let result = s.client.try_release_escrow(&buyer, &second_id);
    assert!(result.is_err());
}

#[test]
fn test_transfer_buyer_role_requires_current_buyer_authorization() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let outsider = Address::generate(&s.env);
    let new_buyer = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    let result = s
        .client
        .try_transfer_buyer_role(&outsider, &escrow_id, &new_buyer);
    assert!(result.is_err());

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.buyer, buyer);
}

#[test]
fn test_transfer_buyer_role_rejected_during_dispute() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let new_buyer = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    s.client.dispute_escrow(
        &buyer,
        &escrow_id,
        &String::from_str(&s.env, "dispute"),
        &250,
    );

    let result = s
        .client
        .try_transfer_buyer_role(&buyer, &escrow_id, &new_buyer);
    assert!(result.is_err());

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.buyer, buyer);
}

#[test]
fn test_transfer_buyer_role_new_buyer_inherits_rights_old_buyer_loses_them() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let new_buyer = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0u32,
    );

    s.client
        .transfer_buyer_role(&buyer, &escrow_id, &new_buyer);

    let transfer_event = s
        .env
        .events()
        .all()
        .last()
        .expect("Expected buyer role transfer event");
    let data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = transfer_event.2.into_val(&s.env);
    let event_escrow_id: u32 = data
        .get(Symbol::new(&s.env, "escrow_id"))
        .unwrap()
        .into_val(&s.env);
    let event_old_buyer: Address = data
        .get(Symbol::new(&s.env, "old_buyer"))
        .unwrap()
        .into_val(&s.env);
    let event_new_buyer: Address = data
        .get(Symbol::new(&s.env, "new_buyer"))
        .unwrap()
        .into_val(&s.env);

    assert_eq!(event_escrow_id, escrow_id);
    assert_eq!(event_old_buyer, buyer);
    assert_eq!(event_new_buyer, new_buyer);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.buyer, new_buyer);

    let old_buyer_release = s.client.try_release_escrow(&buyer, &escrow_id);
    assert!(old_buyer_release.is_err());

    s.client.release_escrow(&new_buyer, &escrow_id);
    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

fn make_batch_config(
    env: &Env,
    seller: Address,
    arbiter: Address,
    amount: i128,
    token: Address,
    deadline: u64,
) -> EscrowBatchConfig {
    EscrowBatchConfig {
        seller,
        arbiter,
        amount,
        token,
        deadline,
        metadata_hash: None,
        sellers: Vec::new(env),
        auto_renew: false,
        renewal_count: 0,
    }
}

#[test]
fn test_create_escrows_batch_max_size_contiguous_and_events() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &20_000);

    let mut configs: Vec<EscrowBatchConfig> = Vec::new(&s.env);
    let deadline = s.env.ledger().timestamp() + 1000;
    for _ in 0..10 {
        configs.push_back(make_batch_config(
            &s.env,
            Address::generate(&s.env),
            Address::generate(&s.env),
            100,
            s.token_addr.clone(),
            deadline,
        ));
    }

    let ids = s.client.create_escrows_batch(&buyer, &configs);
    assert_eq!(ids.len(), 10);
    for i in 0..ids.len() {
        assert_eq!(ids.get(i).unwrap(), i);
    }

    let events = s.env.events().all();
    let created_topic = (Symbol::new(&s.env, "escrow_created"),).into_val(&s.env);
    let mut created_count: u32 = 0;
    for i in 0..events.len() {
        let evt = events.get(i).unwrap();
        if evt.1 == created_topic {
            created_count += 1;
        }
    }
    assert_eq!(created_count, 10);

    let batch_topic = (Symbol::new(&s.env, "batch_escrow_created"),).into_val(&s.env);
    let summary = events
        .iter()
        .find(|e| e.1 == batch_topic)
        .expect("Expected batch summary event");

    let summary_data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = summary.2.into_val(&s.env);
    let count: u32 = summary_data
        .get(Symbol::new(&s.env, "count"))
        .unwrap()
        .into_val(&s.env);
    let first_id: u32 = summary_data
        .get(Symbol::new(&s.env, "first_id"))
        .unwrap()
        .into_val(&s.env);
    let last_id: u32 = summary_data
        .get(Symbol::new(&s.env, "last_id"))
        .unwrap()
        .into_val(&s.env);

    assert_eq!(count, 10);
    assert_eq!(first_id, 0);
    assert_eq!(last_id, 9);
}

#[test]
fn test_create_escrows_batch_rejects_above_cap() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &20_000);

    let mut configs: Vec<EscrowBatchConfig> = Vec::new(&s.env);
    let deadline = s.env.ledger().timestamp() + 1000;
    for _ in 0..11 {
        configs.push_back(make_batch_config(
            &s.env,
            Address::generate(&s.env),
            Address::generate(&s.env),
            50,
            s.token_addr.clone(),
            deadline,
        ));
    }

    let res = s.client.try_create_escrows_batch(&buyer, &configs);
    assert!(res.is_err());
    assert_eq!(s.client.get_escrow_counter(), 0);
    assert_eq!(s.token_client.balance(&buyer), 20_000);
}

#[test]
fn test_create_escrows_batch_invalid_config_reverts_entire_batch() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &20_000);

    let mut configs: Vec<EscrowBatchConfig> = Vec::new(&s.env);
    let deadline = s.env.ledger().timestamp() + 1000;

    configs.push_back(make_batch_config(
        &s.env,
        Address::generate(&s.env),
        Address::generate(&s.env),
        100,
        s.token_addr.clone(),
        deadline,
    ));
    configs.push_back(make_batch_config(
        &s.env,
        Address::generate(&s.env),
        Address::generate(&s.env),
        0,
        s.token_addr.clone(),
        deadline,
    ));
    configs.push_back(make_batch_config(
        &s.env,
        Address::generate(&s.env),
        Address::generate(&s.env),
        100,
        s.token_addr.clone(),
        deadline,
    ));

    let res = s.client.try_create_escrows_batch(&buyer, &configs);
    assert!(res.is_err());
    assert_eq!(s.client.get_escrow_counter(), 0);
    assert_eq!(s.token_client.balance(&buyer), 20_000);
    assert!(s.client.try_get_escrow(&0).is_err());
}

#[test]
fn test_create_escrows_batch_single_item() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let mut configs: Vec<EscrowBatchConfig> = Vec::new(&s.env);
    let deadline = s.env.ledger().timestamp() + 1000;
    configs.push_back(make_batch_config(
        &s.env,
        Address::generate(&s.env),
        Address::generate(&s.env),
        250,
        s.token_addr.clone(),
        deadline,
    ));

    let ids = s.client.create_escrows_batch(&buyer, &configs);
    assert_eq!(ids.len(), 1);
    assert_eq!(ids.get(0).unwrap(), 0);

    let events = s.env.events().all();
    let batch_topic = (Symbol::new(&s.env, "batch_escrow_created"),).into_val(&s.env);
    let summary = events
        .iter()
        .find(|e| e.1 == batch_topic)
        .expect("Expected batch summary event");
    let summary_data: soroban_sdk::Map<Symbol, soroban_sdk::Val> = summary.2.into_val(&s.env);

    let count: u32 = summary_data
        .get(Symbol::new(&s.env, "count"))
        .unwrap()
        .into_val(&s.env);
    let first_id: u32 = summary_data
        .get(Symbol::new(&s.env, "first_id"))
        .unwrap()
        .into_val(&s.env);
    let last_id: u32 = summary_data
        .get(Symbol::new(&s.env, "last_id"))
        .unwrap()
        .into_val(&s.env);

    assert_eq!(count, 1);
    assert_eq!(first_id, 0);
    assert_eq!(last_id, 0);
}



// ===========================================================================
//  Issue #137: Insurance Pool Tests
// ===========================================================================

fn setup_insurance<'a>(s: &'a TestSetup<'a>) {
    s.client.set_insurance_config(&s.admin, &s.token_addr, &7u64);
}

#[test]
fn test_insurance_contribute_and_pool_balance() {
    let s = setup();
    setup_insurance(&s);

    let contributor = Address::generate(&s.env);
    s.token_admin_client.mint(&contributor, &1000);

    s.client.contribute_to_insurance(&contributor, &500);
    assert_eq!(s.client.get_insurance_pool(), 500);

    s.client.contribute_to_insurance(&contributor, &300);
    assert_eq!(s.client.get_insurance_pool(), 800);
}

#[test]
fn test_insurance_claim_after_trigger_period() {
    let s = setup();
    setup_insurance(&s);

    let contributor = Address::generate(&s.env);
    s.token_admin_client.mint(&contributor, &10_000);
    s.client.contribute_to_insurance(&contributor, &10_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "stuck"), &1000);

    // Advance past 7-day trigger (7 * 24 * 60 * 60 = 604800 seconds)
    s.env.ledger().set_timestamp(1000 + 604_801);

    s.client.confirm_insurance_inactivity(&s.admin, &escrow_id, &true);
    s.client.claim_insurance(&buyer, &escrow_id);

    // Claim is capped at 50% of escrow amount = 500
    assert_eq!(s.token_client.balance(&buyer), 500);
    assert_eq!(s.client.get_insurance_pool(), 9_500);
}

#[test]
fn test_insurance_claim_capped_at_50_percent() {
    let s = setup();
    setup_insurance(&s);

    let contributor = Address::generate(&s.env);
    s.token_admin_client.mint(&contributor, &10_000);
    s.client.contribute_to_insurance(&contributor, &10_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2000);

    s.env.ledger().set_timestamp(1000);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &2000, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "stuck"), &2000);
    s.env.ledger().set_timestamp(1000 + 604_801);
    s.client.confirm_insurance_inactivity(&s.admin, &escrow_id, &true);
    s.client.claim_insurance(&buyer, &escrow_id);

    // 50% of 2000 = 1000
    assert_eq!(s.token_client.balance(&buyer), 1000);
}

#[test]
fn test_insurance_claim_before_trigger_period_panics() {
    let s = setup();
    setup_insurance(&s);

    let contributor = Address::generate(&s.env);
    s.token_admin_client.mint(&contributor, &10_000);
    s.client.contribute_to_insurance(&contributor, &10_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "stuck"), &1000);
    // Only 1 day elapsed, not 7
    s.env.ledger().set_timestamp(1000 + 86_400);
    s.client.confirm_insurance_inactivity(&s.admin, &escrow_id, &true);

    let result = s.client.try_claim_insurance(&buyer, &escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_insurance_claim_requires_admin_confirmation() {
    let s = setup();
    setup_insurance(&s);

    let contributor = Address::generate(&s.env);
    s.token_admin_client.mint(&contributor, &10_000);
    s.client.contribute_to_insurance(&contributor, &10_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "stuck"), &1000);
    s.env.ledger().set_timestamp(1000 + 604_801);
    // No admin confirmation

    let result = s.client.try_claim_insurance(&buyer, &escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_insurance_claim_emits_event() {
    let s = setup();
    setup_insurance(&s);

    let contributor = Address::generate(&s.env);
    s.token_admin_client.mint(&contributor, &10_000);
    s.client.contribute_to_insurance(&contributor, &10_000);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "stuck"), &1000);
    s.env.ledger().set_timestamp(1000 + 604_801);
    s.client.confirm_insurance_inactivity(&s.admin, &escrow_id, &true);
    s.client.claim_insurance(&buyer, &escrow_id);

    let events = s.env.events().all();
    let topic = (Symbol::new(&s.env, "insurance_claimed"),).into_val(&s.env);
    let found = events.iter().any(|e| e.1 == topic);
    assert!(found, "Expected insurance_claimed event");
}

#[test]
fn test_insurance_pool_capped_by_pool_balance() {
    let s = setup();
    setup_insurance(&s);

    // Pool only has 100, escrow is 1000 → claim = min(500, 100) = 100
    let contributor = Address::generate(&s.env);
    s.token_admin_client.mint(&contributor, &100);
    s.client.contribute_to_insurance(&contributor, &100);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let deadline = s.env.ledger().timestamp() + 10_000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "stuck"), &1000);
    s.env.ledger().set_timestamp(1000 + 604_801);
    s.client.confirm_insurance_inactivity(&s.admin, &escrow_id, &true);
    s.client.claim_insurance(&buyer, &escrow_id);

    assert_eq!(s.token_client.balance(&buyer), 100);
    assert_eq!(s.client.get_insurance_pool(), 0);
}

// ===========================================================================
//  Issue #138: Arbitration Fee Mechanism Tests
// ===========================================================================

#[test]
fn test_arbiter_fee_deducted_from_loser_seller_wins() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    // 500 bps = 5% arbiter fee
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: Some(500u32),
        dispute_default_winner: None,
        auto_renew_max_renewals: None,

        auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "dispute"), &1000);
    // Seller wins
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    // fee = 1000 * 500 / 10000 = 50; seller gets 950
    assert_eq!(s.token_client.balance(&arbiter), 50);
    assert_eq!(s.token_client.balance(&seller), 950);
    assert_eq!(s.token_client.balance(&buyer), 0);
}

#[test]
fn test_arbiter_fee_deducted_from_loser_buyer_wins() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: Some(500u32),
        dispute_default_winner: None,
        auto_renew_max_renewals: None,

        auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "dispute"), &1000);
    // Buyer wins
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32);

    // fee = 50; buyer gets 950
    assert_eq!(s.token_client.balance(&arbiter), 50);
    assert_eq!(s.token_client.balance(&buyer), 950);
    assert_eq!(s.token_client.balance(&seller), 0);
}

#[test]
fn test_arbiter_fee_zero_is_valid() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: Some(0u32),
        dispute_default_winner: None,
        auto_renew_max_renewals: None,

        auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "dispute"), &1000);
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    assert_eq!(s.token_client.balance(&arbiter), 0);
    assert_eq!(s.token_client.balance(&seller), 1000);
}

#[test]
fn test_arbiter_fee_cap_enforced_at_creation() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: Some(1001u32), // exceeds 1000 bps max
        dispute_default_winner: None,
        auto_renew_max_renewals: None,

        auto_renew_interval_ledgers: None,    };
    let result = s.client.try_create_escrow_v2(&buyer, &request);
    assert!(result.is_err());
}

#[test]
fn test_default_arbiter_fee_applies_when_escrow_fee_not_set() {
    let s = setup();

    // Set protocol-wide default to 200 bps (2%)
    s.client.set_default_arbiter_fee_bps(&s.admin, &200u32);

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &1000, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "dispute"), &1000);
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    // fee = 1000 * 200 / 10000 = 20
    assert_eq!(s.token_client.balance(&arbiter), 20);
    assert_eq!(s.token_client.balance(&seller), 980);
}

#[test]
fn test_arbiter_fee_emits_event() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: Some(300u32),
        dispute_default_winner: None,
        auto_renew_max_renewals: None,

        auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "dispute"), &1000);
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    let events = s.env.events().all();
    let topic = (Symbol::new(&s.env, "arbiter_fee_paid"),).into_val(&s.env);
    let found = events.iter().any(|e| e.1 == topic);
    assert!(found, "Expected arbiter_fee_paid event");
}

// ===========================================================================
//  Issue #139: Time-Locked Escrow Tests
// ===========================================================================

#[test]
fn test_release_before_min_lock_until_panics() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let lock_until = 2000u64;
    let deadline = 3000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 500,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: Some(lock_until),
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Still before lock_until
    s.env.ledger().set_timestamp(1500);
    let result = s.client.try_release_escrow(&buyer, &escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_release_at_min_lock_until_succeeds() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let lock_until = 2000u64;
    let deadline = 3000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 500,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: Some(lock_until),
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Exactly at lock_until
    s.env.ledger().set_timestamp(2000);
    s.client.release_escrow(&buyer, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&seller), 500);
}

#[test]
fn test_release_after_min_lock_until_succeeds() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let lock_until = 2000u64;
    let deadline = 5000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 500,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: Some(lock_until),
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    s.env.ledger().set_timestamp(2500);
    s.client.release_escrow(&buyer, &escrow_id);

    assert_eq!(s.token_client.balance(&seller), 500);
}

#[test]
fn test_dispute_not_blocked_by_lock() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(1000);
    let lock_until = 5000u64;
    let deadline = 6000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 500,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: Some(lock_until),
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Still locked, but dispute should work
    s.env.ledger().set_timestamp(2000);
    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "issue"), &500);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_lock_and_deadline_independently_configurable() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let lock_until = 500u64;
    let deadline = 1000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 500,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: Some(lock_until),
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.extensions.min_lock_until, Some(lock_until));
    assert_eq!(escrow.deadline, deadline);
}

#[test]
fn test_time_locked_escrow_emits_event() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let lock_until = 500u64;
    let deadline = 1000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 500,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: Some(lock_until),
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    s.client.create_escrow_v2(&buyer, &request);

    let events = s.env.events().all();
    let topic = (Symbol::new(&s.env, "escrow_time_locked"),).into_val(&s.env);
    let found = events.iter().any(|e| e.1 == topic);
    assert!(found, "Expected escrow_time_locked event");
}

#[test]
fn test_deadline_must_be_after_lock_until() {
    let s = setup();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    // deadline == lock_until → should panic
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 500,
        token: s.token_addr.clone(),
        deadline: 500,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: Some(500u64),
        release_base: None,
        release_quote: None,
        release_comparison: None,
        release_threshold_price: None,
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let result = s.client.try_create_escrow_v2(&buyer, &request);
    assert!(result.is_err());
}

// ===========================================================================
//  Issue #140: Oracle-Conditional Escrow Release Tests
// ===========================================================================

/// Mock oracle for escrow oracle-release tests.
mod escrow_mock_oracle {
    use crate::PriceData;
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};

    #[contracttype]
    enum OracleKey {
        Price,
        Ts,
    }

    #[contract]
    pub struct EscrowMockOracle;

    #[contractimpl]
    impl EscrowMockOracle {
        pub fn set_price(env: Env, price: i128, timestamp: u64) {
            env.storage().instance().set(&OracleKey::Price, &price);
            env.storage().instance().set(&OracleKey::Ts, &timestamp);
        }

        pub fn lastprice(env: Env, _base: Address, _quote: Address) -> Option<PriceData> {
            let price: i128 = env.storage().instance().get(&OracleKey::Price)?;
            let timestamp: u64 = env.storage().instance().get(&OracleKey::Ts)?;
            Some(PriceData { price, timestamp })
        }
    }
}

use escrow_mock_oracle::EscrowMockOracle;

struct OracleEscrowSetup<'a> {
    env: Env,
    client: AhjoorEscrowContractClient<'a>,
    admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
    oracle_addr: Address,
    base: Address,
    quote: Address,
}

fn setup_oracle_escrow<'a>() -> OracleEscrowSetup<'a> {
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

    let oracle_addr = env.register(EscrowMockOracle, ());
    let base = Address::generate(&env);
    let quote = Address::generate(&env);

    client.initialize(&admin);
    client.add_allowed_token(&admin, &token_addr);
    client.set_oracle(&admin, &oracle_addr, &300u64);

    OracleEscrowSetup {
        env,
        client,
        admin,
        token_addr,
        token_client,
        token_admin_client,
        oracle_addr,
        base,
        quote,
    }
}

fn set_escrow_oracle_price(s: &OracleEscrowSetup, price: i128, ts: u64) {
    use escrow_mock_oracle::EscrowMockOracleClient;
    let oc = EscrowMockOracleClient::new(&s.env, &s.oracle_addr);
    oc.set_price(&price, &ts);
    s.env.ledger().set_timestamp(ts);
}

#[test]
fn test_oracle_release_triggers_when_price_below_threshold() {
    let s = setup_oracle_escrow();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = 5000u64;

    // LessOrEqual condition: release when price <= 500
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: Some(s.base.clone()),
        release_quote: Some(s.quote.clone()),
        release_comparison: Some(0u32), // LessOrEqual
        release_threshold_price: Some(500),
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Price = 400 <= 500 → condition met
    set_escrow_oracle_price(&s, 400, 200);
    s.client.check_and_release_escrow(&escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&seller), 1000);
}

#[test]
fn test_oracle_release_does_not_trigger_when_price_above_threshold() {
    let s = setup_oracle_escrow();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = 5000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: Some(s.base.clone()),
        release_quote: Some(s.quote.clone()),
        release_comparison: Some(0u32), // LessOrEqual
        release_threshold_price: Some(500),
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Price = 600 > 500 → condition NOT met
    set_escrow_oracle_price(&s, 600, 200);
    let result = s.client.try_check_and_release_escrow(&escrow_id);
    assert!(result.is_err());

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Active);
}

#[test]
fn test_oracle_release_triggers_when_price_at_threshold() {
    let s = setup_oracle_escrow();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = 5000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: Some(s.base.clone()),
        release_quote: Some(s.quote.clone()),
        release_comparison: Some(0u32), // LessOrEqual
        release_threshold_price: Some(500),
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Price exactly at threshold = 500 → condition met (<=)
    set_escrow_oracle_price(&s, 500, 200);
    s.client.check_and_release_escrow(&escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_oracle_release_greater_or_equal_condition() {
    let s = setup_oracle_escrow();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = 5000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: Some(s.base.clone()),
        release_quote: Some(s.quote.clone()),
        release_comparison: Some(1u32), // GreaterOrEqual
        release_threshold_price: Some(1000),
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Price = 1200 >= 1000 → condition met
    set_escrow_oracle_price(&s, 1200, 200);
    s.client.check_and_release_escrow(&escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
}

#[test]
fn test_oracle_stale_price_blocks_release() {
    let s = setup_oracle_escrow();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = 5000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: Some(s.base.clone()),
        release_quote: Some(s.quote.clone()),
        release_comparison: Some(0u32),
        release_threshold_price: Some(500),
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Set price at ts=0, advance ledger to ts=500 → age=500 > max_oracle_age(300)
    use escrow_mock_oracle::EscrowMockOracleClient;
    let oc = EscrowMockOracleClient::new(&s.env, &s.oracle_addr);
    oc.set_price(&400, &0u64);
    s.env.ledger().set_timestamp(500);

    let result = s.client.try_check_and_release_escrow(&escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_manual_release_works_regardless_of_oracle_condition() {
    let s = setup_oracle_escrow();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = 5000u64;

    // Oracle condition: price <= 500, but we won't set a price that meets it
    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: Some(s.base.clone()),
        release_quote: Some(s.quote.clone()),
        release_comparison: Some(0u32),
        release_threshold_price: Some(500),
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    // Manual release by buyer still works regardless of oracle
    s.client.release_escrow(&buyer, &escrow_id);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Released);
    assert_eq!(s.token_client.balance(&seller), 1000);
}

#[test]
fn test_oracle_release_emits_event() {
    let s = setup_oracle_escrow();

    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.env.ledger().set_timestamp(100);
    let deadline = 5000u64;

    let request = EscrowCreateRequest {
        seller: seller.clone(),
        arbiter: arbiter.clone(),
        amount: 1000,
        token: s.token_addr.clone(),
        deadline,
        metadata_hash: None,
        sellers: Vec::new(&s.env),
        auto_renew: false,
        renewal_count: 0,
        buyer_inactivity_secs: 0,
        min_lock_until: None,
        release_base: Some(s.base.clone()),
        release_quote: Some(s.quote.clone()),
        release_comparison: Some(0u32),
        release_threshold_price: Some(500),
        arbiter_fee_bps: None,
            dispute_default_winner: None,
            auto_renew_max_renewals: None,

            auto_renew_interval_ledgers: None,    };
    let escrow_id = s.client.create_escrow_v2(&buyer, &request);

    set_escrow_oracle_price(&s, 300, 200);
    s.client.check_and_release_escrow(&escrow_id);

    let events = s.env.events().all();
    let topic = (Symbol::new(&s.env, "oracle_release_triggered"),).into_val(&s.env);
    let found = events.iter().any(|e| e.1 == topic);
    assert!(found, "Expected oracle_release_triggered event");
}

// ===========================================================================
//  #136 — Milestone-Based Escrow Release
// ===========================================================================

fn make_milestones(env: &Env, amounts: &[i128]) -> soroban_sdk::Vec<Milestone> {
    let mut v = soroban_sdk::Vec::new(env);
    for amt in amounts.iter() {
        v.push_back(Milestone {
            description_hash: BytesN::from_array(env, &[0u8; 32]),
            amount: *amt,
            status: MilestoneStatus::Pending,
        });
    }
    v
}

#[test]
fn test_create_milestone_escrow_locks_total() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[100, 200, 300]),
    );

    assert_eq!(s.token_client.balance(&buyer), 400);
    let milestones = s.client.get_milestones(&escrow_id);
    assert_eq!(milestones.len(), 3);
    assert_eq!(milestones.get(0).unwrap().amount, 100);
    assert_eq!(milestones.get(1).unwrap().status, MilestoneStatus::Pending);
}

#[test]
fn test_milestone_partial_release_pays_seller() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[100, 200, 300]),
    );

    s.client.approve_milestone(&buyer, &escrow_id, &0);

    assert_eq!(s.token_client.balance(&seller), 100);
    let m = s.client.get_milestones(&escrow_id);
    assert_eq!(m.get(0).unwrap().status, MilestoneStatus::Approved);
    assert_eq!(m.get(1).unwrap().status, MilestoneStatus::Pending);
    assert_eq!(s.client.get_escrow(&escrow_id).status, EscrowStatus::Active);
}

#[test]
fn test_milestone_full_release_transitions_to_released() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[100, 200, 300]),
    );

    s.client.approve_milestone(&buyer, &escrow_id, &0);
    s.client.approve_milestone(&buyer, &escrow_id, &1);
    s.client.approve_milestone(&arbiter, &escrow_id, &2);

    assert_eq!(s.token_client.balance(&seller), 600);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
    assert_eq!(s.client.get_escrow(&escrow_id).status, EscrowStatus::Released);
}

#[test]
fn test_double_approve_milestone_panics() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[100, 200, 300]),
    );

    s.client.approve_milestone(&buyer, &escrow_id, &0);
    let __typed_err_result = s.client.try_approve_milestone(&buyer, &escrow_id, &0);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::MilestoneNotPending.into());
}

#[test]
fn test_seller_cannot_approve_own_milestone() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[100, 200, 300]),
    );
    let __typed_err_result = s.client.try_approve_milestone(&seller, &escrow_id, &0);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::OnlyBuyerOrArbiterCanApproveMilestones.into());
}

#[test]
fn test_zero_milestone_amount_rejected() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let __typed_err_result = s.client.try_create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[0, 100]),
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::MilestoneAmountMustBePositive.into());
}

#[test]
fn test_empty_milestones_rejected() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let __typed_err_result = s.client.try_create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[]),
    );
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::AtLeastOneMilestoneRequired.into());
}

#[test]
fn test_milestone_out_of_range_rejected() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_milestone_escrow(
        &buyer,
        &seller,
        &arbiter,
        &s.token_addr,
        &deadline,
        &make_milestones(&s.env, &[100, 200]),
    );
    let __typed_err_result = s.client.try_approve_milestone(&buyer, &escrow_id, &5);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt::MilestoneIndexOutRange.into());
}

// ------------------------------
// Top-up Tests
// ------------------------------

#[test]
fn test_single_top_up() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Check initial state
    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.amount, 250);
    assert_eq!(escrow.original_amount, 250);
    assert!(escrow.top_up_history.is_empty());
    assert!(escrow.top_up_acknowledged);

    // Perform top-up
    s.client.top_up_escrow(&buyer, &escrow_id, &150);

    // Check new state
    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.amount, 400);
    assert_eq!(escrow.original_amount, 250);
    assert_eq!(escrow.top_up_history.len(), 1);
    assert_eq!(escrow.top_up_history.get(0).unwrap().amount, 150);
    assert_eq!(escrow.top_up_history.get(0).unwrap().cumulative_total, 400);
    assert!(!escrow.top_up_acknowledged);

    // Check token balances
    assert_eq!(s.token_client.balance(&buyer), 600); // 1000 - 250 -150
    assert_eq!(s.token_client.balance(&s.client.address), 400);
}

#[test]
fn test_multiple_top_ups() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &200,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // First top-up
    s.client.top_up_escrow(&buyer, &escrow_id, &100);
    // Second top-up
    s.client.top_up_escrow(&buyer, &escrow_id, &200);

    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.amount, 500);
    assert_eq!(escrow.top_up_history.len(), 2);
    assert_eq!(escrow.top_up_history.get(0).unwrap().cumulative_total, 300);
    assert_eq!(escrow.top_up_history.get(1).unwrap().cumulative_total, 500);
}

#[test]
fn test_top_up_limit_exceeded() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &2000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &200,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Max top-up is 3x original (200*3=600, so 400 extra allowed)
    let __typed_err_result = s.client.try_top_up_escrow(&buyer, &escrow_id, &450);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt2::TopUpLimitExceeded.into());
}

#[test]
fn test_top_up_after_release_rejected() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Release escrow
    s.client.release_escrow(&buyer, &escrow_id);

    // Try to top up
    let __typed_err_result = s.client.try_top_up_escrow(&buyer, &escrow_id, &100);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt2::EscrowIsNotActiveOrAwaitingInspection.into());
}

#[test]
fn test_seller_acknowledge_topup() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Top up
    s.client.top_up_escrow(&buyer, &escrow_id, &150);
    assert!(!s.client.get_escrow(&escrow_id).top_up_acknowledged);

    // Seller acknowledges
    s.client.acknowledge_topup(&seller, &escrow_id);

    assert!(s.client.get_escrow(&escrow_id).top_up_acknowledged);
}

#[test]
fn test_admin_update_topup_multiplier() {
    let s = setup();
    // Check default
    assert_eq!(s.client.get_max_topup_multiplier(), 3);

    // Update to 5
    s.client.update_max_topup_multiplier(&s.admin, &5);
    assert_eq!(s.client.get_max_topup_multiplier(), 5);
}

#[test]
fn test_partial_release_request() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Request partial release
    let justification_hash = BytesN::from_array(&s.env, &[1u8; 32]);
    s.client.request_partial_release(&seller, &escrow_id, &100, &justification_hash);
}

#[test]
fn test_partial_release_approve() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Request partial release
    let justification_hash = BytesN::from_array(&s.env, &[1u8; 32]);
    s.client.request_partial_release(&seller, &escrow_id, &100, &justification_hash);

    // Approve
    s.client.approve_partial_release(&buyer, &escrow_id, &1);

    // Check escrow amount
    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.amount, 150);

    // Check seller balance
    assert_eq!(s.token_client.balance(&seller), 100);
}

#[test]
fn test_partial_release_reject() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Request partial release
    let justification_hash = BytesN::from_array(&s.env, &[1u8; 32]);
    s.client.request_partial_release(&seller, &escrow_id, &100, &justification_hash);

    // Reject
    let reason_hash = BytesN::from_array(&s.env, &[2u8; 32]);
    s.client.reject_partial_release(&buyer, &escrow_id, &1, &reason_hash);
}

#[test]
fn test_duplicate_partial_release() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // First request
    let justification_hash = BytesN::from_array(&s.env, &[1u8; 32]);
    s.client.request_partial_release(&seller, &escrow_id, &100, &justification_hash);

    // Second request (should panic)
    let __typed_err_result = s.client.try_request_partial_release(&seller, &escrow_id, &100, &justification_hash);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt2::RequestAlreadyPending.into());
}

#[test]
fn test_partial_release_escalate() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Request partial release
    let justification_hash = BytesN::from_array(&s.env, &[1u8; 32]);
    s.client.request_partial_release(&seller, &escrow_id, &100, &justification_hash);

    // Advance time past deadline (default is 86400 sec, so add 86401)
    s.env.ledger().set_timestamp(s.env.ledger().timestamp() + 86401);

    // Escalate to dispute
    s.client.escalate_partial_release_dispute(&seller, &escrow_id);

    // Check escrow status
    let escrow = s.client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Disputed);
}

#[test]
fn test_partial_release_non_active() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer,
        &seller,
        &arbiter,
        &250,
        &s.token_addr,
        &deadline,
        &None,
        &Vec::new(&s.env),
        &false,
        &0,
    );

    // Release escrow first
    s.client.release_escrow(&buyer, &escrow_id);

    // Try to request partial release (should panic)
    let justification_hash = BytesN::from_array(&s.env, &[1u8; 32]);
    let __typed_err_result = s.client.try_request_partial_release(&seller, &escrow_id, &100, &justification_hash);
    assert_eq!(__typed_err_result.unwrap_err().unwrap(), EscrowErrorExt2::PartialReleaseOnlyAllowedActiveEscrow.into());
}

// ===========================================================================
//  Fee Withdrawal Tests
// ===========================================================================

#[test]
fn test_withdraw_fees_happy_path() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    let fee_recipient = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    // 200 bps = 2% protocol fee
    s.client.update_protocol_fee(&s.admin, &200, &fee_recipient);

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &500, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    // Dispute and resolve to generate fees
    s.client.dispute_escrow(&buyer, &escrow_id, &String::from_str(&s.env, "d"), &500);
    s.client.resolve_dispute(&arbiter, &escrow_id, &0u32);

    // fee = 500 * 200 / 10000 = 10 accrued
    let destination = Address::generate(&s.env);
    assert_eq!(s.token_client.balance(&s.client.address), 10);

    s.client.withdraw_fees(&s.admin, &10, &s.token_addr, &destination);

    assert_eq!(s.token_client.balance(&destination), 10);
    assert_eq!(s.token_client.balance(&s.client.address), 0);
}

#[test]
fn test_withdraw_fees_zero_amount_rejected() {
    let s = setup();

    let result = s.client.try_withdraw_fees(
        &s.admin, &0, &s.token_addr, &Address::generate(&s.env),
    );
    assert!(result.is_err());
}

#[test]
fn test_withdraw_fees_exceeds_accrued_rejected() {
    let s = setup();
    let buyer = Address::generate(&s.env);
    let seller = Address::generate(&s.env);
    let arbiter = Address::generate(&s.env);
    s.token_admin_client.mint(&buyer, &1000);

    s.client.update_protocol_fee(&s.admin, &100, &Address::generate(&s.env));

    let deadline = s.env.ledger().timestamp() + 1000;
    let escrow_id = s.client.create_escrow(
        &buyer, &seller, &arbiter, &100, &s.token_addr, &deadline,
        &None, &Vec::new(&s.env), &false, &0u32,
    );

    s.client.dispute_escrow(&seller, &escrow_id, &String::from_str(&s.env, "d"), &100);
    s.client.resolve_dispute(&arbiter, &escrow_id, &100u32);

    // fee = 100 * 100 / 10000 = 1; try withdrawing 2
    let result = s.client.try_withdraw_fees(
        &s.admin, &2, &s.token_addr, &Address::generate(&s.env),
    );
    assert!(result.is_err());
}

#[test]
fn test_withdraw_fees_non_admin_rejected() {
    let s = setup();
    let non_admin = Address::generate(&s.env);

    let result = s.client.try_withdraw_fees(
        &non_admin, &100, &s.token_addr, &Address::generate(&s.env),
    );
    assert!(result.is_err());
}

#[test]
fn test_withdraw_fees_zero_balance_on_fresh_contract_rejected() {
    let s = setup();

    let result = s.client.try_withdraw_fees(
        &s.admin, &1, &s.token_addr, &Address::generate(&s.env),
    );
    assert!(result.is_err());
}
