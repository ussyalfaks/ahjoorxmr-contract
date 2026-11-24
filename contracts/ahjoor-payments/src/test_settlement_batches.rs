#![cfg(test)]
//! #1163: settlement batch records (`settle_merchant_payments`) and the
//! reconciliation view (`get_settlement_batches`, `get_settlement_batch`).
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env, String,
};

const FEE_BPS: u32 = 100; // 1%

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    fee_recipient: Address,
    token: Address,
    token_client: TokenClient<'a>,
    customer: Address,
    merchant: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token);
    let token_admin = TokenAdminClient::new(&env, &token);

    client.initialize(&admin, &fee_recipient, &FEE_BPS);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    token_admin.mint(&customer, &1_000_000);

    Setup {
        env,
        client,
        admin,
        fee_recipient,
        token,
        token_client,
        customer,
        merchant,
    }
}

impl<'a> Setup<'a> {
    /// A payment released to `merchant` through dispute resolution is
    /// completed but unsettled, so it is eligible for batch settlement.
    fn settleable_payment_for(&self, merchant: &Address, amount: i128) -> u32 {
        let id = self.client.create_payment(
            &self.customer,
            merchant,
            &amount,
            &self.token,
            &None,
            &None,
            &None,
        );
        self.client
            .dispute_payment(&self.customer, &id, &String::from_str(&self.env, "late"));
        self.client.resolve_dispute(&id, &true);
        id
    }

    fn settleable_payment(&self, amount: i128) -> u32 {
        self.settleable_payment_for(&self.merchant, amount)
    }

    fn settle(&self, ids: &Vec<u32>) -> u32 {
        self.client
            .settle_merchant_payments(&self.admin, &self.merchant, ids)
    }
}

// ===========================================================================
//  Recording
// ===========================================================================

#[test]
fn test_settlement_creates_batch_record() {
    let s = setup();
    s.env.ledger().with_mut(|l| l.sequence_number = 4_242);
    let p1 = s.settleable_payment(1_000);
    let p2 = s.settleable_payment(2_000);

    let batch_id = s.settle(&vec![&s.env, p1, p2]);
    assert_eq!(batch_id, 1);

    let batch = s.client.get_settlement_batch(&batch_id);
    assert_eq!(
        batch,
        PaymentSettlementBatch {
            id: 1,
            merchant: s.merchant.clone(),
            token: s.token.clone(),
            payment_ids: vec![&s.env, p1, p2],
            gross: 3_000,
            fees: 30,
            net: 2_970,
            reserve_held: 0,
            ledger: 4_242,
        }
    );

    // The record matches the funds that actually moved.
    assert_eq!(s.token_client.balance(&s.merchant), batch.net);
    assert_eq!(s.token_client.balance(&s.fee_recipient), batch.fees);
}

#[test]
fn test_batch_ids_increment_and_are_indexed_per_merchant() {
    let s = setup();
    let other = Address::generate(&s.env);

    let a = s.settleable_payment(100);
    let first = s.settle(&vec![&s.env, a]);

    let b = s.settleable_payment_for(&other, 200);
    let other_batch = s
        .client
        .settle_merchant_payments(&s.admin, &other, &vec![&s.env, b]);

    let c = s.settleable_payment(300);
    let third = s.settle(&vec![&s.env, c]);

    assert_eq!((first, other_batch, third), (1, 2, 3));
    assert_eq!(s.client.get_settlement_batch_count(&s.merchant), 2);
    assert_eq!(s.client.get_settlement_batch_count(&other), 1);

    let mine = s.client.get_settlement_batches(&s.merchant, &0, &10);
    assert_eq!(mine.len(), 2);
    assert_eq!(mine.get(0).unwrap().id, 1);
    assert_eq!(mine.get(1).unwrap().id, 3);
    assert_eq!(mine.get(1).unwrap().gross, 300);

    let theirs = s.client.get_settlement_batches(&other, &0, &10);
    assert_eq!(theirs.len(), 1);
    assert_eq!(theirs.get(0).unwrap().merchant, other);
}

#[test]
fn test_batch_records_rolling_reserve_hold() {
    let s = setup();
    s.client.set_rolling_reserve(&s.merchant, &1_000, &100); // 10%
    let id = s.settleable_payment(1_000);

    let batch = s.client.get_settlement_batch(&s.settle(&vec![&s.env, id]));
    assert_eq!(batch.gross, 1_000);
    assert_eq!(batch.fees, 10);
    assert_eq!(batch.net, 990);
    assert_eq!(batch.reserve_held, 99);
    assert_eq!(s.token_client.balance(&s.merchant), batch.net - batch.reserve_held);
}

#[test]
fn test_failed_settlement_records_no_batch() {
    let s = setup();
    let settled = s.settleable_payment(100);
    s.settle(&vec![&s.env, settled]);

    // Re-settling the same payment fails and must not record a batch.
    let res = s
        .client
        .try_settle_merchant_payments(&s.admin, &s.merchant, &vec![&s.env, settled]);
    assert!(res.is_err());
    assert_eq!(s.client.get_settlement_batch_count(&s.merchant), 1);
    assert_eq!(
        s.client.try_get_settlement_batch(&2).unwrap_err().unwrap(),
        ExtError2::SettlementBatchNotFound.into()
    );
}

// ===========================================================================
//  Pagination
// ===========================================================================

#[test]
fn test_get_settlement_batches_paginates_oldest_first() {
    let s = setup();
    for amount in [100i128, 200, 300, 400, 500] {
        let id = s.settleable_payment(amount);
        s.settle(&vec![&s.env, id]);
    }

    let page1 = s.client.get_settlement_batches(&s.merchant, &0, &2);
    let page2 = s.client.get_settlement_batches(&s.merchant, &2, &2);
    let page3 = s.client.get_settlement_batches(&s.merchant, &4, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page1.get(0).unwrap().gross, 100);
    assert_eq!(page1.get(1).unwrap().gross, 200);
    assert_eq!(page2.get(0).unwrap().gross, 300);
    assert_eq!(page3.get(0).unwrap().gross, 500);
}

#[test]
fn test_get_settlement_batches_start_past_end_is_empty() {
    let s = setup();
    let id = s.settleable_payment(100);
    s.settle(&vec![&s.env, id]);

    assert_eq!(s.client.get_settlement_batches(&s.merchant, &1, &10).len(), 0);
    assert_eq!(s.client.get_settlement_batches(&s.merchant, &u32::MAX, &50).len(), 0);
}

#[test]
fn test_get_settlement_batches_unknown_merchant_is_empty() {
    let s = setup();
    let nobody = Address::generate(&s.env);
    assert_eq!(s.client.get_settlement_batches(&nobody, &0, &10).len(), 0);
    assert_eq!(s.client.get_settlement_batch_count(&nobody), 0);
}

#[test]
fn test_get_settlement_batches_accepts_max_page_size() {
    let s = setup();
    let id = s.settleable_payment(100);
    s.settle(&vec![&s.env, id]);
    let page = s
        .client
        .get_settlement_batches(&s.merchant, &0, &MAX_SETTLEMENT_BATCH_PAGE_SIZE);
    assert_eq!(page.len(), 1);
}

// ===========================================================================
//  Error branches
// ===========================================================================

#[test]
fn test_get_settlement_batches_rejects_zero_limit() {
    let s = setup();
    let err = s
        .client
        .try_get_settlement_batches(&s.merchant, &0, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::InvalidPageLimit.into());
}

#[test]
fn test_get_settlement_batches_rejects_limit_above_max() {
    let s = setup();
    let err = s
        .client
        .try_get_settlement_batches(&s.merchant, &0, &(MAX_SETTLEMENT_BATCH_PAGE_SIZE + 1))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::InvalidPageLimit.into());
}

#[test]
fn test_get_settlement_batch_not_found() {
    let s = setup();
    let err = s.client.try_get_settlement_batch(&1).unwrap_err().unwrap();
    assert_eq!(err, ExtError2::SettlementBatchNotFound.into());
}
