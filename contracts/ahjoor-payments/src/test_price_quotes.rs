#![cfg(test)]
//! #1165: price-lock quotes for oracle-priced payments
//! (`create_oracle_priced_payment`, `create_quote`, `pay_oracle_priced_payment`).
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

/// Oracle whose price can be changed mid-test. A stored price of 0 makes
/// `lastprice` return `None`.
mod settable_oracle {
    use crate::PriceData;
    use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env};

    #[contract]
    pub struct SettableOracle;

    #[contractimpl]
    impl SettableOracle {
        pub fn set_price(env: Env, price: i128, timestamp: u64) {
            env.storage()
                .instance()
                .set(&symbol_short!("price"), &(price, timestamp));
        }

        pub fn lastprice(env: Env, _base: Address, _quote: Address) -> Option<PriceData> {
            let (price, timestamp): (i128, u64) =
                env.storage().instance().get(&symbol_short!("price"))?;
            if price == 0 {
                return None;
            }
            Some(PriceData { price, timestamp })
        }
    }
}
use settable_oracle::{SettableOracle, SettableOracleClient};

const NOW: u64 = 100_000;
const START_LEDGER: u32 = 1_000;
const MAX_ORACLE_AGE: u64 = 3_600;
/// 1 payment token = 2 USDC (scaled by 10^7).
const ORACLE_RATE: i128 = 20_000_000;
const AMOUNT_USDC: i128 = 1_000_000;

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    oracle: SettableOracleClient<'a>,
    usdc: Address,
    pay_token: Address,
    pay_token_client: TokenClient<'a>,
    customer: Address,
    merchant: Address,
}

fn setup_unconfigured<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| {
        l.timestamp = NOW;
        l.sequence_number = START_LEDGER;
    });

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin, &0u32);

    let usdc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let pay_token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let pay_token_client = TokenClient::new(&env, &pay_token);

    let oracle_id = env.register(SettableOracle, ());
    let oracle = SettableOracleClient::new(&env, &oracle_id);
    oracle.set_price(&ORACLE_RATE, &NOW);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    TokenAdminClient::new(&env, &pay_token).mint(&customer, &10_000_000);

    Setup {
        env,
        client,
        admin,
        oracle,
        usdc,
        pay_token,
        pay_token_client,
        customer,
        merchant,
    }
}

fn setup<'a>() -> Setup<'a> {
    let s = setup_unconfigured();
    s.client
        .set_oracle(&s.oracle.address, &s.usdc, &MAX_ORACLE_AGE);
    s
}

impl<'a> Setup<'a> {
    fn checkout(&self) -> u32 {
        self.client.create_oracle_priced_payment(
            &self.merchant,
            &self.customer,
            &AMOUNT_USDC,
            &self.pay_token,
        )
    }

    fn set_ledger(&self, seq: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number = seq);
    }

    fn token_amount_at(rate: i128) -> i128 {
        AMOUNT_USDC * ORACLE_PRICE_PRECISION / rate
    }
}

// ===========================================================================
//  Happy paths
// ===========================================================================

#[test]
fn test_create_oracle_priced_payment_reserves_payment_id() {
    let s = setup();
    let id = s.checkout();
    assert_eq!(id, 0);
    assert_eq!(s.client.get_payment_counter(), 1);

    let record = s.client.get_oracle_priced_payment(&id);
    assert_eq!(record.merchant, s.merchant);
    assert_eq!(record.customer, s.customer);
    assert_eq!(record.amount_usdc, AMOUNT_USDC);
    assert_eq!(record.payment_token, s.pay_token);
    assert_eq!(record.created_ledger, START_LEDGER);
    assert!(!record.paid);

    // No funds move and no Payment exists until the customer pays.
    assert!(s.client.try_get_payment(&id).is_err());
    assert_eq!(s.pay_token_client.balance(&s.client.address), 0);
}

#[test]
fn test_create_quote_stores_quote() {
    let s = setup();
    let id = s.checkout();
    // +0.5% from the oracle: exactly at the 50 bps default slippage.
    let rate = ORACLE_RATE + ORACLE_RATE * 50 / 10_000;
    s.client.create_quote(&s.merchant, &id, &rate, &(START_LEDGER + 100));

    assert_eq!(
        s.client.get_quote(&id),
        Some(PriceQuote {
            payment_id: id,
            rate,
            oracle_rate: ORACLE_RATE,
            expiry_ledger: START_LEDGER + 100,
            created_ledger: START_LEDGER,
        })
    );
}

#[test]
fn test_payment_before_expiry_uses_quoted_rate() {
    let s = setup();
    let id = s.checkout();
    let quoted = ORACLE_RATE - ORACLE_RATE * 40 / 10_000; // -0.4%
    s.client.create_quote(&s.merchant, &id, &quoted, &(START_LEDGER + 100));

    // The oracle moves far away after the quote is issued.
    s.oracle.set_price(&(ORACLE_RATE * 3), &NOW);
    s.set_ledger(START_LEDGER + 50);

    let before = s.pay_token_client.balance(&s.customer);
    let charged = s.client.pay_oracle_priced_payment(&s.customer, &id);
    let expected = Setup::token_amount_at(quoted);
    assert_eq!(charged, expected);
    assert_eq!(s.pay_token_client.balance(&s.customer), before - expected);
    assert_eq!(s.pay_token_client.balance(&s.client.address), expected);

    let payment = s.client.get_payment(&id);
    assert_eq!(payment.status, PaymentStatus::Pending);
    assert_eq!(payment.amount, expected);
    assert_eq!(payment.token, s.pay_token);
    assert_eq!(payment.customer, s.customer);
    assert_eq!(payment.merchant, s.merchant);

    let record = s.client.get_oracle_priced_payment(&id);
    assert!(record.paid);
    assert!(record.paid_with_quote);
    assert_eq!(record.paid_rate, quoted);
    assert_eq!(record.token_amount, expected);
    // The quote is consumed.
    assert_eq!(s.client.get_quote(&id), None);
    assert!(s.client.get_customer_payments(&s.customer).contains(&id));
}

#[test]
fn test_payment_on_expiry_ledger_uses_quoted_rate() {
    let s = setup();
    let id = s.checkout();
    s.client
        .create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10));
    s.oracle.set_price(&(ORACLE_RATE * 2), &NOW);
    s.set_ledger(START_LEDGER + 10);

    let charged = s.client.pay_oracle_priced_payment(&s.customer, &id);
    assert_eq!(charged, Setup::token_amount_at(ORACLE_RATE));
}

#[test]
fn test_payment_without_quote_uses_live_oracle_rate() {
    let s = setup();
    let id = s.checkout();
    let live = ORACLE_RATE * 2;
    s.oracle.set_price(&live, &NOW);

    let charged = s.client.pay_oracle_priced_payment(&s.customer, &id);
    assert_eq!(charged, Setup::token_amount_at(live));
    let record = s.client.get_oracle_priced_payment(&id);
    assert!(!record.paid_with_quote);
    assert_eq!(record.paid_rate, live);
}

#[test]
fn test_new_quote_replaces_previous_quote() {
    let s = setup();
    let id = s.checkout();
    s.client
        .create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 5));
    let second = ORACLE_RATE + ORACLE_RATE * 10 / 10_000;
    s.client.create_quote(&s.merchant, &id, &second, &(START_LEDGER + 500));

    s.set_ledger(START_LEDGER + 100); // past the first quote's expiry
    let charged = s.client.pay_oracle_priced_payment(&s.customer, &id);
    assert_eq!(charged, Setup::token_amount_at(second));
}

#[test]
fn test_merchant_slippage_tolerance_widens_quote_band() {
    let s = setup();
    let id = s.checkout();
    let rate = ORACLE_RATE + ORACLE_RATE * 150 / 10_000; // +1.5%

    // Rejected under the 50 bps global default...
    assert_eq!(
        s.client
            .try_create_quote(&s.merchant, &id, &rate, &(START_LEDGER + 10))
            .unwrap_err()
            .unwrap(),
        ExtError2::QuoteOutsideSlippage.into()
    );
    // ...accepted once the merchant allows 200 bps.
    s.client.set_merchant_slippage_tolerance(&s.merchant, &200);
    s.client
        .create_quote(&s.merchant, &id, &rate, &(START_LEDGER + 10));
    assert_eq!(s.client.get_quote(&id).unwrap().rate, rate);
}

#[test]
fn test_quoted_payment_completes_to_merchant() {
    let s = setup();
    let id = s.checkout();
    s.client
        .create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10));
    let charged = s.client.pay_oracle_priced_payment(&s.customer, &id);

    s.client.complete_payment(&id);
    assert_eq!(s.client.get_payment(&id).status, PaymentStatus::Completed);
    assert_eq!(s.pay_token_client.balance(&s.merchant), charged);
}

// ===========================================================================
//  create_oracle_priced_payment errors
// ===========================================================================

#[test]
fn test_create_oracle_priced_payment_rejects_non_positive_amount() {
    let s = setup();
    let err = s
        .client
        .try_create_oracle_priced_payment(&s.merchant, &s.customer, &0, &s.pay_token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError::InvalidAmount.into());
}

#[test]
fn test_create_oracle_priced_payment_requires_oracle() {
    let s = setup_unconfigured();
    let err = s
        .client
        .try_create_oracle_priced_payment(&s.merchant, &s.customer, &AMOUNT_USDC, &s.pay_token)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OracleNotConfigured.into());
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_create_oracle_priced_payment_rejects_when_paused() {
    let s = setup();
    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));
    s.checkout();
}

// ===========================================================================
//  create_quote errors
// ===========================================================================

#[test]
fn test_create_quote_unknown_payment() {
    let s = setup();
    let err = s
        .client
        .try_create_quote(&s.merchant, &99, &ORACLE_RATE, &(START_LEDGER + 10))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OraclePricedPaymentNotFound.into());
}

#[test]
fn test_create_quote_rejects_other_merchant() {
    let s = setup();
    let id = s.checkout();
    let other = Address::generate(&s.env);
    let err = s
        .client
        .try_create_quote(&other, &id, &ORACLE_RATE, &(START_LEDGER + 10))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::NotPaymentMerchant.into());
}

#[test]
fn test_create_quote_rejects_paid_payment() {
    let s = setup();
    let id = s.checkout();
    s.client.pay_oracle_priced_payment(&s.customer, &id);
    let err = s
        .client
        .try_create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::PaymentAlreadyPaid.into());
}

#[test]
fn test_create_quote_rejects_non_positive_rate() {
    let s = setup();
    let id = s.checkout();
    for rate in [0i128, -1] {
        let err = s
            .client
            .try_create_quote(&s.merchant, &id, &rate, &(START_LEDGER + 10))
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ExtError2::InvalidQuoteRate.into());
    }
}

#[test]
fn test_create_quote_rejects_expiry_not_in_future() {
    let s = setup();
    let id = s.checkout();
    for expiry in [START_LEDGER, START_LEDGER - 1] {
        let err = s
            .client
            .try_create_quote(&s.merchant, &id, &ORACLE_RATE, &expiry)
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ExtError2::InvalidQuoteExpiry.into());
    }
}

#[test]
fn test_create_quote_rejects_rate_outside_slippage() {
    let s = setup();
    let id = s.checkout();
    let above = ORACLE_RATE + ORACLE_RATE * 51 / 10_000;
    let below = ORACLE_RATE - ORACLE_RATE * 51 / 10_000;
    for rate in [above, below] {
        let err = s
            .client
            .try_create_quote(&s.merchant, &id, &rate, &(START_LEDGER + 10))
            .unwrap_err()
            .unwrap();
        assert_eq!(err, ExtError2::QuoteOutsideSlippage.into());
    }
}

#[test]
fn test_create_quote_rejects_stale_oracle_price() {
    let s = setup();
    let id = s.checkout();
    s.oracle.set_price(&ORACLE_RATE, &(NOW - MAX_ORACLE_AGE - 1));
    let err = s
        .client
        .try_create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OraclePriceStale.into());
}

#[test]
fn test_create_quote_rejects_missing_oracle_price() {
    let s = setup();
    let id = s.checkout();
    s.oracle.set_price(&0, &NOW); // lastprice → None
    let err = s
        .client
        .try_create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OraclePriceUnavailable.into());
}

#[test]
fn test_create_quote_rejects_non_positive_oracle_price() {
    let s = setup();
    let id = s.checkout();
    s.oracle.set_price(&-5, &NOW);
    let err = s
        .client
        .try_create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10))
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OraclePriceUnavailable.into());
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_create_quote_rejects_when_paused() {
    let s = setup();
    let id = s.checkout();
    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));
    s.client
        .create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10));
}

// ===========================================================================
//  pay_oracle_priced_payment errors
// ===========================================================================

#[test]
fn test_pay_rejects_expired_quote() {
    let s = setup();
    let id = s.checkout();
    s.client
        .create_quote(&s.merchant, &id, &ORACLE_RATE, &(START_LEDGER + 10));
    s.set_ledger(START_LEDGER + 11);

    let before = s.pay_token_client.balance(&s.customer);
    let err = s
        .client
        .try_pay_oracle_priced_payment(&s.customer, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::QuoteExpired.into());
    assert_eq!(s.pay_token_client.balance(&s.customer), before);
    assert!(!s.client.get_oracle_priced_payment(&id).paid);
}

#[test]
fn test_pay_unknown_payment() {
    let s = setup();
    let err = s
        .client
        .try_pay_oracle_priced_payment(&s.customer, &42)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OraclePricedPaymentNotFound.into());
}

#[test]
fn test_pay_rejects_other_customer() {
    let s = setup();
    let id = s.checkout();
    let stranger = Address::generate(&s.env);
    let err = s
        .client
        .try_pay_oracle_priced_payment(&stranger, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError::NotPaymentCustomer.into());
}

#[test]
fn test_pay_rejects_double_payment() {
    let s = setup();
    let id = s.checkout();
    s.client.pay_oracle_priced_payment(&s.customer, &id);
    let err = s
        .client
        .try_pay_oracle_priced_payment(&s.customer, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::PaymentAlreadyPaid.into());
}

#[test]
fn test_pay_without_quote_rejects_stale_oracle_price() {
    let s = setup();
    let id = s.checkout();
    s.oracle.set_price(&ORACLE_RATE, &(NOW - MAX_ORACLE_AGE - 1));
    let err = s
        .client
        .try_pay_oracle_priced_payment(&s.customer, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OraclePriceStale.into());
}

#[test]
fn test_pay_rejects_zero_token_amount() {
    let s = setup();
    // 1 USDC unit at a rate above 10^7 rounds down to 0 tokens.
    let id = s
        .client
        .create_oracle_priced_payment(&s.merchant, &s.customer, &1, &s.pay_token);
    let err = s
        .client
        .try_pay_oracle_priced_payment(&s.customer, &id)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError::InvalidAmount.into());
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_pay_rejects_when_paused() {
    let s = setup();
    let id = s.checkout();
    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));
    s.client.pay_oracle_priced_payment(&s.customer, &id);
}

#[test]
fn test_get_oracle_priced_payment_not_found() {
    let s = setup();
    let err = s
        .client
        .try_get_oracle_priced_payment(&7)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, ExtError2::OraclePricedPaymentNotFound.into());
    assert_eq!(s.client.get_quote(&7), None);
}
