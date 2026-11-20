//! #1163: settlement batch records and the merchant reconciliation view.
//!
//! Every successful `settle_merchant_payments` call writes one
//! `PaymentSettlementBatch` so merchants can match on-chain payouts against
//! their books: which payments were included, the gross amount, the protocol
//! fee taken, the net amount and the ledger it settled on.

use crate::{
    events, AhjoorPaymentsContract, AhjoorPaymentsContractArgs, AhjoorPaymentsContractClient,
    DataKey4, ExtError2, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
    PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{contractimpl, contracttype, panic_with_error, Address, Env, Vec};

/// Maximum `limit` accepted by `get_settlement_batches`.
pub const MAX_SETTLEMENT_BATCH_PAGE_SIZE: u32 = 50;

/// Record of one `settle_merchant_payments` call.
///
/// Named `PaymentSettlementBatch` (rather than `SettlementBatch`) because the
/// multi-token invoice module already exports a `SettlementBatch` type.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentSettlementBatch {
    /// Batch ID (starts at 1).
    pub id: u32,
    pub merchant: Address,
    /// Token every payment in the batch was settled in.
    pub token: Address,
    /// Payment IDs settled in this batch, in the order given.
    pub payment_ids: Vec<u32>,
    /// Sum of the settled payment amounts.
    pub gross: i128,
    /// Protocol fee deducted from `gross`.
    pub fees: i128,
    /// `gross - fees`: the amount credited to the merchant.
    pub net: i128,
    /// Portion of `net` held back in the merchant's rolling reserve (#1051);
    /// the amount transferred at settlement is `net - reserve_held`.
    pub reserve_held: i128,
    /// Ledger sequence the batch settled on.
    pub ledger: u32,
}

impl AhjoorPaymentsContract {
    /// Persists a settlement batch and indexes it under the merchant.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record_settlement_batch(
        env: &Env,
        merchant: &Address,
        token: &Address,
        payment_ids: &Vec<u32>,
        gross: i128,
        fees: i128,
        net: i128,
        reserve_held: i128,
    ) -> u32 {
        let id: u32 = env
            .storage()
            .instance()
            .get(&DataKey4::SettlementBatchCounter)
            .unwrap_or(0u32)
            + 1;
        env.storage()
            .instance()
            .set(&DataKey4::SettlementBatchCounter, &id);

        let batch = PaymentSettlementBatch {
            id,
            merchant: merchant.clone(),
            token: token.clone(),
            payment_ids: payment_ids.clone(),
            gross,
            fees,
            net,
            reserve_held,
            ledger: env.ledger().sequence(),
        };
        let batch_key = DataKey4::SettlementBatch(id);
        env.storage().persistent().set(&batch_key, &batch);
        env.storage().persistent().extend_ttl(
            &batch_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        let index_key = DataKey4::MerchantSettlementBatches(merchant.clone());
        let mut index: Vec<u32> = env
            .storage()
            .persistent()
            .get(&index_key)
            .unwrap_or(Vec::new(env));
        index.push_back(id);
        env.storage().persistent().set(&index_key, &index);
        env.storage().persistent().extend_ttl(
            &index_key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_settlement_batch_recorded(
            env,
            id,
            merchant.clone(),
            payment_ids.len(),
            gross,
            fees,
            net,
        );

        id
    }
}

#[contractimpl]
impl AhjoorPaymentsContract {
    /// Paginated reconciliation view of a merchant's settlement batches,
    /// oldest first. `start` indexes into the merchant's batch list; a
    /// `start` past the end returns an empty page. `limit` must be between 1
    /// and `MAX_SETTLEMENT_BATCH_PAGE_SIZE`.
    pub fn get_settlement_batches(
        env: Env,
        merchant: Address,
        start: u32,
        limit: u32,
    ) -> Vec<PaymentSettlementBatch> {
        if limit == 0 || limit > MAX_SETTLEMENT_BATCH_PAGE_SIZE {
            panic_with_error!(&env, ExtError2::InvalidPageLimit);
        }

        let index: Vec<u32> = env
            .storage()
            .persistent()
            .get(&DataKey4::MerchantSettlementBatches(merchant))
            .unwrap_or(Vec::new(&env));

        let end = start.saturating_add(limit).min(index.len());
        let mut page = Vec::new(&env);
        for i in start..end {
            let id = index.get(i).unwrap();
            if let Some(batch) = env
                .storage()
                .persistent()
                .get::<_, PaymentSettlementBatch>(&DataKey4::SettlementBatch(id))
            {
                page.push_back(batch);
            }
        }

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        page
    }

    /// Number of settlement batches recorded for a merchant.
    pub fn get_settlement_batch_count(env: Env, merchant: Address) -> u32 {
        env.storage()
            .persistent()
            .get::<_, Vec<u32>>(&DataKey4::MerchantSettlementBatches(merchant))
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Returns a single settlement batch by ID.
    pub fn get_settlement_batch(env: Env, id: u32) -> PaymentSettlementBatch {
        env.storage()
            .persistent()
            .get(&DataKey4::SettlementBatch(id))
            .unwrap_or_else(|| panic_with_error!(&env, ExtError2::SettlementBatchNotFound))
    }
}
