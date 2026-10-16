#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger, MockAuth, MockAuthInvoke},
    vec, Address, Env, IntoVal, String,
};

const DELAY: u32 = 50;
const START_LEDGER: u32 = 1_000;

struct Setup<'a> {
    env: Env,
    client: AhjoorPaymentsContractClient<'a>,
    admin: Address,
    token: Address,
    token_client: TokenClient<'a>,
    token_admin: TokenAdminClient<'a>,
    merchant: Address,
    new_payout: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.sequence_number = START_LEDGER);

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
    client.set_payout_change_delay_ledgers(&admin, &DELAY);

    let merchant = Address::generate(&env);
    let new_payout = Address::generate(&env);

    Setup {
        env,
        client,
        admin,
        token,
        token_client,
        token_admin,
        merchant,
        new_payout,
    }
}

impl<'a> Setup<'a> {
    fn request(&self) {
        self.client
            .request_payout_address_change(&self.merchant, &self.new_payout);
    }

    fn set_ledger(&self, seq: u32) {
        self.env.ledger().with_mut(|l| l.sequence_number = seq);
    }

    /// A completed payment left unsettled by a merchant-favoured dispute, so
    /// `settle_merchant_payments` pays it to the active payout address.
    fn unsettled_payment(&self, amount: i128) -> u32 {
        let customer = Address::generate(&self.env);
        self.token_admin.mint(&customer, &amount);
        let id = self.client.create_payment(
            &customer,
            &self.merchant,
            &amount,
            &self.token,
            &None,
            &None,
            &None,
        );
        self.client
            .dispute_payment(&customer, &id, &String::from_str(&self.env, "check"));
        self.client.resolve_dispute(&id, &true);
        id
    }

    fn settle(&self, id: u32) {
        self.client
            .settle_merchant_payments(&self.admin, &self.merchant, &vec![&self.env, id]);
    }

    /// Configures the contract's collateral token and deposits collateral.
    fn deposit_collateral(&self, amount: i128) {
        self.client
            .set_oracle(&Address::generate(&self.env), &self.token, &3_600);
        self.client.set_min_collateral(&0);
        self.token_admin.mint(&self.merchant, &amount);
        self.client.deposit_collateral(&self.merchant, &amount);
    }
}

// ── Defaults and delay configuration ─────────────────────────────────────────

#[test]
fn test_payout_address_defaults_to_merchant() {
    let s = setup();
    assert_eq!(s.client.get_payout_address(&s.merchant), s.merchant);
    assert_eq!(s.client.get_pending_payout_change(&s.merchant), None);
}

#[test]
fn test_default_delay_used_when_unset() {
    let env = Env::default();
    env.mock_all_auths();
    let client = AhjoorPaymentsContractClient::new(&env, &env.register(AhjoorPaymentsContract, ()));
    client.initialize(&Address::generate(&env), &Address::generate(&env), &0);
    assert_eq!(
        client.get_payout_change_delay_ledgers(),
        DEFAULT_PAYOUT_CHANGE_DELAY_LEDGERS
    );
}

#[test]
fn test_admin_sets_delay() {
    let s = setup();
    assert_eq!(s.client.get_payout_change_delay_ledgers(), DELAY);
    s.client.set_payout_change_delay_ledgers(&s.admin, &7);
    assert_eq!(s.client.get_payout_change_delay_ledgers(), 7);
}

#[test]
#[should_panic(expected = "Only admin")]
fn test_non_admin_cannot_set_delay() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    s.client.set_payout_change_delay_ledgers(&outsider, &7);
}

// ── Request ──────────────────────────────────────────────────────────────────

#[test]
fn test_request_creates_pending_change_after_delay() {
    let s = setup();
    s.request();
    assert_eq!(
        s.client.get_pending_payout_change(&s.merchant),
        Some(PendingPayoutChange {
            new_payout: s.new_payout.clone(),
            effective_ledger: START_LEDGER + DELAY,
        })
    );
    // The active address is unchanged until the change is applied.
    assert_eq!(s.client.get_payout_address(&s.merchant), s.merchant);
}

#[test]
fn test_new_request_replaces_pending_change() {
    let s = setup();
    s.request();
    s.set_ledger(START_LEDGER + 10);
    let other = Address::generate(&s.env);
    s.client.request_payout_address_change(&s.merchant, &other);

    let pending = s.client.get_pending_payout_change(&s.merchant).unwrap();
    assert_eq!(pending.new_payout, other);
    assert_eq!(pending.effective_ledger, START_LEDGER + 10 + DELAY);
}

#[test]
fn test_request_requires_merchant_auth() {
    let s = setup();
    s.request();
    assert_eq!(s.env.auths()[0].0, s.merchant);
}

#[test]
fn test_only_merchant_can_request() {
    let s = setup();
    let attacker = Address::generate(&s.env);
    let result = s
        .client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &s.client.address,
                fn_name: "request_payout_address_change",
                args: (&s.merchant, &attacker).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_request_payout_address_change(&s.merchant, &attacker);
    assert!(result.is_err());
    assert_eq!(s.client.get_pending_payout_change(&s.merchant), None);
}

// ── Apply ────────────────────────────────────────────────────────────────────

#[test]
fn test_apply_before_delay_fails() {
    let s = setup();
    s.request();

    let result = s.client.try_apply_payout_address_change(&s.merchant);
    assert_eq!(result, Err(Ok(ExtError::PayoutChangeNotReady.into())));

    s.set_ledger(START_LEDGER + DELAY - 1);
    let result = s.client.try_apply_payout_address_change(&s.merchant);
    assert_eq!(result, Err(Ok(ExtError::PayoutChangeNotReady.into())));

    assert_eq!(s.client.get_payout_address(&s.merchant), s.merchant);
    assert!(s.client.get_pending_payout_change(&s.merchant).is_some());
}

#[test]
fn test_apply_after_delay_succeeds_and_clears_pending() {
    let s = setup();
    s.request();
    s.set_ledger(START_LEDGER + DELAY);

    s.client.apply_payout_address_change(&s.merchant);

    assert_eq!(s.client.get_payout_address(&s.merchant), s.new_payout);
    assert_eq!(s.client.get_pending_payout_change(&s.merchant), None);
    // A second apply has nothing pending.
    assert_eq!(
        s.client.try_apply_payout_address_change(&s.merchant),
        Err(Ok(ExtError::NoPendingPayoutChange.into()))
    );
}

#[test]
fn test_apply_is_permissionless_once_due() {
    let s = setup();
    s.request();
    s.set_ledger(START_LEDGER + DELAY);

    s.env.set_auths(&[]);
    s.client.apply_payout_address_change(&s.merchant);
    assert_eq!(s.client.get_payout_address(&s.merchant), s.new_payout);
}

#[test]
fn test_apply_without_pending_change_fails() {
    let s = setup();
    assert_eq!(
        s.client.try_apply_payout_address_change(&s.merchant),
        Err(Ok(ExtError::NoPendingPayoutChange.into()))
    );
}

// ── Cancel ───────────────────────────────────────────────────────────────────

#[test]
fn test_cancel_clears_pending_change() {
    let s = setup();
    s.request();
    s.client.cancel_payout_address_change(&s.merchant);

    assert_eq!(s.client.get_pending_payout_change(&s.merchant), None);
    s.set_ledger(START_LEDGER + DELAY);
    assert_eq!(
        s.client.try_apply_payout_address_change(&s.merchant),
        Err(Ok(ExtError::NoPendingPayoutChange.into()))
    );
    assert_eq!(s.client.get_payout_address(&s.merchant), s.merchant);
}

#[test]
fn test_cancel_without_pending_change_fails() {
    let s = setup();
    assert_eq!(
        s.client.try_cancel_payout_address_change(&s.merchant),
        Err(Ok(ExtError::NoPendingPayoutChange.into()))
    );
}

#[test]
fn test_cancel_requires_merchant_auth() {
    let s = setup();
    s.request();
    s.client.cancel_payout_address_change(&s.merchant);
    assert_eq!(s.env.auths()[0].0, s.merchant);
}

#[test]
fn test_only_merchant_can_cancel() {
    let s = setup();
    s.request();
    let attacker = Address::generate(&s.env);
    let result = s
        .client
        .mock_auths(&[MockAuth {
            address: &attacker,
            invoke: &MockAuthInvoke {
                contract: &s.client.address,
                fn_name: "cancel_payout_address_change",
                args: (&s.merchant,).into_val(&s.env),
                sub_invokes: &[],
            },
        }])
        .try_cancel_payout_address_change(&s.merchant);
    assert!(result.is_err());
    assert!(s.client.get_pending_payout_change(&s.merchant).is_some());
}

// ── Withdrawals while a change is pending ────────────────────────────────────

#[test]
fn test_settlement_goes_to_old_address_while_change_pending() {
    let s = setup();
    let first = s.unsettled_payment(400);
    let second = s.unsettled_payment(600);
    s.request();

    s.settle(first);
    assert_eq!(s.token_client.balance(&s.merchant), 400);
    assert_eq!(s.token_client.balance(&s.new_payout), 0);

    s.set_ledger(START_LEDGER + DELAY);
    s.client.apply_payout_address_change(&s.merchant);
    s.settle(second);
    assert_eq!(s.token_client.balance(&s.merchant), 400);
    assert_eq!(s.token_client.balance(&s.new_payout), 600);
}

#[test]
fn test_collateral_withdrawal_goes_to_old_address_while_change_pending() {
    let s = setup();
    s.deposit_collateral(1_000);
    s.request();

    s.client.withdraw_collateral(&s.merchant, &300);
    assert_eq!(s.token_client.balance(&s.merchant), 300);
    assert_eq!(s.token_client.balance(&s.new_payout), 0);

    s.set_ledger(START_LEDGER + DELAY);
    s.client.apply_payout_address_change(&s.merchant);
    s.client.withdraw_collateral(&s.merchant, &200);
    assert_eq!(s.token_client.balance(&s.merchant), 300);
    assert_eq!(s.token_client.balance(&s.new_payout), 200);
}

#[test]
fn test_cancelled_change_keeps_withdrawals_on_old_address() {
    let s = setup();
    let id = s.unsettled_payment(500);
    s.request();
    s.client.cancel_payout_address_change(&s.merchant);
    s.set_ledger(START_LEDGER + DELAY);

    s.settle(id);
    assert_eq!(s.token_client.balance(&s.merchant), 500);
    assert_eq!(s.token_client.balance(&s.new_payout), 0);
}
