#![cfg(test)]
//! #1169: multisig approval for merchant withdrawals above a threshold.
use super::*;
use crate::withdrawal_policy::{
    LargeWithdrawalStatus, WithdrawalPolicyError, MAX_WITHDRAWAL_SIGNERS,
    WITHDRAWAL_POLICY_WINDOW_SECONDS,
};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env,
};

const COLLATERAL: i128 = 10_000;
const LIMIT: i128 = 1_000;

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    usdc_client: TokenClient<'a>,
    merchant: Address,
    s1: Address,
    s2: Address,
    s3: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let contract_id = env.register(AhjoorPaymentsContract, ());
    let client = AhjoorPaymentsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let usdc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    client.initialize(&admin, &Address::generate(&env), &0);
    client.set_oracle(&Address::generate(&env), &usdc, &3600);
    client.set_min_collateral(&0i128);

    let merchant = Address::generate(&env);
    TokenAdminClient::new(&env, &usdc).mint(&merchant, &COLLATERAL);
    client.deposit_collateral(&merchant, &COLLATERAL);

    Setup {
        client,
        usdc_client: TokenClient::new(&env, &usdc),
        merchant,
        s1: Address::generate(&env),
        s2: Address::generate(&env),
        s3: Address::generate(&env),
        env,
    }
}

impl<'a> Setup<'a> {
    /// 2-of-3 policy; the merchant key alone may withdraw `LIMIT` per window.
    fn set_policy(&self) {
        self.client.set_withdrawal_policy(
            &self.merchant,
            &vec![&self.env, self.s1.clone(), self.s2.clone(), self.s3.clone()],
            &2,
            &LIMIT,
            &vec![&self.env],
        );
    }

    fn signers(&self) -> Vec<Address> {
        vec![&self.env, self.s1.clone(), self.s2.clone(), self.s3.clone()]
    }

    fn collateral(&self) -> i128 {
        self.client.get_collateral_balance(&self.merchant)
    }

    fn wallet(&self) -> i128 {
        self.usdc_client.balance(&self.merchant)
    }
}

fn assert_err<T: core::fmt::Debug>(
    result: Result<T, Result<soroban_sdk::Error, soroban_sdk::InvokeError>>,
    expected: WithdrawalPolicyError,
) {
    assert_eq!(result.unwrap_err().unwrap(), expected.into());
}

// --- behaviour without / with a policy ---

#[test]
fn test_without_policy_full_withdrawal_is_allowed() {
    let s = setup();
    assert_eq!(s.client.get_withdrawal_policy(&s.merchant), None);
    s.client.withdraw_collateral(&s.merchant, &COLLATERAL);
    assert_eq!(s.wallet(), COLLATERAL);
}

#[test]
fn test_set_policy_and_read_back() {
    let s = setup();
    s.set_policy();
    let p = s.client.get_withdrawal_policy(&s.merchant).unwrap();
    assert_eq!(p.signers, s.signers());
    assert_eq!(p.threshold, 2);
    assert_eq!(p.amount_threshold, LIMIT);
}

#[test]
fn test_small_withdrawals_allowed_up_to_threshold_per_window() {
    let s = setup();
    s.set_policy();
    s.client.withdraw_collateral(&s.merchant, &600);
    s.client.withdraw_collateral(&s.merchant, &400);
    assert_eq!(s.wallet(), LIMIT);

    // Splitting does not get around the threshold.
    assert_err(
        s.client.try_withdraw_collateral(&s.merchant, &1),
        WithdrawalPolicyError::LargeWithdrawalRequiresApproval,
    );

    // A new window restores the single-key allowance.
    s.env
        .ledger()
        .set_timestamp(1_000 + WITHDRAWAL_POLICY_WINDOW_SECONDS);
    s.client.withdraw_collateral(&s.merchant, &LIMIT);
    assert_eq!(s.wallet(), 2 * LIMIT);
}

#[test]
fn test_large_direct_withdrawal_is_rejected() {
    let s = setup();
    s.set_policy();
    assert_err(
        s.client.try_withdraw_collateral(&s.merchant, &(LIMIT + 1)),
        WithdrawalPolicyError::LargeWithdrawalRequiresApproval,
    );
    assert_eq!(s.collateral(), COLLATERAL);
}

#[test]
fn test_large_withdrawal_queues_until_threshold_approvals() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);

    // Funds are locked immediately.
    assert_eq!(s.collateral(), COLLATERAL - 5_000);
    let r = s.client.get_large_withdrawal(&id);
    assert_eq!(r.status, LargeWithdrawalStatus::Pending);
    assert_eq!(r.amount, 5_000);

    assert!(!s.client.approve_large_withdrawal(&s.s1, &id));
    assert_eq!(s.wallet(), 0);
    assert_eq!(
        s.client.get_large_withdrawal(&id).status,
        LargeWithdrawalStatus::Pending
    );

    assert!(s.client.approve_large_withdrawal(&s.s3, &id));
    assert_eq!(s.wallet(), 5_000);
    let r = s.client.get_large_withdrawal(&id);
    assert_eq!(r.status, LargeWithdrawalStatus::Executed);
    assert_eq!(r.approvals, vec![&s.env, s.s1.clone(), s.s3.clone()]);
}

#[test]
fn test_cancel_returns_locked_amount() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    s.client.cancel_large_withdrawal(&s.merchant, &id);
    assert_eq!(s.collateral(), COLLATERAL);
    assert_eq!(
        s.client.get_large_withdrawal(&id).status,
        LargeWithdrawalStatus::Cancelled
    );
    assert_eq!(s.client.request_large_withdrawal(&s.merchant, &1), id + 1);
}

#[test]
fn test_policy_change_needs_current_threshold() {
    let s = setup();
    s.set_policy();
    let s4 = Address::generate(&s.env);

    assert_err(
        s.client.try_set_withdrawal_policy(
            &s.merchant,
            &vec![&s.env, s4.clone()],
            &1,
            &i128::MAX,
            &vec![&s.env, s.s1.clone()],
        ),
        WithdrawalPolicyError::InsufficientPolicyApprovals,
    );

    s.client.set_withdrawal_policy(
        &s.merchant,
        &vec![&s.env, s4.clone()],
        &1,
        &i128::MAX,
        &vec![&s.env, s.s1.clone(), s.s2.clone()],
    );

    // Each approver had to authorize the change.
    let auths = s.env.auths();
    for who in [&s.merchant, &s.s1, &s.s2] {
        assert!(auths.iter().any(|(addr, _)| addr == who));
    }

    let p = s.client.get_withdrawal_policy(&s.merchant).unwrap();
    assert_eq!(p.signers, vec![&s.env, s4]);
    assert_eq!(p.threshold, 1);
}

#[test]
fn test_merchant_signer_can_approve_policy_change() {
    let s = setup();
    s.client.set_withdrawal_policy(
        &s.merchant,
        &vec![&s.env, s.merchant.clone(), s.s1.clone()],
        &2,
        &LIMIT,
        &vec![&s.env],
    );
    s.client.set_withdrawal_policy(
        &s.merchant,
        &vec![&s.env, s.merchant.clone(), s.s1.clone()],
        &1,
        &LIMIT,
        &vec![&s.env, s.merchant.clone(), s.s1.clone()],
    );
    assert_eq!(
        s.client
            .get_withdrawal_policy(&s.merchant)
            .unwrap()
            .threshold,
        1
    );
}

#[test]
fn test_remove_policy_with_quorum() {
    let s = setup();
    s.set_policy();
    s.client.withdraw_collateral(&s.merchant, &LIMIT);
    s.client
        .remove_withdrawal_policy(&s.merchant, &vec![&s.env, s.s2.clone(), s.s3.clone()]);
    assert_eq!(s.client.get_withdrawal_policy(&s.merchant), None);
    s.client
        .withdraw_collateral(&s.merchant, &(COLLATERAL - LIMIT));
    assert_eq!(s.wallet(), COLLATERAL);
}

#[test]
fn test_approvals_from_removed_signers_do_not_count() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    s.client.approve_large_withdrawal(&s.s1, &id);

    let s4 = Address::generate(&s.env);
    s.client.set_withdrawal_policy(
        &s.merchant,
        &vec![&s.env, s.s2.clone(), s.s3.clone(), s4.clone()],
        &2,
        &LIMIT,
        &vec![&s.env, s.s1.clone(), s.s2.clone()],
    );

    assert!(!s.client.approve_large_withdrawal(&s.s2, &id));
    assert!(s.client.approve_large_withdrawal(&s4, &id));
    assert_eq!(s.wallet(), 5_000);
}

// --- set_withdrawal_policy errors ---

#[test]
fn test_set_policy_invalid_configs() {
    let s = setup();
    let signers = s.signers();
    let none = vec![&s.env];
    let cases: [(Vec<Address>, u32, i128); 5] = [
        // no signers
        (vec![&s.env], 1, LIMIT),
        // zero threshold
        (signers.clone(), 0, LIMIT),
        // threshold above signer count
        (signers.clone(), 4, LIMIT),
        // negative amount threshold
        (signers.clone(), 2, -1),
        // duplicate signer
        (vec![&s.env, s.s1.clone(), s.s1.clone()], 1, LIMIT),
    ];
    for (signers, threshold, amount) in cases.iter() {
        assert_err(
            s.client
                .try_set_withdrawal_policy(&s.merchant, signers, threshold, amount, &none),
            WithdrawalPolicyError::InvalidWithdrawalPolicy,
        );
    }

    let mut too_many = vec![&s.env];
    for _ in 0..=MAX_WITHDRAWAL_SIGNERS {
        too_many.push_back(Address::generate(&s.env));
    }
    assert_err(
        s.client
            .try_set_withdrawal_policy(&s.merchant, &too_many, &1, &LIMIT, &none),
        WithdrawalPolicyError::InvalidWithdrawalPolicy,
    );
}

#[test]
fn test_policy_change_with_non_signer_approver() {
    let s = setup();
    s.set_policy();
    let outsider = Address::generate(&s.env);
    assert_err(
        s.client.try_set_withdrawal_policy(
            &s.merchant,
            &s.signers(),
            &1,
            &LIMIT,
            &vec![&s.env, s.s1.clone(), outsider],
        ),
        WithdrawalPolicyError::NotAPolicySigner,
    );
}

#[test]
fn test_policy_change_with_duplicate_approver() {
    let s = setup();
    s.set_policy();
    assert_err(
        s.client.try_set_withdrawal_policy(
            &s.merchant,
            &s.signers(),
            &1,
            &LIMIT,
            &vec![&s.env, s.s1.clone(), s.s1.clone()],
        ),
        WithdrawalPolicyError::DuplicatePolicyApprover,
    );
}

// --- remove_withdrawal_policy errors ---

#[test]
fn test_remove_without_policy() {
    let s = setup();
    assert_err(
        s.client
            .try_remove_withdrawal_policy(&s.merchant, &vec![&s.env]),
        WithdrawalPolicyError::NoWithdrawalPolicy,
    );
}

#[test]
fn test_remove_without_quorum() {
    let s = setup();
    s.set_policy();
    assert_err(
        s.client
            .try_remove_withdrawal_policy(&s.merchant, &vec![&s.env, s.s1.clone()]),
        WithdrawalPolicyError::InsufficientPolicyApprovals,
    );
}

// --- request_large_withdrawal errors ---

#[test]
fn test_request_without_policy() {
    let s = setup();
    assert_err(
        s.client.try_request_large_withdrawal(&s.merchant, &5_000),
        WithdrawalPolicyError::NoWithdrawalPolicy,
    );
}

#[test]
fn test_request_non_positive_amount() {
    let s = setup();
    s.set_policy();
    assert_err(
        s.client.try_request_large_withdrawal(&s.merchant, &0),
        WithdrawalPolicyError::InvalidWithdrawalAmount,
    );
}

#[test]
fn test_request_more_than_collateral() {
    let s = setup();
    s.set_policy();
    assert_err(
        s.client
            .try_request_large_withdrawal(&s.merchant, &(COLLATERAL + 1)),
        WithdrawalPolicyError::InsufficientCollateral,
    );
}

#[test]
fn test_request_below_minimum_collateral() {
    let s = setup();
    s.set_policy();
    s.client.set_min_collateral(&5_000i128);
    assert_err(
        s.client.try_request_large_withdrawal(&s.merchant, &5_001),
        WithdrawalPolicyError::BelowMinimumCollateral,
    );
}

// --- approve_large_withdrawal errors ---

#[test]
fn test_approve_unknown_request() {
    let s = setup();
    s.set_policy();
    assert_err(
        s.client.try_approve_large_withdrawal(&s.s1, &99),
        WithdrawalPolicyError::WithdrawalRequestNotFound,
    );
}

#[test]
fn test_approve_by_non_signer() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    let outsider = Address::generate(&s.env);
    assert_err(
        s.client.try_approve_large_withdrawal(&outsider, &id),
        WithdrawalPolicyError::NotAPolicySigner,
    );
}

#[test]
fn test_approve_twice() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    s.client.approve_large_withdrawal(&s.s1, &id);
    assert_err(
        s.client.try_approve_large_withdrawal(&s.s1, &id),
        WithdrawalPolicyError::WithdrawalAlreadyApproved,
    );
}

#[test]
fn test_approve_after_execution() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    s.client.approve_large_withdrawal(&s.s1, &id);
    s.client.approve_large_withdrawal(&s.s2, &id);
    assert_err(
        s.client.try_approve_large_withdrawal(&s.s3, &id),
        WithdrawalPolicyError::WithdrawalRequestNotPending,
    );
}

#[test]
fn test_approve_after_policy_removed() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    s.client
        .remove_withdrawal_policy(&s.merchant, &vec![&s.env, s.s1.clone(), s.s2.clone()]);
    assert_err(
        s.client.try_approve_large_withdrawal(&s.s1, &id),
        WithdrawalPolicyError::NoWithdrawalPolicy,
    );
    // The merchant can still cancel and recover the locked collateral.
    s.client.cancel_large_withdrawal(&s.merchant, &id);
    assert_eq!(s.collateral(), COLLATERAL);
}

// --- cancel_large_withdrawal errors ---

#[test]
fn test_cancel_by_other_merchant() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    let other = Address::generate(&s.env);
    assert_err(
        s.client.try_cancel_large_withdrawal(&other, &id),
        WithdrawalPolicyError::NotWithdrawalRequester,
    );
}

#[test]
fn test_cancel_after_execution() {
    let s = setup();
    s.set_policy();
    let id = s.client.request_large_withdrawal(&s.merchant, &5_000);
    s.client.approve_large_withdrawal(&s.s1, &id);
    s.client.approve_large_withdrawal(&s.s2, &id);
    assert_err(
        s.client.try_cancel_large_withdrawal(&s.merchant, &id),
        WithdrawalPolicyError::WithdrawalRequestNotPending,
    );
}

#[test]
fn test_cancel_unknown_request() {
    let s = setup();
    assert_err(
        s.client.try_cancel_large_withdrawal(&s.merchant, &99),
        WithdrawalPolicyError::WithdrawalRequestNotFound,
    );
}
