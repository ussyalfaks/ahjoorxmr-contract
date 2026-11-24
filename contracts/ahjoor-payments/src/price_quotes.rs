//! #1165: price-lock quotes for oracle-priced payments.
//!
//! Flow:
//!   1. The merchant opens a checkout with `create_oracle_priced_payment`,
//!      fixing the USDC price and the token the customer will pay in. This
//!      reserves a payment ID; no funds move yet.
//!   2. The merchant locks a rate with `create_quote(payment_id, rate,
//!      expiry_ledger)`. The rate must lie within the configured slippage of
//!      the live oracle price.
//!   3. The customer pays with `pay_oracle_priced_payment`. On or before
//!      `expiry_ledger` the quoted rate is used; after it the payment is
//!      rejected with `QuoteExpired`. Without a quote the live oracle rate is
//!      used.
//!
//! Rates use the oracle's convention: the USDC value of one unit of the
//! payment token, scaled by `ORACLE_PRICE_PRECISION` (10^7). The token amount
//! charged is `amount_usdc * 10^7 / rate`.

use crate::{
    events, oracle, AhjoorPaymentsContract, AhjoorPaymentsContractArgs,
    AhjoorPaymentsContractClient, DataKey, DataKey3, DataKey4, ExtError, ExtError2, Payment,
    PaymentStatus, PriceData, SlippageConfig, DEFAULT_PAYMENT_TIMEOUT, INSTANCE_BUMP_AMOUNT,
    INSTANCE_LIFETIME_THRESHOLD, ORACLE_PRICE_PRECISION, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{contractimpl, contracttype, panic_with_error, token, Address, Env};

/// A merchant-created checkout whose token amount is set by an oracle rate
/// (or a price-lock quote) at the moment the customer pays.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OraclePricedPayment {
    /// Reserved payment ID; the `Payment` record is written under the same
    /// ID once the customer pays.
    pub payment_id: u32,
    pub merchant: Address,
    pub customer: Address,
    /// Price in USDC units.
    pub amount_usdc: i128,
    /// Token the customer pays in.
    pub payment_token: Address,
    pub created_ledger: u32,
    pub paid: bool,
    /// Rate applied at payment (0 until paid).
    pub paid_rate: i128,
    /// Token amount charged at payment (0 until paid).
    pub token_amount: i128,
    /// True when `paid_rate` came from a price-lock quote.
    pub paid_with_quote: bool,
}

/// A merchant-issued price lock for one oracle-priced payment.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceQuote {
    pub payment_id: u32,
    /// Locked rate (USDC per payment-token unit, scaled by 10^7).
    pub rate: i128,
    /// Oracle rate at the time the quote was issued.
    pub oracle_rate: i128,
    /// Last ledger (inclusive) on which the quote can be used.
    pub expiry_ledger: u32,
    pub created_ledger: u32,
}

impl AhjoorPaymentsContract {
    fn load_oracle_priced_payment(env: &Env, payment_id: u32) -> OraclePricedPayment {
        env.storage()
            .persistent()
            .get(&DataKey4::OraclePricedPayment(payment_id))
            .unwrap_or_else(|| panic_with_error!(env, ExtError2::OraclePricedPaymentNotFound))
    }

    fn save_oracle_priced_payment(env: &Env, record: &OraclePricedPayment) {
        let key = DataKey4::OraclePricedPayment(record.payment_id);
        env.storage().persistent().set(&key, record);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    /// Live, fresh, positive oracle rate for `payment_token` in USDC.
    fn fresh_oracle_rate(env: &Env, payment_token: &Address) -> i128 {
        let oracle_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::OracleAddress)
            .unwrap_or_else(|| panic_with_error!(env, ExtError2::OracleNotConfigured));
        let usdc_token: Address = env
            .storage()
            .instance()
            .get(&DataKey::UsdcToken)
            .unwrap_or_else(|| panic_with_error!(env, ExtError2::OracleNotConfigured));
        let max_age: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MaxOracleAge)
            .unwrap_or_else(|| panic_with_error!(env, ExtError2::OracleNotConfigured));

        let price: PriceData = oracle::OracleClient::new(env, &oracle_addr)
            .lastprice(payment_token, &usdc_token)
            .unwrap_or_else(|| panic_with_error!(env, ExtError2::OraclePriceUnavailable));
        if price.price <= 0 {
            panic_with_error!(env, ExtError2::OraclePriceUnavailable);
        }
        if env.ledger().timestamp().saturating_sub(price.timestamp) > max_age {
            panic_with_error!(env, ExtError2::OraclePriceStale);
        }
        price.price
    }

    /// Slippage allowed between a quote and the oracle: the merchant's own
    /// tolerance if set, otherwise the global default.
    fn quote_slippage_bps(env: &Env, merchant: &Address) -> u32 {
        if let Some(bps) = env
            .storage()
            .persistent()
            .get::<_, u32>(&DataKey3::SlippageToleranceBps(merchant.clone()))
        {
            return bps;
        }
        let cfg: SlippageConfig = Self::get_slippage_config_internal(env);
        cfg.default_bps
    }
}

#[contractimpl]
impl AhjoorPaymentsContract {
    /// Merchant opens an oracle-priced checkout for `customer`, priced at
    /// `amount_usdc` and payable in `payment_token`. Returns the reserved
    /// payment ID used by `create_quote` and `pay_oracle_priced_payment`.
    pub fn create_oracle_priced_payment(
        env: Env,
        merchant: Address,
        customer: Address,
        amount_usdc: i128,
        payment_token: Address,
    ) -> u32 {
        Self::require_not_paused(&env);
        merchant.require_auth();

        if amount_usdc <= 0 {
            panic_with_error!(&env, ExtError::InvalidAmount);
        }
        if !env.storage().instance().has(&DataKey::OracleAddress)
            || !env.storage().instance().has(&DataKey::UsdcToken)
        {
            panic_with_error!(&env, ExtError2::OracleNotConfigured);
        }
        Self::require_merchant_approved(&env, &merchant);
        Self::require_token_accepted(&env, &merchant, &payment_token);

        let payment_id = Self::next_payment_id(&env);
        let record = OraclePricedPayment {
            payment_id,
            merchant: merchant.clone(),
            customer: customer.clone(),
            amount_usdc,
            payment_token: payment_token.clone(),
            created_ledger: env.ledger().sequence(),
            paid: false,
            paid_rate: 0,
            token_amount: 0,
            paid_with_quote: false,
        };
        Self::save_oracle_priced_payment(&env, &record);

        events::emit_oracle_priced_payment_created(
            &env,
            payment_id,
            merchant,
            customer,
            amount_usdc,
            payment_token,
        );
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        payment_id
    }

    /// Merchant locks `rate` for an unpaid oracle-priced payment until
    /// `expiry_ledger` (inclusive). The rate must be within the merchant's
    /// slippage tolerance (or the global default) of the live oracle rate.
    /// Issuing a new quote replaces any existing one.
    pub fn create_quote(
        env: Env,
        merchant: Address,
        payment_id: u32,
        rate: i128,
        expiry_ledger: u32,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let record = Self::load_oracle_priced_payment(&env, payment_id);
        if record.merchant != merchant {
            panic_with_error!(&env, ExtError2::NotPaymentMerchant);
        }
        if record.paid {
            panic_with_error!(&env, ExtError2::PaymentAlreadyPaid);
        }
        if rate <= 0 {
            panic_with_error!(&env, ExtError2::InvalidQuoteRate);
        }
        let now_ledger = env.ledger().sequence();
        if expiry_ledger <= now_ledger {
            panic_with_error!(&env, ExtError2::InvalidQuoteExpiry);
        }

        let oracle_rate = Self::fresh_oracle_rate(&env, &record.payment_token);
        let deviation = if rate >= oracle_rate {
            rate - oracle_rate
        } else {
            oracle_rate - rate
        };
        let deviation_bps = deviation
            .checked_mul(10_000)
            .map(|d| d / oracle_rate)
            .unwrap_or(i128::MAX);
        let slippage_bps = Self::quote_slippage_bps(&env, &merchant);
        if deviation_bps > slippage_bps as i128 {
            panic_with_error!(&env, ExtError2::QuoteOutsideSlippage);
        }

        let quote = PriceQuote {
            payment_id,
            rate,
            oracle_rate,
            expiry_ledger,
            created_ledger: now_ledger,
        };
        let key = DataKey4::PriceQuote(payment_id);
        env.storage().persistent().set(&key, &quote);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        events::emit_price_quote_created(&env, payment_id, rate, oracle_rate, expiry_ledger);
    }

    /// Customer pays an oracle-priced payment. Uses the price-lock quote when
    /// one exists (rejecting with `QuoteExpired` once it has lapsed), and the
    /// live oracle rate otherwise. The tokens are escrowed and a `Pending`
    /// payment is recorded under the reserved ID, so it completes, refunds
    /// and disputes like any other payment. Returns the token amount charged.
    pub fn pay_oracle_priced_payment(env: Env, customer: Address, payment_id: u32) -> i128 {
        Self::require_not_paused(&env);
        customer.require_auth();

        let mut record = Self::load_oracle_priced_payment(&env, payment_id);
        if record.customer != customer {
            panic_with_error!(&env, ExtError::NotPaymentCustomer);
        }
        if record.paid {
            panic_with_error!(&env, ExtError2::PaymentAlreadyPaid);
        }
        Self::require_merchant_approved(&env, &record.merchant);
        Self::enforce_rate_limit(&env, &customer, 1);

        let quote_key = DataKey4::PriceQuote(payment_id);
        let quote: Option<PriceQuote> = env.storage().persistent().get(&quote_key);
        let (rate, used_quote) = match quote {
            Some(q) => {
                if env.ledger().sequence() > q.expiry_ledger {
                    panic_with_error!(&env, ExtError2::QuoteExpired);
                }
                (q.rate, true)
            }
            None => (Self::fresh_oracle_rate(&env, &record.payment_token), false),
        };

        let token_amount = record
            .amount_usdc
            .checked_mul(ORACLE_PRICE_PRECISION)
            .map(|v| v / rate)
            .unwrap_or(0);
        if token_amount <= 0 {
            panic_with_error!(&env, ExtError::InvalidAmount);
        }

        token::Client::new(&env, &record.payment_token).transfer(
            &customer,
            &env.current_contract_address(),
            &token_amount,
        );

        let now = env.ledger().timestamp();
        let timeout: u64 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentTimeout)
            .unwrap_or(DEFAULT_PAYMENT_TIMEOUT);
        let payment = Payment {
            id: payment_id,
            customer: customer.clone(),
            merchant: record.merchant.clone(),
            amount: token_amount,
            token: record.payment_token.clone(),
            status: PaymentStatus::Pending,
            created_at: now,
            expires_at: now + timeout,
            refunded_amount: 0,
            reference: None,
            metadata: None,
            split_recipients: None,
            execute_after: 0,
            category: None,
            tags: None,
            capture_deadline: 0,
            external_id: None,
            tipping_enabled: false,
            extension_count: 0,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Payment(payment_id), &payment);
        env.storage().persistent().extend_ttl(
            &DataKey::Payment(payment_id),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        Self::add_customer_payment(&env, &customer, payment_id);

        record.paid = true;
        record.paid_rate = rate;
        record.token_amount = token_amount;
        record.paid_with_quote = used_quote;
        Self::save_oracle_priced_payment(&env, &record);
        if used_quote {
            env.storage().persistent().remove(&quote_key);
        }

        events::emit_payment_created(
            &env,
            payment_id,
            customer,
            record.merchant.clone(),
            token_amount,
            record.payment_token.clone(),
        );
        events::emit_oracle_priced_payment_paid(&env, payment_id, rate, token_amount, used_quote);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        token_amount
    }

    /// Returns an oracle-priced payment record.
    pub fn get_oracle_priced_payment(env: Env, payment_id: u32) -> OraclePricedPayment {
        Self::load_oracle_priced_payment(&env, payment_id)
    }

    /// Returns the active price-lock quote for a payment, if any. A quote is
    /// removed once it has been used to pay.
    pub fn get_quote(env: Env, payment_id: u32) -> Option<PriceQuote> {
        env.storage()
            .persistent()
            .get(&DataKey4::PriceQuote(payment_id))
    }
}
