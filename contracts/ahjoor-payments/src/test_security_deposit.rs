#![cfg(test)]
//! #1167: refundable security deposits with itemized deductions.
use super::*;
use crate::security_deposit::{
    DepositError, DepositStatus, DEPOSIT_RESPONSE_WINDOW_SECONDS, MAX_DEPOSIT_DEDUCTIONS,
};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, BytesN, Env,
};

const START: u64 = 10_000;
const RETURN_BY: u64 = START + 86_400;
const DEPOSIT: i128 = 1_000;
const BALANCE: i128 = 10_000;

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token: Address,
    token_client: TokenClient<'a>,
    customer: Address,
    merchant: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START);
    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.initialize(&admin, &Address::generate(&env), &0);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    TokenAdminClient::new(&env, &token).mint(&customer, &BALANCE);
    let token_client = TokenClient::new(&env, &token);
    Setup {
        env,
        client,
        admin,
        token: token.clone(),
        token_client,
        customer,
        merchant,
    }
}

impl<'a> Setup<'a> {
    fn create(&self) -> u32 {
        self.client.create_security_deposit(
            &self.merchant,
            &self.customer,
            &self.token,
            &DEPOSIT,
            &RETURN_BY,
        )
    }

    fn hash(&self, b: u8) -> BytesN<32> {
        BytesN::from_array(&self.env, &[b; 32])
    }

    /// Claims 200 + 100 = 300 in deductions.
    fn claim(&self, id: u32) {
        self.client.claim_deductions(
            &self.merchant,
            &id,
            &vec![&self.env, (self.hash(1), 200i128), (self.hash(2), 100i128)],
        );
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token_client.balance(who)
    }

    fn after_response_window(&self) {
        self.env
            .ledger()
            .set_timestamp(START + DEPOSIT_RESPONSE_WINDOW_SECONDS + 1);
    }
}

fn assert_err<T: core::fmt::Debug>(
    result: Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: DepositError,
) {
    assert_eq!(result.unwrap_err().unwrap(), expected.into());
}

// --- happy paths ---

#[test]
fn test_create_holds_funds() {
    let s = setup();
    let id = s.create();
    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.id, id);
    assert_eq!(d.merchant, s.merchant);
    assert_eq!(d.customer, s.customer);
    assert_eq!(d.amount, DEPOSIT);
    assert_eq!(d.return_by, RETURN_BY);
    assert_eq!(d.status, DepositStatus::Held);
    assert_eq!(s.balance(&s.customer), BALANCE - DEPOSIT);
    assert_eq!(s.balance(&s.client.address), DEPOSIT);
    assert_eq!(s.create(), id + 1);
}

#[test]
fn test_release_returns_full_deposit() {
    let s = setup();
    let id = s.create();
    s.client.release_deposit(&s.merchant, &id);
    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, DepositStatus::Settled);
    assert_eq!((d.merchant_paid, d.customer_refunded), (0, DEPOSIT));
    assert_eq!(s.balance(&s.customer), BALANCE);
    assert_eq!(s.balance(&s.client.address), 0);
}

#[test]
fn test_accept_deductions_settles() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, DepositStatus::DeductionsProposed);
    assert_eq!(d.deductions.len(), 2);
    assert_eq!(d.deductions.get(0).unwrap().reason_hash, s.hash(1));
    assert_eq!(d.deductions_total, 300);
    assert_eq!(d.proposed_at, START);

    s.client.accept_deductions(&s.customer, &id);

    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, DepositStatus::Settled);
    assert_eq!((d.merchant_paid, d.customer_refunded), (300, 700));
    assert_eq!(s.balance(&s.merchant), 300);
    assert_eq!(s.balance(&s.customer), BALANCE - 300);
    assert_eq!(s.balance(&s.client.address), 0);
}

#[test]
fn test_deductions_may_equal_full_deposit() {
    let s = setup();
    let id = s.create();
    s.client
        .claim_deductions(&s.merchant, &id, &vec![&s.env, (s.hash(1), DEPOSIT)]);
    s.client.accept_deductions(&s.customer, &id);
    assert_eq!(s.balance(&s.merchant), DEPOSIT);
    assert_eq!(s.balance(&s.customer), BALANCE - DEPOSIT);
}

#[test]
fn test_dispute_then_admin_resolution() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.client.dispute_deductions(&s.customer, &id);
    assert_eq!(
        s.client.get_security_deposit(&id).status,
        DepositStatus::Disputed
    );

    s.client.resolve_deposit_dispute(&s.admin, &id, &120);

    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, DepositStatus::Settled);
    assert_eq!((d.merchant_paid, d.customer_refunded), (120, 880));
    assert_eq!(s.balance(&s.merchant), 120);
    assert_eq!(s.balance(&s.customer), BALANCE - 120);
}

#[test]
fn test_merchant_finalizes_after_silence() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.after_response_window();
    s.client.finalize_deductions(&s.merchant, &id);
    assert_eq!(s.balance(&s.merchant), 300);
    assert_eq!(s.balance(&s.customer), BALANCE - 300);
}

#[test]
fn test_customer_reclaims_when_merchant_does_nothing() {
    let s = setup();
    let id = s.create();
    s.env.ledger().set_timestamp(RETURN_BY + 1);
    s.client.reclaim_deposit(&s.customer, &id);
    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, DepositStatus::Reclaimed);
    assert_eq!(d.customer_refunded, DEPOSIT);
    assert_eq!(s.balance(&s.customer), BALANCE);
    assert_eq!(s.balance(&s.client.address), 0);
}

// --- create errors ---

#[test]
fn test_create_rejects_non_positive_amount() {
    let s = setup();
    assert_err(
        s.client
            .try_create_security_deposit(&s.merchant, &s.customer, &s.token, &0, &RETURN_BY),
        DepositError::InvalidDepositAmount,
    );
}

#[test]
fn test_create_rejects_same_parties() {
    let s = setup();
    assert_err(
        s.client.try_create_security_deposit(
            &s.customer,
            &s.customer,
            &s.token,
            &DEPOSIT,
            &RETURN_BY,
        ),
        DepositError::InvalidDepositParties,
    );
}

#[test]
fn test_create_rejects_past_return_by() {
    let s = setup();
    assert_err(
        s.client
            .try_create_security_deposit(&s.merchant, &s.customer, &s.token, &DEPOSIT, &START),
        DepositError::InvalidReturnBy,
    );
}

#[test]
fn test_get_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client.try_get_security_deposit(&42),
        DepositError::DepositNotFound,
    );
}

// --- claim_deductions errors ---

#[test]
fn test_claim_wrong_merchant() {
    let s = setup();
    let id = s.create();
    let other = Address::generate(&s.env);
    assert_err(
        s.client
            .try_claim_deductions(&other, &id, &vec![&s.env, (s.hash(1), 10i128)]),
        DepositError::NotDepositMerchant,
    );
}

#[test]
fn test_claim_after_return_by() {
    let s = setup();
    let id = s.create();
    s.env.ledger().set_timestamp(RETURN_BY + 1);
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &vec![&s.env, (s.hash(1), 10i128)]),
        DepositError::ReturnDeadlinePassed,
    );
}

#[test]
fn test_claim_empty() {
    let s = setup();
    let id = s.create();
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &vec![&s.env]),
        DepositError::InvalidDeductions,
    );
}

#[test]
fn test_claim_non_positive_item() {
    let s = setup();
    let id = s.create();
    assert_err(
        s.client.try_claim_deductions(
            &s.merchant,
            &id,
            &vec![&s.env, (s.hash(1), 10i128), (s.hash(2), 0i128)],
        ),
        DepositError::InvalidDeductions,
    );
}

#[test]
fn test_claim_exceeds_deposit() {
    let s = setup();
    let id = s.create();
    assert_err(
        s.client.try_claim_deductions(
            &s.merchant,
            &id,
            &vec![&s.env, (s.hash(1), DEPOSIT), (s.hash(2), 1i128)],
        ),
        DepositError::DeductionsExceedDeposit,
    );
}

#[test]
fn test_claim_overflow_is_rejected() {
    let s = setup();
    let id = s.create();
    assert_err(
        s.client.try_claim_deductions(
            &s.merchant,
            &id,
            &vec![&s.env, (s.hash(1), i128::MAX), (s.hash(2), i128::MAX)],
        ),
        DepositError::DeductionsExceedDeposit,
    );
}

#[test]
fn test_claim_too_many_items() {
    let s = setup();
    let id = s.create();
    let mut items = vec![&s.env];
    for i in 0..=MAX_DEPOSIT_DEDUCTIONS {
        items.push_back((s.hash(i as u8), 1i128));
    }
    assert_err(
        s.client.try_claim_deductions(&s.merchant, &id, &items),
        DepositError::TooManyDeductions,
    );
}

#[test]
fn test_claim_twice() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &vec![&s.env, (s.hash(3), 10i128)]),
        DepositError::DepositNotHeld,
    );
}

#[test]
fn test_claim_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &7, &vec![&s.env, (s.hash(1), 10i128)]),
        DepositError::DepositNotFound,
    );
}

// --- release_deposit errors ---

#[test]
fn test_release_wrong_merchant() {
    let s = setup();
    let id = s.create();
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_release_deposit(&other, &id),
        DepositError::NotDepositMerchant,
    );
}

#[test]
fn test_release_after_claim() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    assert_err(
        s.client.try_release_deposit(&s.merchant, &id),
        DepositError::DepositNotHeld,
    );
}

// --- accept_deductions errors ---

#[test]
fn test_accept_wrong_customer() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_accept_deductions(&other, &id),
        DepositError::NotDepositCustomer,
    );
}

#[test]
fn test_accept_without_proposal() {
    let s = setup();
    let id = s.create();
    assert_err(
        s.client.try_accept_deductions(&s.customer, &id),
        DepositError::NoDeductionsProposed,
    );
}

// --- dispute_deductions errors ---

#[test]
fn test_dispute_wrong_customer() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_dispute_deductions(&other, &id),
        DepositError::NotDepositCustomer,
    );
}

#[test]
fn test_dispute_without_proposal() {
    let s = setup();
    let id = s.create();
    assert_err(
        s.client.try_dispute_deductions(&s.customer, &id),
        DepositError::NoDeductionsProposed,
    );
}

#[test]
fn test_dispute_after_response_window() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.after_response_window();
    assert_err(
        s.client.try_dispute_deductions(&s.customer, &id),
        DepositError::ResponseWindowClosed,
    );
}

// --- finalize_deductions errors ---

#[test]
fn test_finalize_wrong_merchant() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.after_response_window();
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_finalize_deductions(&other, &id),
        DepositError::NotDepositMerchant,
    );
}

#[test]
fn test_finalize_while_window_open() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    assert_err(
        s.client.try_finalize_deductions(&s.merchant, &id),
        DepositError::ResponseWindowOpen,
    );
}

#[test]
fn test_finalize_without_proposal() {
    let s = setup();
    let id = s.create();
    assert_err(
        s.client.try_finalize_deductions(&s.merchant, &id),
        DepositError::NoDeductionsProposed,
    );
}

#[test]
fn test_finalize_after_dispute() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.client.dispute_deductions(&s.customer, &id);
    s.after_response_window();
    assert_err(
        s.client.try_finalize_deductions(&s.merchant, &id),
        DepositError::NoDeductionsProposed,
    );
}

// --- resolve_deposit_dispute errors ---

#[test]
fn test_resolve_not_admin() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.client.dispute_deductions(&s.customer, &id);
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_resolve_deposit_dispute(&other, &id, &100),
        DepositError::NotAdmin,
    );
}

#[test]
fn test_resolve_not_disputed() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    assert_err(
        s.client.try_resolve_deposit_dispute(&s.admin, &id, &100),
        DepositError::DepositNotDisputed,
    );
}

#[test]
fn test_resolve_above_claim() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.client.dispute_deductions(&s.customer, &id);
    assert_err(
        s.client.try_resolve_deposit_dispute(&s.admin, &id, &301),
        DepositError::InvalidDepositResolution,
    );
}

#[test]
fn test_resolve_negative() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.client.dispute_deductions(&s.customer, &id);
    assert_err(
        s.client.try_resolve_deposit_dispute(&s.admin, &id, &-1),
        DepositError::InvalidDepositResolution,
    );
}

// --- reclaim_deposit errors ---

#[test]
fn test_reclaim_before_return_by() {
    let s = setup();
    let id = s.create();
    s.env.ledger().set_timestamp(RETURN_BY);
    assert_err(
        s.client.try_reclaim_deposit(&s.customer, &id),
        DepositError::ReturnDeadlineNotReached,
    );
}

#[test]
fn test_reclaim_wrong_customer() {
    let s = setup();
    let id = s.create();
    s.env.ledger().set_timestamp(RETURN_BY + 1);
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_reclaim_deposit(&other, &id),
        DepositError::NotDepositCustomer,
    );
}

#[test]
fn test_reclaim_after_merchant_claimed() {
    let s = setup();
    let id = s.create();
    s.claim(id);
    s.env.ledger().set_timestamp(RETURN_BY + 1);
    assert_err(
        s.client.try_reclaim_deposit(&s.customer, &id),
        DepositError::DepositNotHeld,
    );
}

#[test]
fn test_reclaim_twice() {
    let s = setup();
    let id = s.create();
    s.env.ledger().set_timestamp(RETURN_BY + 1);
    s.client.reclaim_deposit(&s.customer, &id);
    assert_err(
        s.client.try_reclaim_deposit(&s.customer, &id),
        DepositError::DepositNotHeld,
    );
}
