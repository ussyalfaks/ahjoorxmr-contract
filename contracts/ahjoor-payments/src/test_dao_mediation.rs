#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{testutils::{Address as _, Ledger}, vec, Address, Env, String};

fn setup_with_payment() -> (
    Env,
    AhjoorPaymentsContractClient<'static>,
    Address,
    Address,
    Address,
    TokenClient<'static>,
    u32,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);

    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_addr);
    let token_admin = TokenAdminClient::new(&env, &token_addr);
    token_admin.mint(&customer, &1_000_000);

    client.initialize(&admin, &fee_recipient, &0);

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &500_000,
        &token_addr,
        &None,
        &None,
        &None,
    );
    client.dispute_payment(
        &customer,
        &payment_id,
        &String::from_str(&env, "Goods not received"),
    );

    (env, client, admin, customer, merchant, token_client, payment_id)
}

#[test]
fn test_configure_dao() {
    let (env, client, admin, _customer, _merchant, _token, _payment_id) =
        setup_with_payment();

    let member_a = Address::generate(&env);
    let member_b = Address::generate(&env);
    let member_c = Address::generate(&env);
    let members = vec![&env, member_a, member_b, member_c];

    client.configure_dao(&members, &86_400u64, &2u32);

    let dao_members = client.get_dao_members();
    assert_eq!(dao_members.len(), 3);
}

#[test]
fn test_escalate_to_dao() {
    let (env, client, admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);

    let case_id = client.escalate_to_dao(&customer, &payment_id);
    assert_eq!(case_id, 0);

    let case = client.get_dao_mediation_case(&case_id);
    assert_eq!(case.payment_id, payment_id);
    assert_eq!(case.votes_for_merchant, 0);
    assert_eq!(case.votes_for_customer, 0);
    assert!(!case.executed);
}

#[test]
fn test_cancel_dao_escalation_before_votes() {
    let (env, client, _admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);

    let case_id_0 = client.escalate_to_dao(&customer, &payment_id);
    assert_eq!(case_id_0, 0);

    client.cancel_dao_escalation(&payment_id);

    // Cancellation removes the active case, allowing a fresh escalation.
    let case_id_1 = client.escalate_to_dao(&customer, &payment_id);
    assert_eq!(case_id_1, 1);
}

#[test]
#[should_panic]
fn test_cancel_dao_escalation_after_vote_panics() {
    let (env, client, _admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);

    let case_id = client.escalate_to_dao(&customer, &payment_id);
    client.dao_vote(&mediator, &case_id, &false);

    // Any vote activity blocks cancellation.
    client.cancel_dao_escalation(&payment_id);
}

#[test]
fn test_dao_vote_and_execute_customer_wins() {
    let (env, client, admin, customer, _merchant, token_client, payment_id) =
        setup_with_payment();

    let mediator_a = Address::generate(&env);
    let mediator_b = Address::generate(&env);
    client.configure_dao(
        &vec![&env, mediator_a.clone(), mediator_b.clone()],
        &1u64, // 1-second window so we can advance past it
        &1u32,
    );

    let case_id = client.escalate_to_dao(&customer, &payment_id);

    client.dao_vote(&mediator_a, &case_id, &false); // for customer
    client.dao_vote(&mediator_b, &case_id, &false); // for customer

    // Advance time past the vote window.
    env.ledger().with_mut(|li| li.timestamp += 10);

    let customer_balance_before = token_client.balance(&customer);
    client.execute_dao_verdict(&case_id);
    let customer_balance_after = token_client.balance(&customer);

    assert!(customer_balance_after > customer_balance_before);

    let case = client.get_dao_mediation_case(&case_id);
    assert!(case.executed);
    assert_eq!(case.votes_for_customer, 2);
}

#[test]
fn test_dao_vote_and_execute_merchant_wins() {
    let (env, client, admin, customer, merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &1u64, &1u32);

    let case_id = client.escalate_to_dao(&customer, &payment_id);
    client.dao_vote(&mediator, &case_id, &true); // for merchant

    env.ledger().with_mut(|li| li.timestamp += 10);
    client.execute_dao_verdict(&case_id);

    let case = client.get_dao_mediation_case(&case_id);
    assert!(case.executed);
    assert_eq!(case.votes_for_merchant, 1);
}

#[test]
#[should_panic]
fn test_dao_double_vote_rejected() {
    let (env, client, admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);

    let case_id = client.escalate_to_dao(&customer, &payment_id);
    client.dao_vote(&mediator, &case_id, &true);
    client.dao_vote(&mediator, &case_id, &false); // should panic: already voted
}

#[test]
#[should_panic]
fn test_non_dao_member_vote_rejected() {
    let (env, client, admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    let outsider = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);

    let case_id = client.escalate_to_dao(&customer, &payment_id);
    client.dao_vote(&outsider, &case_id, &true); // should panic: not a DAO member
}

#[test]
#[should_panic]
fn test_execute_before_window_closes_panics() {
    let (env, client, admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);

    let case_id = client.escalate_to_dao(&customer, &payment_id);
    client.dao_vote(&mediator, &case_id, &true);

    // Window not closed yet — should panic with DaoVoteWindowOpen.
    client.execute_dao_verdict(&case_id);
}

#[test]
#[should_panic]
fn test_escalate_non_disputed_payment_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_admin = TokenAdminClient::new(&env, &token_addr);
    token_admin.mint(&customer, &1_000_000);

    client.initialize(&admin, &fee_recipient, &0);

    let payment_id = client.create_payment(
        &customer,
        &merchant,
        &500_000,
        &token_addr,
        &None,
        &None,
        &None,
    );
    // Payment is Pending, not Disputed — escalation should panic.
    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);
    client.escalate_to_dao(&customer, &payment_id);
}

// ── Issue (b): duplicate escalation guard ────────────────────────────────────

/// A second call to `escalate_to_dao` for the same still-disputed payment must
/// revert with `DaoAlreadyEscalated`. The first call must still succeed and
/// produce exactly one case.
#[test]
#[should_panic]
fn test_double_escalation_rejected() {
    let (env, client, _admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(&vec![&env, mediator.clone()], &86_400u64, &1u32);

    // First escalation: must succeed.
    let case_id = client.escalate_to_dao(&customer, &payment_id);
    assert_eq!(case_id, 0);

    // Verify exactly one case exists.
    let case = client.get_dao_mediation_case(&case_id);
    assert_eq!(case.payment_id, payment_id);
    assert!(!case.executed);

    // Second escalation on the same still-disputed payment: must panic with
    // DaoAlreadyEscalated.
    client.escalate_to_dao(&customer, &payment_id);
}

// ── Issue (a): execute_dao_verdict payment-status guard ───────────────────────

/// Executing a second verdict for an already-resolved payment must revert.
///
/// Scenario: admin resolves the dispute out-of-band (via `resolve_dispute`)
/// while a DAO case is still open.  When `execute_dao_verdict` is then called
/// the payment is already `Refunded`, so the contract must reject the call
/// instead of transferring funds a second time.
#[test]
#[should_panic]
fn test_double_verdict_execution_via_already_resolved_payment_rejected() {
    let (env, client, admin, customer, _merchant, token_client, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(
        &vec![&env, mediator.clone()],
        &1u64, // 1-second window so we can advance past it
        &1u32,
    );

    // Escalate to DAO.
    let case_id = client.escalate_to_dao(&customer, &payment_id);

    // Cast a customer-winning vote.
    client.dao_vote(&mediator, &case_id, &false);

    // Admin resolves the dispute independently — payment is now Refunded.
    client.resolve_dispute(&payment_id, &false);

    // Advance time past the vote window.
    env.ledger().with_mut(|li| li.timestamp += 10);

    // Attempt to execute the DAO verdict against an already-Refunded payment.
    // Must panic because the payment status guard rejects the call.
    client.execute_dao_verdict(&case_id);
}

/// Directly re-executing an already-executed case must revert with
/// `DaoCaseAlreadyExecuted` (the `case.executed` flag check, unchanged from
/// before, should still hold).  Additionally, the payment status guard
/// independently blocks a second payout even if the `executed` flag were
/// somehow bypassed (covered by the scenario above).
#[test]
#[should_panic]
fn test_execute_already_executed_case_rejected() {
    let (env, client, _admin, customer, _merchant, _token, payment_id) =
        setup_with_payment();

    let mediator = Address::generate(&env);
    client.configure_dao(
        &vec![&env, mediator.clone()],
        &1u64,
        &1u32,
    );

    let case_id = client.escalate_to_dao(&customer, &payment_id);
    client.dao_vote(&mediator, &case_id, &false); // customer wins

    env.ledger().with_mut(|li| li.timestamp += 10);

    // First execution: succeeds.
    client.execute_dao_verdict(&case_id);
    let case = client.get_dao_mediation_case(&case_id);
    assert!(case.executed);

    // Second execution on the same case_id: must panic with DaoCaseAlreadyExecuted.
    client.execute_dao_verdict(&case_id);
}
