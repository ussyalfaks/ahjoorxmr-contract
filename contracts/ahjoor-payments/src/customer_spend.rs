//! #1166: Customer spending breakdown by category and period.
//!
//! Spend is aggregated when a payment completes and when a completed payment
//! is refunded, so queries never scan payment records. Aggregates are kept in
//! per-customer ledger buckets of [`SPEND_BUCKET_LEDGERS`] ledgers (~1 day);
//! a query sums every bucket that overlaps `[from_ledger, to_ledger]`.

use crate::{
    AhjoorPaymentsContract, AhjoorPaymentsContractArgs, AhjoorPaymentsContractClient, DataKey,
    Payment, PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{
    contracterror, contractimpl, contracttype, panic_with_error, Address, Env, Map, String, Symbol,
    Vec,
};

/// Width of one spend aggregation bucket, in ledgers (~1 day at 5s/ledger).
pub const SPEND_BUCKET_LEDGERS: u32 = 17_280;

/// Maximum span (`to_ledger - from_ledger`) a single spend query may cover
/// (~31 days), which bounds the number of buckets read per call.
pub const MAX_SPEND_QUERY_LEDGERS: u32 = 31 * SPEND_BUCKET_LEDGERS;

/// Category name used to aggregate payments created without a category.
pub const UNCATEGORIZED: &str = "uncategorized";

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SpendError {
    /// `from_ledger` is greater than `to_ledger`.
    InvalidSpendRange = 160,
    /// The requested range exceeds `MAX_SPEND_QUERY_LEDGERS`.
    SpendRangeTooLarge = 161,
}

/// Running totals for one category inside one bucket.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategoryTotals {
    pub spent: i128,
    pub refunded: i128,
}

/// One row of a customer's spending breakdown.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CategorySpend {
    pub category: Symbol,
    /// Gross amount of completed payments in the period.
    pub spent: i128,
    /// Amount refunded on completed payments in the period.
    pub refunded: i128,
    /// `spent - refunded`.
    pub net: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SpendKey {
    /// (customer, bucket) -> Map<category, CategoryTotals>
    Bucket(Address, u32),
}

fn category_or_default(env: &Env, category: &Option<Symbol>) -> Symbol {
    match category {
        Some(c) => c.clone(),
        None => Symbol::new(env, UNCATEGORIZED),
    }
}

fn record(env: &Env, customer: &Address, category: &Option<Symbol>, spent: i128, refunded: i128) {
    if spent <= 0 && refunded <= 0 {
        return;
    }
    let bucket = env.ledger().sequence() / SPEND_BUCKET_LEDGERS;
    let key = SpendKey::Bucket(customer.clone(), bucket);
    let mut totals: Map<Symbol, CategoryTotals> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Map::new(env));
    let cat = category_or_default(env, category);
    let mut entry = totals.get(cat.clone()).unwrap_or(CategoryTotals {
        spent: 0,
        refunded: 0,
    });
    entry.spent = entry.spent.saturating_add(spent.max(0));
    entry.refunded = entry.refunded.saturating_add(refunded.max(0));
    totals.set(cat, entry);
    env.storage().persistent().set(&key, &totals);
    env.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

/// Adds a completed payment's gross amount to the customer's current bucket.
pub(crate) fn record_customer_spend(
    env: &Env,
    customer: &Address,
    category: &Option<Symbol>,
    amount: i128,
) {
    record(env, customer, category, amount, 0);
}

/// Adds a refund on a completed payment to the customer's current bucket.
pub(crate) fn record_customer_refund(
    env: &Env,
    customer: &Address,
    category: &Option<Symbol>,
    amount: i128,
) {
    record(env, customer, category, 0, amount);
}

#[contractimpl]
impl AhjoorPaymentsContract {
    /// #1166: Create a payment tagged with a spending `category`.
    ///
    /// Identical to `create_payment` except that the payment's `category` is
    /// set and the payment is indexed for `get_payments_by_category`.
    #[allow(clippy::too_many_arguments)]
    pub fn create_payment_with_category(
        env: Env,
        customer: Address,
        merchant: Address,
        amount: i128,
        token: Address,
        category: Symbol,
        reference: Option<String>,
        metadata: Option<Map<String, String>>,
    ) -> u32 {
        let payment_id = Self::create_payment_with_expiry(
            env.clone(),
            customer,
            merchant.clone(),
            amount,
            token,
            reference,
            metadata,
            None,
            None,
            None,
            None,
        );

        let mut payment: Payment = env
            .storage()
            .persistent()
            .get(&DataKey::Payment(payment_id))
            .expect("Payment not found");
        payment.category = Some(category.clone());
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);

        let index_key = DataKey::CategoryPayments(merchant, category);
        let mut ids: Vec<u32> = env
            .storage()
            .persistent()
            .get(&index_key)
            .unwrap_or(Vec::new(&env));
        ids.push_back(payment_id);
        env.storage().persistent().set(&index_key, &ids);
        env.storage().persistent().extend_ttl(
            &index_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        payment_id
    }

    /// #1166: A customer's spending per category between two ledgers.
    ///
    /// Sums the pre-aggregated buckets overlapping `[from_ledger, to_ledger]`
    /// (bucket width `SPEND_BUCKET_LEDGERS`). Payments without a category are
    /// reported under `uncategorized`. Rows are ordered by category.
    ///
    /// Errors: `InvalidSpendRange` if `from_ledger > to_ledger`,
    /// `SpendRangeTooLarge` if the span exceeds `MAX_SPEND_QUERY_LEDGERS`.
    pub fn get_customer_spend_by_category(
        env: Env,
        customer: Address,
        from_ledger: u32,
        to_ledger: u32,
    ) -> Vec<CategorySpend> {
        if from_ledger > to_ledger {
            panic_with_error!(&env, SpendError::InvalidSpendRange);
        }
        if to_ledger - from_ledger > MAX_SPEND_QUERY_LEDGERS {
            panic_with_error!(&env, SpendError::SpendRangeTooLarge);
        }

        let first = from_ledger / SPEND_BUCKET_LEDGERS;
        let last = to_ledger / SPEND_BUCKET_LEDGERS;
        let mut sums: Map<Symbol, CategoryTotals> = Map::new(&env);
        for bucket in first..=last {
            let totals: Option<Map<Symbol, CategoryTotals>> = env
                .storage()
                .persistent()
                .get(&SpendKey::Bucket(customer.clone(), bucket));
            if let Some(totals) = totals {
                for (cat, t) in totals.iter() {
                    let mut acc = sums.get(cat.clone()).unwrap_or(CategoryTotals {
                        spent: 0,
                        refunded: 0,
                    });
                    acc.spent = acc.spent.saturating_add(t.spent);
                    acc.refunded = acc.refunded.saturating_add(t.refunded);
                    sums.set(cat, acc);
                }
            }
        }

        let mut out: Vec<CategorySpend> = Vec::new(&env);
        for (category, t) in sums.iter() {
            out.push_back(CategorySpend {
                category,
                spent: t.spent,
                refunded: t.refunded,
                net: t.spent.saturating_sub(t.refunded),
            });
        }
        out
    }
}
