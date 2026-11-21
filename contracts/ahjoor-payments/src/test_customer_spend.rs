#![cfg(test)]
//! #1166: customer spending breakdown by category and period.
use super::*;
use crate::customer_spend::{
    CategorySpend, SpendError, MAX_SPEND_QUERY_LEDGERS, SPEND_BUCKET_LEDGERS,
};
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, Symbol,
};

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token: Address,
    customer: Address,
    merchant: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.initialize(&admin, &Address::generate(&env), &0);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    TokenAdminClient::new(&env, &token).mint(&customer, &100_000);
    env.ledger().set_sequence_number(SPEND_BUCKET_LEDGERS * 100);
    Setup {
        env,
        client,
        admin,
        token,
        customer,
        merchant,
    }
}

impl<'a> Setup<'a> {
    fn sym(&self, s: &str) -> Symbol {
        Symbol::new(&self.env, s)
    }

    fn pay(&self, category: &str, amount: i128) -> u32 {
        let id = self.client.create_payment_with_category(
            &self.customer,
            &self.merchant,
            &amount,
            &self.token,
            &self.sym(category),
            &None,
            &None,
        );
        self.client.complete_payment(&id);
        id
    }

    fn now(&self) -> u32 {
        self.env.ledger().sequence()
    }

    fn breakdown(&self) -> Vec<CategorySpend> {
        let now = self.now();
        self.client.get_customer_spend_by_category(
            &self.customer,
            &(now - SPEND_BUCKET_LEDGERS),
            &now,
        )
    }

    fn row(&self, rows: &Vec<CategorySpend>, category: &str) -> Option<CategorySpend> {
        let cat = self.sym(category);
        rows.iter().find(|r| r.category == cat)
    }
}

#[test]
fn test_breakdown_groups_completed_payments_by_category() {
    let s = setup();
    s.pay("food", 300);
    s.pay("food", 200);
    s.pay("travel", 150);

    let rows = s.breakdown();
    assert_eq!(rows.len(), 2);
    let food = s.row(&rows, "food").unwrap();
    assert_eq!((food.spent, food.refunded, food.net), (500, 0, 500));
    let travel = s.row(&rows, "travel").unwrap();
    assert_eq!((travel.spent, travel.refunded, travel.net), (150, 0, 150));
}

#[test]
fn test_category_is_stored_and_indexed() {
    let s = setup();
    let id = s.pay("food", 300);
    assert_eq!(s.client.get_payment(&id).category, Some(s.sym("food")));
    let ids = s
        .client
        .get_payments_by_category(&s.merchant, &s.sym("food"), &0, &10);
    assert_eq!(ids, soroban_sdk::vec![&s.env, id]);
}

#[test]
fn test_uncategorized_payments_are_reported() {
    let s = setup();
    let id = s.client.create_payment(
        &s.customer,
        &s.merchant,
        &400,
        &s.token,
        &None,
        &None,
        &None,
    );
    s.client.complete_payment(&id);

    let rows = s.breakdown();
    let row = s.row(&rows, "uncategorized").unwrap();
    assert_eq!(row.spent, 400);
}

#[test]
fn test_pending_payments_are_not_counted() {
    let s = setup();
    s.client.create_payment_with_category(
        &s.customer,
        &s.merchant,
        &300,
        &s.token,
        &s.sym("food"),
        &None,
        &None,
    );
    assert_eq!(s.breakdown().len(), 0);
}

#[test]
fn test_refund_of_settled_payment_is_aggregated() {
    let s = setup();
    let id = s.pay("food", 300);
    s.client.refund_settled_payment(&s.merchant, &id, &100);

    let food = s.row(&s.breakdown(), "food").unwrap();
    assert_eq!((food.spent, food.refunded, food.net), (300, 100, 200));
}

#[test]
fn test_cooling_off_cancellation_is_aggregated_as_refund() {
    let s = setup();
    s.client.set_max_cooling_off_ledgers(&s.admin, &1000);
    let id =
        s.client
            .create_payment_with_cooling_off(&s.customer, &s.merchant, &250, &s.token, &500);
    s.client.complete_payment(&id);
    // Completion already paid the merchant; top the contract up so the
    // (pre-existing) cooling-off refund transfer can run and we can observe
    // the spend aggregate.
    TokenAdminClient::new(&s.env, &s.token).mint(&s.client.address, &250);
    s.client.cancel_during_cooling_off(&s.customer, &id);

    let row = s.row(&s.breakdown(), "uncategorized").unwrap();
    assert_eq!((row.spent, row.refunded, row.net), (250, 250, 0));
}

#[test]
fn test_period_filters_by_bucket() {
    let s = setup();
    let first_period = s.now();
    s.pay("food", 100);

    s.env
        .ledger()
        .set_sequence_number(first_period + 5 * SPEND_BUCKET_LEDGERS);
    let second_period = s.now();
    s.pay("food", 40);

    // Only the later bucket.
    let rows = s
        .client
        .get_customer_spend_by_category(&s.customer, &second_period, &second_period);
    assert_eq!(s.row(&rows, "food").unwrap().spent, 40);

    // Only the earlier bucket.
    let rows = s
        .client
        .get_customer_spend_by_category(&s.customer, &first_period, &first_period);
    assert_eq!(s.row(&rows, "food").unwrap().spent, 100);

    // Both.
    let rows = s
        .client
        .get_customer_spend_by_category(&s.customer, &first_period, &second_period);
    assert_eq!(s.row(&rows, "food").unwrap().spent, 140);

    // Neither (between the two buckets).
    let gap = first_period + 2 * SPEND_BUCKET_LEDGERS;
    let rows = s
        .client
        .get_customer_spend_by_category(&s.customer, &gap, &gap);
    assert_eq!(rows.len(), 0);
}

#[test]
fn test_breakdown_is_per_customer() {
    let s = setup();
    s.pay("food", 300);
    let other = Address::generate(&s.env);
    let now = s.now();
    let rows =
        s.client
            .get_customer_spend_by_category(&other, &0, &now.min(MAX_SPEND_QUERY_LEDGERS));
    assert_eq!(rows.len(), 0);
}

#[test]
fn test_max_range_is_allowed() {
    let s = setup();
    s.pay("food", 300);
    let now = s.now();
    let rows = s.client.get_customer_spend_by_category(
        &s.customer,
        &(now - MAX_SPEND_QUERY_LEDGERS),
        &now,
    );
    assert_eq!(s.row(&rows, "food").unwrap().spent, 300);
}

#[test]
fn test_error_inverted_range() {
    let s = setup();
    let result = s
        .client
        .try_get_customer_spend_by_category(&s.customer, &10, &9);
    assert_eq!(
        result.unwrap_err().unwrap(),
        SpendError::InvalidSpendRange.into()
    );
}

#[test]
fn test_error_range_too_large() {
    let s = setup();
    let result = s.client.try_get_customer_spend_by_category(
        &s.customer,
        &0,
        &(MAX_SPEND_QUERY_LEDGERS + 1),
    );
    assert_eq!(
        result.unwrap_err().unwrap(),
        SpendError::SpendRangeTooLarge.into()
    );
}
