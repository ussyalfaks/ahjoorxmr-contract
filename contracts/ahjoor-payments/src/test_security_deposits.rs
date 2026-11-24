#![cfg(test)]
//! #1167: refundable security deposits with itemized deductions.
use super::*;
use crate::security_deposits::MAX_DEPOSIT_DEDUCTIONS;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, BytesN, Env, String,
};

const START_LEDGER: u32 = 1_000;
const RETURN_BY: u32 = START_LEDGER + 100;
const DEPOSIT: i128 = 1_000;
const CUSTOMER_FUNDS: i128 = 10_000;

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
    env.ledger().with_mut(|l| l.sequence_number = START_LEDGER);

    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin, &admin, &0u32);

    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    TokenAdminClient::new(&env, &token).mint(&customer, &CUSTOMER_FUNDS);

    Setup {
        env,
        client,
        admin,
        token,
        token_client,
        customer,
        merchant,
    }
}

fn hash(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

impl<'a> Setup<'a> {
    fn deposit(&self) -> u32 {
        self.client.create_security_deposit(
            &self.merchant,
            &self.customer,
            &self.token,
            &DEPOSIT,
            &RETURN_BY,
        )
    }

    fn deductions(&self, amounts: &[i128]) -> Vec<(BytesN<32>, i128)> {
        let mut v = Vec::new(&self.env);
        for (i, amount) in amounts.iter().enumerate() {
            v.push_back((hash(&self.env, i as u8 + 1), *amount));
        }
        v
    }

    fn claim(&self, id: u32, amounts: &[i128]) {
        self.client
            .claim_deductions(&self.merchant, &id, &self.deductions(amounts));
    }

    fn set_ledger(&self, seq: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number = seq);
    }

    fn balance(&self, who: &Address) -> i128 {
        self.token_client.balance(who)
    }

    fn status(&self, id: u32) -> SecurityDepositStatus {
        self.client.get_security_deposit(&id).status
    }
}

fn assert_err<T: core::fmt::Debug, E: core::fmt::Debug>(
    res: Result<T, Result<soroban_sdk::Error, E>>,
    expected: ExtError2,
) {
    assert_eq!(res.unwrap_err().unwrap(), expected.into());
}

// ===========================================================================
//  Happy paths
// ===========================================================================

#[test]
fn test_create_security_deposit_escrows_funds() {
    let s = setup();
    let id = s.deposit();
    assert_eq!(id, 1);

    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.merchant, s.merchant);
    assert_eq!(d.customer, s.customer);
    assert_eq!(d.token, s.token);
    assert_eq!(d.amount, DEPOSIT);
    assert_eq!(d.return_by, RETURN_BY);
    assert_eq!(d.created_ledger, START_LEDGER);
    assert_eq!(d.status, SecurityDepositStatus::Held);
    assert_eq!(d.deductions.len(), 0);

    assert_eq!(s.balance(&s.customer), CUSTOMER_FUNDS - DEPOSIT);
    assert_eq!(s.balance(&s.client.address), DEPOSIT);

    assert_eq!(s.deposit(), 2);
}

#[test]
fn test_claim_and_accept_deductions_pays_out_itemized_amounts() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[300, 200]);

    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, SecurityDepositStatus::DeductionsProposed);
    assert_eq!(d.proposed_total, 500);
    assert_eq!(
        d.deductions,
        vec![&s.env, (hash(&s.env, 1), 300i128), (hash(&s.env, 2), 200i128)]
    );
    // Nothing moves until the customer responds.
    assert_eq!(s.balance(&s.client.address), DEPOSIT);

    s.client.accept_deductions(&s.customer, &id);
    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, SecurityDepositStatus::Settled);
    assert_eq!(d.deducted_amount, 500);
    assert_eq!(d.returned_amount, 500);
    assert_eq!(s.balance(&s.merchant), 500);
    assert_eq!(s.balance(&s.customer), CUSTOMER_FUNDS - 500);
    assert_eq!(s.balance(&s.client.address), 0);
}

#[test]
fn test_claim_on_return_by_ledger_and_full_deduction() {
    let s = setup();
    let id = s.deposit();
    s.set_ledger(RETURN_BY);
    s.claim(id, &[DEPOSIT]);
    s.client.accept_deductions(&s.customer, &id);

    assert_eq!(s.balance(&s.merchant), DEPOSIT);
    assert_eq!(s.balance(&s.customer), CUSTOMER_FUNDS - DEPOSIT);
    assert_eq!(s.client.get_security_deposit(&id).returned_amount, 0);
}

#[test]
fn test_dispute_then_admin_resolution() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[400]);
    s.client.dispute_deductions(&s.customer, &id);
    assert_eq!(s.status(id), SecurityDepositStatus::Disputed);

    s.client.resolve_deposit_dispute(&s.admin, &id, &150);
    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, SecurityDepositStatus::Settled);
    assert_eq!(d.deducted_amount, 150);
    assert_eq!(d.returned_amount, DEPOSIT - 150);
    assert_eq!(s.balance(&s.merchant), 150);
    assert_eq!(s.balance(&s.customer), CUSTOMER_FUNDS - 150);
}

#[test]
fn test_admin_can_resolve_unanswered_claim() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[250]);
    s.client.resolve_deposit_dispute(&s.admin, &id, &250);
    assert_eq!(s.status(id), SecurityDepositStatus::Settled);
    assert_eq!(s.balance(&s.merchant), 250);
}

#[test]
fn test_customer_reclaims_full_deposit_after_return_by() {
    let s = setup();
    let id = s.deposit();
    s.set_ledger(RETURN_BY + 1);
    s.client.reclaim_security_deposit(&s.customer, &id);

    let d = s.client.get_security_deposit(&id);
    assert_eq!(d.status, SecurityDepositStatus::Reclaimed);
    assert_eq!(d.returned_amount, DEPOSIT);
    assert_eq!(d.deducted_amount, 0);
    assert_eq!(s.balance(&s.customer), CUSTOMER_FUNDS);
    assert_eq!(s.balance(&s.client.address), 0);
}

#[test]
fn test_merchant_releases_full_deposit() {
    let s = setup();
    let id = s.deposit();
    s.client.release_security_deposit(&s.merchant, &id);
    assert_eq!(s.status(id), SecurityDepositStatus::Released);
    assert_eq!(s.balance(&s.customer), CUSTOMER_FUNDS);
}

#[test]
fn test_deductions_paid_to_merchant_payout_address() {
    let s = setup();
    let payout = Address::generate(&s.env);
    s.client.set_payout_change_delay_ledgers(&s.admin, &0);
    s.client.request_payout_address_change(&s.merchant, &payout);
    s.client.apply_payout_address_change(&s.merchant);

    let id = s.deposit();
    s.claim(id, &[100]);
    s.client.accept_deductions(&s.customer, &id);
    assert_eq!(s.balance(&payout), 100);
    assert_eq!(s.balance(&s.merchant), 0);
}

// ===========================================================================
//  create_security_deposit errors
// ===========================================================================

#[test]
fn test_create_rejects_non_positive_amount() {
    let s = setup();
    for amount in [0i128, -1] {
        assert_err(
            s.client.try_create_security_deposit(
                &s.merchant, &s.customer, &s.token, &amount, &RETURN_BY,
            ),
            ExtError2::InvalidDepositAmount,
        );
    }
}

#[test]
fn test_create_rejects_return_by_not_in_future() {
    let s = setup();
    for return_by in [START_LEDGER, START_LEDGER - 1] {
        assert_err(
            s.client.try_create_security_deposit(
                &s.merchant, &s.customer, &s.token, &DEPOSIT, &return_by,
            ),
            ExtError2::InvalidReturnBy,
        );
    }
}

#[test]
fn test_create_rejects_same_merchant_and_customer() {
    let s = setup();
    assert_err(
        s.client.try_create_security_deposit(
            &s.customer, &s.customer, &s.token, &DEPOSIT, &RETURN_BY,
        ),
        ExtError2::InvalidDepositParties,
    );
}

#[test]
fn test_create_rejects_token_not_accepted_by_merchant() {
    let s = setup();
    s.client
        .set_merchant_accepted_tokens(&s.merchant, &vec![&s.env, Address::generate(&s.env)]);
    let res = s.client.try_create_security_deposit(
        &s.merchant, &s.customer, &s.token, &DEPOSIT, &RETURN_BY,
    );
    assert_eq!(
        res.unwrap_err().unwrap(),
        ExtError::TokenNotAcceptedByMerchant.into()
    );
}

#[test]
#[should_panic(expected = "Contract is paused")]
fn test_create_rejects_when_paused() {
    let s = setup();
    s.client
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));
    s.deposit();
}

// ===========================================================================
//  claim_deductions errors
// ===========================================================================

#[test]
fn test_claim_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &9, &s.deductions(&[1])),
        ExtError2::SecurityDepositNotFound,
    );
}

#[test]
fn test_claim_rejects_other_merchant() {
    let s = setup();
    let id = s.deposit();
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_claim_deductions(&other, &id, &s.deductions(&[1])),
        ExtError2::NotDepositMerchant,
    );
}

#[test]
fn test_claim_rejects_second_claim() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[100]);
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &s.deductions(&[50])),
        ExtError2::InvalidDepositStatus,
    );
}

#[test]
fn test_claim_rejects_after_return_by() {
    let s = setup();
    let id = s.deposit();
    s.set_ledger(RETURN_BY + 1);
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &s.deductions(&[100])),
        ExtError2::ReturnDeadlinePassed,
    );
}

#[test]
fn test_claim_rejects_empty_deductions() {
    let s = setup();
    let id = s.deposit();
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &Vec::new(&s.env)),
        ExtError2::InvalidDeduction,
    );
}

#[test]
fn test_claim_rejects_too_many_deductions() {
    let s = setup();
    let id = s.deposit();
    let items = [1i128; (MAX_DEPOSIT_DEDUCTIONS + 1) as usize];
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &s.deductions(&items)),
        ExtError2::InvalidDeduction,
    );
}

#[test]
fn test_claim_rejects_non_positive_deduction() {
    let s = setup();
    let id = s.deposit();
    for bad in [0i128, -10] {
        assert_err(
            s.client
                .try_claim_deductions(&s.merchant, &id, &s.deductions(&[100, bad])),
            ExtError2::InvalidDeduction,
        );
    }
}

#[test]
fn test_claim_rejects_deductions_exceeding_deposit() {
    let s = setup();
    let id = s.deposit();
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &s.deductions(&[600, 401])),
        ExtError2::DeductionsExceedDeposit,
    );
    assert_err(
        s.client
            .try_claim_deductions(&s.merchant, &id, &s.deductions(&[i128::MAX, 1])),
        ExtError2::DeductionsExceedDeposit,
    );
}

// ===========================================================================
//  accept_deductions / dispute_deductions errors
// ===========================================================================

#[test]
fn test_accept_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client.try_accept_deductions(&s.customer, &9),
        ExtError2::SecurityDepositNotFound,
    );
}

#[test]
fn test_accept_rejects_other_customer() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[100]);
    assert_err(
        s.client
            .try_accept_deductions(&Address::generate(&s.env), &id),
        ExtError2::NotDepositCustomer,
    );
}

#[test]
fn test_accept_rejects_without_proposed_deductions() {
    let s = setup();
    let id = s.deposit();
    assert_err(
        s.client.try_accept_deductions(&s.customer, &id),
        ExtError2::InvalidDepositStatus,
    );
}

#[test]
fn test_dispute_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client.try_dispute_deductions(&s.customer, &9),
        ExtError2::SecurityDepositNotFound,
    );
}

#[test]
fn test_dispute_rejects_other_customer() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[100]);
    assert_err(
        s.client
            .try_dispute_deductions(&Address::generate(&s.env), &id),
        ExtError2::NotDepositCustomer,
    );
}

#[test]
fn test_dispute_rejects_without_proposed_deductions() {
    let s = setup();
    let id = s.deposit();
    assert_err(
        s.client.try_dispute_deductions(&s.customer, &id),
        ExtError2::InvalidDepositStatus,
    );
    // Cannot dispute twice either.
    s.claim(id, &[100]);
    s.client.dispute_deductions(&s.customer, &id);
    assert_err(
        s.client.try_dispute_deductions(&s.customer, &id),
        ExtError2::InvalidDepositStatus,
    );
}

// ===========================================================================
//  resolve_deposit_dispute errors
// ===========================================================================

#[test]
fn test_resolve_rejects_non_admin() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[100]);
    s.client.dispute_deductions(&s.customer, &id);
    assert_err(
        s.client
            .try_resolve_deposit_dispute(&s.merchant, &id, &100),
        ExtError2::NotAdmin,
    );
}

#[test]
fn test_resolve_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client.try_resolve_deposit_dispute(&s.admin, &9, &0),
        ExtError2::SecurityDepositNotFound,
    );
}

#[test]
fn test_resolve_rejects_held_or_closed_deposit() {
    let s = setup();
    let id = s.deposit();
    assert_err(
        s.client.try_resolve_deposit_dispute(&s.admin, &id, &0),
        ExtError2::InvalidDepositStatus,
    );
    s.claim(id, &[100]);
    s.client.accept_deductions(&s.customer, &id);
    assert_err(
        s.client.try_resolve_deposit_dispute(&s.admin, &id, &0),
        ExtError2::InvalidDepositStatus,
    );
}

#[test]
fn test_resolve_rejects_award_out_of_range() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[300]);
    s.client.dispute_deductions(&s.customer, &id);
    for award in [-1i128, 301] {
        assert_err(
            s.client.try_resolve_deposit_dispute(&s.admin, &id, &award),
            ExtError2::InvalidDeduction,
        );
    }
}

// ===========================================================================
//  reclaim / release errors
// ===========================================================================

#[test]
fn test_reclaim_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client.try_reclaim_security_deposit(&s.customer, &9),
        ExtError2::SecurityDepositNotFound,
    );
}

#[test]
fn test_reclaim_rejects_other_customer() {
    let s = setup();
    let id = s.deposit();
    s.set_ledger(RETURN_BY + 1);
    assert_err(
        s.client
            .try_reclaim_security_deposit(&Address::generate(&s.env), &id),
        ExtError2::NotDepositCustomer,
    );
}

#[test]
fn test_reclaim_rejects_before_return_by_passes() {
    let s = setup();
    let id = s.deposit();
    s.set_ledger(RETURN_BY);
    assert_err(
        s.client.try_reclaim_security_deposit(&s.customer, &id),
        ExtError2::ReturnDeadlineNotReached,
    );
}

#[test]
fn test_reclaim_rejects_when_merchant_claimed_deductions() {
    let s = setup();
    let id = s.deposit();
    s.claim(id, &[100]);
    s.set_ledger(RETURN_BY + 1);
    assert_err(
        s.client.try_reclaim_security_deposit(&s.customer, &id),
        ExtError2::InvalidDepositStatus,
    );
}

#[test]
fn test_release_unknown_deposit() {
    let s = setup();
    assert_err(
        s.client.try_release_security_deposit(&s.merchant, &9),
        ExtError2::SecurityDepositNotFound,
    );
}

#[test]
fn test_release_rejects_other_merchant() {
    let s = setup();
    let id = s.deposit();
    assert_err(
        s.client
            .try_release_security_deposit(&Address::generate(&s.env), &id),
        ExtError2::NotDepositMerchant,
    );
}

#[test]
fn test_release_rejects_closed_deposit() {
    let s = setup();
    let id = s.deposit();
    s.client.release_security_deposit(&s.merchant, &id);
    assert_err(
        s.client.try_release_security_deposit(&s.merchant, &id),
        ExtError2::InvalidDepositStatus,
    );
}

#[test]
fn test_get_security_deposit_not_found() {
    let s = setup();
    assert_err(
        s.client.try_get_security_deposit(&1),
        ExtError2::SecurityDepositNotFound,
    );
}
