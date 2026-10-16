#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env, String,
};

const AMOUNT: i128 = 1_000;
const BPS: u32 = 1_000; // 10%
const HOLD: u32 = 100;
const HELD: i128 = AMOUNT * BPS as i128 / 10_000;

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token: Address,
    token_client: TokenClient<'a>,
    token_admin: TokenAdminClient<'a>,
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

    client.initialize(&admin, &fee_recipient, &0);

    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    token_admin.mint(&customer, &100_000);

    Setup {
        env,
        client,
        admin,
        token,
        token_client,
        token_admin,
        customer,
        merchant,
    }
}

impl<'a> Setup<'a> {
    fn configure(&self) {
        self.client.set_rolling_reserve(&self.merchant, &BPS, &HOLD);
    }

    /// Creates and completes a payment of `amount`; completion settles it.
    fn pay(&self, amount: i128) -> u32 {
        let id = self.client.create_payment(
            &self.customer,
            &self.merchant,
            &amount,
            &self.token,
            &None,
            &None,
            &None,
        );
        self.client.complete_payment(&id);
        id
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token_client.balance(who)
    }

    fn reserve(&self) -> i128 {
        self.client
            .get_rolling_reserve_balance(&self.merchant, &self.token)
    }

    fn set_ledger(&self, seq: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number = seq);
    }

    /// Moves `amount` out of the merchant's wallet so refunds come up short.
    fn spend_merchant_funds(&self, amount: i128) {
        let sink = Address::generate(&self.env);
        self.token_client.transfer(&self.merchant, &sink, &amount);
    }
}

// ── Configuration ────────────────────────────────────────────────────────────

#[test]
fn test_set_rolling_reserve_stores_config() {
    let s = setup();
    assert_eq!(s.client.get_rolling_reserve(&s.merchant), None);
    s.configure();
    assert_eq!(
        s.client.get_rolling_reserve(&s.merchant),
        Some(RollingReserveConfig { bps: BPS, hold_ledgers: HOLD })
    );
}

#[test]
fn test_set_rolling_reserve_zero_bps_clears_config() {
    let s = setup();
    s.configure();
    s.client.set_rolling_reserve(&s.merchant, &0, &0);
    assert_eq!(s.client.get_rolling_reserve(&s.merchant), None);

    s.pay(AMOUNT);
    assert_eq!(s.balance(&s.merchant), AMOUNT);
    assert_eq!(s.reserve(), 0);
}

#[test]
fn test_set_rolling_reserve_rejects_invalid_config() {
    let s = setup();
    assert_eq!(
        s.client.try_set_rolling_reserve(&s.merchant, &10_001, &HOLD),
        Err(Ok(ExtError::InvalidRollingReserveConfig.into()))
    );
    assert_eq!(
        s.client.try_set_rolling_reserve(&s.merchant, &BPS, &0),
        Err(Ok(ExtError::InvalidRollingReserveConfig.into()))
    );
}

#[test]
fn test_set_rolling_reserve_requires_admin_auth() {
    let s = setup();
    s.configure();
    let auths = s.env.auths();
    assert_eq!(auths.len(), 1);
    assert_eq!(auths[0].0, s.admin);
}

// ── Holding on settlement ────────────────────────────────────────────────────

#[test]
fn test_no_hold_without_config() {
    let s = setup();
    s.pay(AMOUNT);
    assert_eq!(s.balance(&s.merchant), AMOUNT);
    assert_eq!(s.client.get_rolling_reserve_buckets(&s.merchant).len(), 0);
}

#[test]
fn test_completed_payment_holds_reserve_share() {
    let s = setup();
    s.configure();
    s.set_ledger(500);
    s.pay(AMOUNT);

    assert_eq!(s.balance(&s.merchant), AMOUNT - HELD);
    assert_eq!(s.balance(&s.client.address), HELD);
    assert_eq!(s.reserve(), HELD);

    let buckets = s.client.get_rolling_reserve_buckets(&s.merchant);
    assert_eq!(buckets.len(), 1);
    assert_eq!(
        buckets.get(0).unwrap(),
        ReserveBucket {
            token: s.token.clone(),
            amount: HELD,
            held_at_ledger: 500,
            release_ledger: 500 + HOLD,
        }
    );
}

#[test]
fn test_batch_settlement_holds_reserve_share() {
    let s = setup();
    s.configure();

    // A dispute released to the merchant leaves the payment completed but
    // unsettled, so it goes through settle_merchant_payments.
    let id = s.client.create_payment(
        &s.customer,
        &s.merchant,
        &AMOUNT,
        &s.token,
        &None,
        &None,
        &None,
    );
    s.client
        .dispute_payment(&s.customer, &id, &String::from_str(&s.env, "late"));
    s.client.resolve_dispute(&id, &true);
    assert!(!s.client.is_settled(&id));

    s.client
        .settle_merchant_payments(&s.admin, &s.merchant, &vec![&s.env, id]);

    assert!(s.client.is_settled(&id));
    assert_eq!(s.balance(&s.merchant), AMOUNT - HELD);
    assert_eq!(s.reserve(), HELD);
}

// ── Release timing ───────────────────────────────────────────────────────────

#[test]
fn test_release_only_after_hold_period() {
    let s = setup();
    s.configure();
    s.set_ledger(1_000);
    s.pay(AMOUNT);

    // Immediately, and one ledger before maturity: nothing is released.
    assert_eq!(s.client.release_rolling_reserve(&s.merchant, &s.token), 0);
    s.set_ledger(1_000 + HOLD - 1);
    assert_eq!(
        s.client
            .get_releasable_rolling_reserve(&s.merchant, &s.token),
        0
    );
    assert_eq!(s.client.release_rolling_reserve(&s.merchant, &s.token), 0);
    assert_eq!(s.balance(&s.merchant), AMOUNT - HELD);
    assert_eq!(s.reserve(), HELD);

    // At maturity the bucket is paid out.
    s.set_ledger(1_000 + HOLD);
    assert_eq!(
        s.client
            .get_releasable_rolling_reserve(&s.merchant, &s.token),
        HELD
    );
    assert_eq!(s.client.release_rolling_reserve(&s.merchant, &s.token), HELD);
    assert_eq!(s.balance(&s.merchant), AMOUNT);
    assert_eq!(s.reserve(), 0);
    assert_eq!(s.client.get_rolling_reserve_buckets(&s.merchant).len(), 0);

    // A second release has nothing left to pay.
    assert_eq!(s.client.release_rolling_reserve(&s.merchant, &s.token), 0);
    assert_eq!(s.balance(&s.merchant), AMOUNT);
}

#[test]
fn test_buckets_release_on_their_own_schedule() {
    let s = setup();
    s.configure();
    s.set_ledger(100);
    s.pay(AMOUNT);
    s.set_ledger(150);
    s.pay(2 * AMOUNT);
    assert_eq!(s.reserve(), HELD * 3);

    // Only the first bucket has matured.
    s.set_ledger(100 + HOLD);
    assert_eq!(s.client.release_rolling_reserve(&s.merchant, &s.token), HELD);
    assert_eq!(s.reserve(), HELD * 2);

    s.set_ledger(150 + HOLD);
    assert_eq!(
        s.client.release_rolling_reserve(&s.merchant, &s.token),
        HELD * 2
    );
    assert_eq!(s.reserve(), 0);
    assert_eq!(s.balance(&s.merchant), AMOUNT * 3);
}

#[test]
fn test_release_goes_to_active_payout_address() {
    let s = setup();
    s.configure();
    s.client.set_payout_change_delay_ledgers(&s.admin, &10);
    s.set_ledger(10);
    s.pay(AMOUNT);

    let new_payout = Address::generate(&s.env);
    s.client
        .request_payout_address_change(&s.merchant, &new_payout);
    s.set_ledger(10 + HOLD);
    s.client.apply_payout_address_change(&s.merchant);

    s.client.release_rolling_reserve(&s.merchant, &s.token);
    assert_eq!(s.balance(&new_payout), HELD);
}

#[test]
fn test_release_ignores_other_tokens() {
    let s = setup();
    s.configure();
    s.pay(AMOUNT);
    s.set_ledger(HOLD);

    let other = s
        .env
        .register_stellar_asset_contract_v2(s.admin.clone())
        .address();
    assert_eq!(s.client.release_rolling_reserve(&s.merchant, &other), 0);
    assert_eq!(s.reserve(), HELD);
}

// ── Refund drawdown ──────────────────────────────────────────────────────────

#[test]
fn test_refund_from_merchant_balance_leaves_reserve_untouched() {
    let s = setup();
    s.configure();
    let id = s.pay(AMOUNT);
    let customer_before = s.balance(&s.customer);

    s.client.refund_settled_payment(&s.merchant, &id, &200);

    assert_eq!(s.balance(&s.customer), customer_before + 200);
    assert_eq!(s.balance(&s.merchant), AMOUNT - HELD - 200);
    assert_eq!(s.reserve(), HELD);
    assert_eq!(s.client.get_rolling_reserve_drawdowns(&s.merchant).len(), 0);
    assert_eq!(s.client.get_payment(&id).refunded_amount, 200);
    assert_eq!(s.client.get_payment(&id).status, PaymentStatus::Completed);
}

#[test]
fn test_refund_draws_shortfall_from_reserve_and_records_it() {
    let s = setup();
    s.configure();
    s.set_ledger(77);
    let id = s.pay(AMOUNT);
    // Leave the merchant only 30 so a 100 refund is short by 70.
    s.spend_merchant_funds(AMOUNT - HELD - 30);
    let customer_before = s.balance(&s.customer);

    s.client.refund_settled_payment(&s.merchant, &id, &100);

    assert_eq!(s.balance(&s.customer), customer_before + 100);
    assert_eq!(s.balance(&s.merchant), 0);
    assert_eq!(s.reserve(), HELD - 70);
    assert_eq!(s.balance(&s.client.address), HELD - 70);

    let drawdowns = s.client.get_rolling_reserve_drawdowns(&s.merchant);
    assert_eq!(drawdowns.len(), 1);
    assert_eq!(
        drawdowns.get(0).unwrap(),
        ReserveDrawdown {
            payment_id: id,
            token: s.token.clone(),
            amount: 70,
            ledger: 77,
        }
    );
}

#[test]
fn test_refund_drawdown_spans_buckets_oldest_first() {
    let s = setup();
    s.configure();
    s.set_ledger(10);
    let id = s.pay(AMOUNT);
    s.set_ledger(20);
    s.pay(AMOUNT);
    s.spend_merchant_funds(s.balance(&s.merchant));

    s.client
        .refund_settled_payment(&s.merchant, &id, &(HELD + 40));

    let buckets = s.client.get_rolling_reserve_buckets(&s.merchant);
    assert_eq!(buckets.len(), 1);
    let left = buckets.get(0).unwrap();
    assert_eq!(left.held_at_ledger, 20);
    assert_eq!(left.amount, HELD - 40);
}

#[test]
fn test_refund_fails_when_reserve_cannot_cover_shortfall() {
    let s = setup();
    s.configure();
    let id = s.pay(AMOUNT);
    s.spend_merchant_funds(s.balance(&s.merchant));
    let customer_before = s.balance(&s.customer);

    let result = s
        .client
        .try_refund_settled_payment(&s.merchant, &id, &(HELD + 1));
    assert_eq!(result, Err(Ok(ExtError::InsufficientRollingReserve.into())));

    assert_eq!(s.balance(&s.customer), customer_before);
    assert_eq!(s.reserve(), HELD);
    assert_eq!(s.client.get_rolling_reserve_drawdowns(&s.merchant).len(), 0);
    assert_eq!(s.client.get_payment(&id).refunded_amount, 0);
}

#[test]
fn test_full_refund_marks_refunded_and_dequeues_withdrawal() {
    let s = setup();
    s.configure();
    let id = s.pay(AMOUNT);
    assert_eq!(s.client.get_merchant_withdrawal_queue(&s.merchant).len(), 1);

    // Merchant holds AMOUNT - HELD; the remaining HELD comes from the reserve.
    s.client.refund_settled_payment(&s.merchant, &id, &AMOUNT);

    let payment = s.client.get_payment(&id);
    assert_eq!(payment.status, PaymentStatus::Refunded);
    assert_eq!(payment.refunded_amount, AMOUNT);
    assert_eq!(s.reserve(), 0);
    assert_eq!(s.client.get_merchant_withdrawal_queue(&s.merchant).len(), 0);
    assert_eq!(
        s.client.get_rolling_reserve_drawdowns(&s.merchant).get(0).unwrap().amount,
        HELD
    );
}

#[test]
fn test_refund_rejects_unsettled_payment() {
    let s = setup();
    s.configure();
    let id = s.client.create_payment(
        &s.customer,
        &s.merchant,
        &AMOUNT,
        &s.token,
        &None,
        &None,
        &None,
    );
    let result = s.client.try_refund_settled_payment(&s.merchant, &id, &10);
    assert_eq!(result, Err(Ok(ExtError::PaymentNotSettled.into())));
}

#[test]
#[should_panic(expected = "Refund amount exceeds remaining balance")]
fn test_refund_cannot_exceed_payment() {
    let s = setup();
    s.configure();
    let id = s.pay(AMOUNT);
    s.token_admin.mint(&s.merchant, &AMOUNT);
    s.client
        .refund_settled_payment(&s.merchant, &id, &(AMOUNT + 1));
}

#[test]
#[should_panic(expected = "Payment does not belong to merchant")]
fn test_refund_rejects_other_merchant() {
    let s = setup();
    s.configure();
    let id = s.pay(AMOUNT);
    let other = Address::generate(&s.env);
    s.client.refund_settled_payment(&other, &id, &10);
}
