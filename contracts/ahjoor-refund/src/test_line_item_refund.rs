#![cfg(test)]
//! #1180: Refunds for specific invoice line items.
use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{vec, Address, Env, String, Symbol};

use ahjoor_payments::{
    AhjoorPaymentsContract, AhjoorPaymentsContractClient, InvoiceData, LineItem,
};

struct S<'a> {
    env: Env,
    refund: AhjoorRefundContractClient<'a>,
    payments: AhjoorPaymentsContractClient<'a>,
    customer: Address,
    merchant: Address,
    token_addr: Address,
    token_admin: TokenAdminClient<'a>,
}

fn setup<'a>() -> S<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let payments = AhjoorPaymentsContractClient::new(&env, &env.register(AhjoorPaymentsContract, ()));
    let refund = AhjoorRefundContractClient::new(&env, &env.register(AhjoorRefundContract, ()));
    let admin = Address::generate(&env);
    let token_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    payments.initialize(&admin, &admin, &0u32);
    refund.initialize(&admin, &payments.address, &86_400u64, &None);
    let token_admin = TokenAdminClient::new(&env, &token_addr);
    let customer = Address::generate(&env);
    token_admin.mint(&customer, &10_000);
    S { customer, merchant: Address::generate(&env), env, refund, payments, token_addr, token_admin }
}

/// Item 0: 2 x 100 = 200, item 1: 1 x 300 = 300.
fn items(env: &Env) -> (Vec<LineItem>, Vec<InvoiceLineItem>) {
    (
        vec![
            env,
            LineItem { description: Symbol::new(env, "shoes"), quantity: 2, unit_price: 100 },
            LineItem { description: Symbol::new(env, "coat"), quantity: 1, unit_price: 300 },
        ],
        vec![
            env,
            InvoiceLineItem { description: Symbol::new(env, "shoes"), quantity: 2, unit_price: 100 },
            InvoiceLineItem { description: Symbol::new(env, "coat"), quantity: 1, unit_price: 300 },
        ],
    )
}

/// Creates a completed invoice payment and returns (payment_id, proof).
fn invoice_payment(s: &S, tax_bps: u32) -> (u32, InvoiceProof) {
    let (pay_items, proof_items) = items(&s.env);
    let total = 500 + 500 * tax_bps as i128 / 10_000;
    let invoice = InvoiceData {
        line_items: pay_items,
        tax_bps,
        currency_label: Symbol::new(&s.env, "USD"),
    };
    let pid = s.payments.create_payment_with_invoice(
        &s.customer, &s.merchant, &total, &s.token_addr, &None, &None, &Some(invoice), &None,
    );
    s.payments.complete_payment(&pid);
    let proof = InvoiceProof {
        line_items: proof_items,
        tax_bps,
        currency_label: Symbol::new(&s.env, "USD"),
    };
    (pid, proof)
}

fn request_items(s: &S, pid: u32, amount: i128, indices: Vec<u32>, proof: &InvoiceProof) -> u32 {
    s.refund.request_refund_for_items(
        &s.customer,
        &pid,
        &amount,
        &String::from_str(&s.env, "return"),
        &0,
        &indices,
        proof,
    )
}

// ---------- happy paths ----------

#[test]
fn test_refund_single_line_item() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    let rid = request_items(&s, pid, 200, vec![&s.env, 0u32], &proof);

    assert_eq!(s.refund.get_refund(&rid).amount, 200);
    assert_eq!(s.refund.get_refund_line_items(&rid), vec![&s.env, 0u32]);
    assert_eq!(s.refund.get_refunded_line_items(&pid), vec![&s.env, 0u32]);
}

#[test]
fn test_refund_remaining_item_in_second_request() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    request_items(&s, pid, 200, vec![&s.env, 0u32], &proof);
    let rid2 = request_items(&s, pid, 300, vec![&s.env, 1u32], &proof);

    assert_eq!(s.refund.get_refund_line_items(&rid2), vec![&s.env, 1u32]);
    assert_eq!(s.refund.get_refunded_line_items(&pid), vec![&s.env, 0u32, 1u32]);
}

#[test]
fn test_refund_multiple_items_and_partial_amount() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    let rid = request_items(&s, pid, 450, vec![&s.env, 1u32, 0u32], &proof);
    assert_eq!(s.refund.get_refund(&rid).amount, 450);
    assert_eq!(s.refund.get_refund_line_items(&rid), vec![&s.env, 1u32, 0u32]);
}

#[test]
fn test_line_item_total_includes_tax() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 1_000); // 10% tax
    let rid = request_items(&s, pid, 330, vec![&s.env, 1u32], &proof);
    assert_eq!(s.refund.get_refund(&rid).amount, 330);
}

#[test]
fn test_amount_only_refund_records_no_line_items() {
    let s = setup();
    let (pid, _) = invoice_payment(&s, 0);
    let rid = s.refund.request_refund(&s.customer, &pid, &100, &String::from_str(&s.env, "x"), &0);
    assert_eq!(s.refund.get_refund_line_items(&rid).len(), 0);
    assert_eq!(s.refund.get_refunded_line_items(&pid).len(), 0);
}

// ---------- error branches ----------

#[test]
#[should_panic(expected = "LineItemIndicesEmpty")]
fn test_empty_indices_rejected() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    request_items(&s, pid, 100, Vec::new(&s.env), &proof);
}

#[test]
#[should_panic(expected = "PaymentHasNoInvoice")]
fn test_payment_without_invoice_rejected() {
    let s = setup();
    let (_, proof) = invoice_payment(&s, 0);
    let pid = s.payments.create_payment(&s.customer, &s.merchant, &500, &s.token_addr, &None, &None, &None);
    s.payments.complete_payment(&pid);
    request_items(&s, pid, 100, vec![&s.env, 0u32], &proof);
}

#[test]
#[should_panic(expected = "InvalidLineItemProof")]
fn test_tampered_proof_rejected() {
    let s = setup();
    let (pid, mut proof) = invoice_payment(&s, 0);
    let mut item = proof.line_items.get(0).unwrap();
    item.unit_price = 1_000;
    proof.line_items.set(0, item);
    request_items(&s, pid, 100, vec![&s.env, 0u32], &proof);
}

#[test]
#[should_panic(expected = "InvalidLineItemProof")]
fn test_wrong_tax_in_proof_rejected() {
    let s = setup();
    let (pid, mut proof) = invoice_payment(&s, 0);
    proof.tax_bps = 500;
    request_items(&s, pid, 100, vec![&s.env, 0u32], &proof);
}

#[test]
#[should_panic(expected = "LineItemIndexOutOfRange")]
fn test_index_out_of_range_rejected() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    request_items(&s, pid, 100, vec![&s.env, 2u32], &proof);
}

#[test]
#[should_panic(expected = "DuplicateLineItemIndex")]
fn test_duplicate_index_in_request_rejected() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    request_items(&s, pid, 100, vec![&s.env, 0u32, 0u32], &proof);
}

#[test]
#[should_panic(expected = "LineItemAlreadyRefunded")]
fn test_same_line_item_cannot_be_refunded_twice() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    request_items(&s, pid, 200, vec![&s.env, 0u32], &proof);
    request_items(&s, pid, 100, vec![&s.env, 1u32, 0u32], &proof);
}

#[test]
#[should_panic(expected = "AmountExceedsLineItemsTotal")]
fn test_amount_above_items_total_rejected() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    request_items(&s, pid, 201, vec![&s.env, 0u32], &proof);
}

#[test]
#[should_panic(expected = "AmountExceedsLineItemsTotal")]
fn test_amount_above_items_total_with_tax_rejected() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 1_000);
    request_items(&s, pid, 331, vec![&s.env, 1u32], &proof);
}

#[test]
fn test_failed_request_does_not_claim_items() {
    let s = setup();
    let (pid, proof) = invoice_payment(&s, 0);
    let res = s.refund.try_request_refund_for_items(
        &s.customer, &pid, &201, &String::from_str(&s.env, "r"), &0, &vec![&s.env, 0u32], &proof,
    );
    assert!(res.is_err());
    assert_eq!(s.refund.get_refunded_line_items(&pid).len(), 0);
    request_items(&s, pid, 200, vec![&s.env, 0u32], &proof);
}
