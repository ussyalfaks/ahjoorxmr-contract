#![cfg(test)]
//! #1170: Refunds paid in the customer's preferred token via the swap router.
use super::*;
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use ahjoor_token_whitelist::{TokenWhitelistContract, TokenWhitelistContractClient};
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{contract, contractimpl, testutils::Address as _, vec, Address, Env, String};

// ---------------------------------------------------------------------------
// Mock swap router
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone)]
enum MockRouterKey {
    QuoteBps,
    ExecBps,
    FailQuote,
    FailSwap,
    IgnoreMinOut,
}

/// Quotes `amount_in * quote_bps / 10_000` and executes at
/// `amount_in * exec_bps / 10_000`, reverting when that is below
/// `min_amount_out` (unless `ignore_min_out`, which simulates a broken router).
#[contract]
pub struct MockSwapRouter;

#[contractimpl]
impl MockSwapRouter {
    pub fn configure(
        env: Env,
        quote_bps: i128,
        exec_bps: i128,
        fail_quote: bool,
        fail_swap: bool,
        ignore_min_out: bool,
    ) {
        let s = env.storage().instance();
        s.set(&MockRouterKey::QuoteBps, &quote_bps);
        s.set(&MockRouterKey::ExecBps, &exec_bps);
        s.set(&MockRouterKey::FailQuote, &fail_quote);
        s.set(&MockRouterKey::FailSwap, &fail_swap);
        s.set(&MockRouterKey::IgnoreMinOut, &ignore_min_out);
    }

    pub fn quote(env: Env, _token_in: Address, _token_out: Address, amount_in: i128) -> i128 {
        let s = env.storage().instance();
        if s.get(&MockRouterKey::FailQuote).unwrap_or(false) {
            panic!("quote unavailable");
        }
        let bps: i128 = s.get(&MockRouterKey::QuoteBps).unwrap_or(10_000);
        amount_in * bps / 10_000
    }

    pub fn swap(
        env: Env,
        sender: Address,
        token_in: Address,
        token_out: Address,
        amount_in: i128,
        min_amount_out: i128,
        recipient: Address,
    ) -> i128 {
        let s = env.storage().instance();
        if s.get(&MockRouterKey::FailSwap).unwrap_or(false) {
            panic!("pool unavailable");
        }
        let this = env.current_contract_address();
        TokenClient::new(&env, &token_in).transfer_from(&this, &sender, &this, &amount_in);
        let bps: i128 = s.get(&MockRouterKey::ExecBps).unwrap_or(10_000);
        let out = amount_in * bps / 10_000;
        if !s.get(&MockRouterKey::IgnoreMinOut).unwrap_or(false) && out < min_amount_out {
            panic!("router: slippage");
        }
        TokenClient::new(&env, &token_out).transfer(&this, &recipient, &out);
        out
    }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    refund: AhjoorRefundContractClient<'a>,
    payments: AhjoorPaymentsContractClient<'a>,
    router: MockSwapRouterClient<'a>,
    admin: Address,
    customer: Address,
    merchant: Address,
    fee_recipient: Address,
    /// Payment token.
    token_a: Address,
    /// Customer's preferred refund token (e.g. a stablecoin).
    token_b: Address,
}

fn setup_with(refund_fee_bps: u32, with_router: bool) -> Setup<'static> {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let customer = Address::generate(&env);
    let merchant = Address::generate(&env);
    let fee_recipient = Address::generate(&env);
    let config = Some(RefundInitConfig {
        escrow_contract: None,
        refund_fee_bps,
        fee_recipient: Some(fee_recipient.clone()),
        auto_reject_window_seconds: 0,
        appeal_window_seconds: 0,
        refund_tiers: None,
        refund_cooldown_seconds: 0,
        customer_cancel_window_seconds: 0,
    });

    let payments =
        AhjoorPaymentsContractClient::new(&env, &env.register(AhjoorPaymentsContract, ()));
    payments.initialize(&admin, &admin, &0u32);
    let refund = AhjoorRefundContractClient::new(&env, &env.register(AhjoorRefundContract, ()));
    refund.initialize(&admin, &payments.address, &86_400u64, &config);

    let token_a = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_b = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let router = MockSwapRouterClient::new(&env, &env.register(MockSwapRouter, ()));
    // 1 token_a = 2 token_b, executed at the quoted rate.
    router.configure(&20_000, &20_000, &false, &false, &false);
    TokenAdminClient::new(&env, &token_b).mint(&router.address, &1_000_000);
    if with_router {
        refund.set_swap_router(&admin, &Some(router.address.clone()));
    }

    Setup {
        env,
        refund,
        payments,
        router,
        admin,
        customer,
        merchant,
        fee_recipient,
        token_a,
        token_b,
    }
}

fn setup() -> Setup<'static> {
    setup_with(0, true)
}

impl<'a> Setup<'a> {
    fn completed_payment(&self, amount: i128) -> u32 {
        // Payment + refund escrow.
        TokenAdminClient::new(&self.env, &self.token_a).mint(&self.customer, &(amount * 2));
        let pid = self.payments.create_payment(
            &self.customer,
            &self.merchant,
            &amount,
            &self.token_a,
            &None,
            &None,
            &None,
        );
        self.payments.complete_payment(&pid);
        pid
    }

    fn reason(&self) -> String {
        String::from_str(&self.env, "Prefer stablecoin")
    }

    fn request_in(&self, pid: u32, amount: i128, preferred: &Address) -> u32 {
        self.refund.request_refund_in_token(
            &self.customer,
            &pid,
            &amount,
            &self.reason(),
            &0,
            preferred,
        )
    }

    fn approve_and_process(&self, refund_id: u32) {
        self.refund.approve_refund(&self.admin, &refund_id);
        self.refund.process_refund(&self.admin, &refund_id);
    }

    fn balance(&self, token: &Address, who: &Address) -> i128 {
        TokenClient::new(&self.env, token).balance(who)
    }

    /// Whitelist token_a (and token_b if `allow_b`), optionally allowing the
    /// token_a/token_b swap pair with the given max slippage.
    fn whitelist(
        &self,
        allow_b: bool,
        allow_pair: Option<u32>,
    ) -> TokenWhitelistContractClient<'a> {
        let wl = TokenWhitelistContractClient::new(
            &self.env,
            &self.env.register(TokenWhitelistContract, ()),
        );
        wl.initialize(&self.admin);
        wl.add_token(&self.admin, &self.token_a);
        if allow_b {
            wl.add_token(&self.admin, &self.token_b);
        }
        if let Some(max_bps) = allow_pair {
            wl.allow_swap_pair(&self.admin, &self.token_a, &self.token_b, &max_bps);
        }
        self.refund
            .set_token_whitelist_contract(&self.admin, &wl.address);
        wl
    }

    /// Asserts the refund fell back to the original token in full.
    fn assert_fell_back(&self, refund_id: u32, amount: i128, customer_a_before: i128) {
        assert_eq!(
            self.balance(&self.token_a, &self.customer),
            customer_a_before + amount
        );
        assert_eq!(self.balance(&self.token_b, &self.customer), 0);
        assert_eq!(
            self.refund.get_refund_payout(&refund_id),
            Some(RefundPayout {
                token: self.token_a.clone(),
                amount,
                swapped: false
            })
        );
        assert_eq!(
            self.refund.get_refund(&refund_id).status,
            RefundStatus::Processed
        );
        self.assert_no_router_allowance();
    }

    fn assert_no_router_allowance(&self) {
        assert_eq!(
            TokenClient::new(&self.env, &self.token_a)
                .allowance(&self.refund.address, &self.router.address),
            0
        );
    }
}

// ---------------------------------------------------------------------------
// Happy path
// ---------------------------------------------------------------------------

#[test]
fn test_refund_paid_in_preferred_token() {
    let s = setup();
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    assert_eq!(
        s.refund.get_refund_preferred_token(&rid),
        Some(s.token_b.clone())
    );
    assert_eq!(s.refund.get_refund_payout(&rid), None);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);

    // 500 token_a swapped 1:2 into token_b, delivered to the customer.
    assert_eq!(s.balance(&s.token_b, &s.customer), 1_000);
    assert_eq!(s.balance(&s.token_a, &s.customer), a_before);
    assert_eq!(s.balance(&s.token_a, &s.router.address), 500);
    assert_eq!(s.balance(&s.token_a, &s.refund.address), 0);
    assert_eq!(
        s.refund.get_refund_payout(&rid),
        Some(RefundPayout {
            token: s.token_b.clone(),
            amount: 1_000,
            swapped: true
        })
    );
    assert_eq!(s.refund.get_refund(&rid).status, RefundStatus::Processed);
    s.assert_no_router_allowance();
}

#[test]
fn test_swap_within_slippage_tolerance_succeeds() {
    let s = setup();
    // Executes 0.5% below the quote; default tolerance is 1%.
    s.router.configure(&20_000, &19_900, &false, &false, &false);
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    s.approve_and_process(rid);

    assert_eq!(s.balance(&s.token_b, &s.customer), 995);
    assert_eq!(s.refund.get_refund_payout(&rid).unwrap().swapped, true);
}

#[test]
fn test_preferred_token_with_whitelist_and_allowed_pair() {
    let s = setup();
    s.whitelist(true, Some(100));
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    s.approve_and_process(rid);

    assert_eq!(s.balance(&s.token_b, &s.customer), 1_000);
    assert_eq!(s.refund.get_refund_payout(&rid).unwrap().token, s.token_b);
}

#[test]
fn test_fee_is_taken_in_original_token_and_rest_is_swapped() {
    // 2% refund fee (the contract maximum).
    let s = setup_with(200, true);
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    s.approve_and_process(rid);

    // 10 token_a fee, 490 token_a swapped into 980 token_b.
    assert_eq!(s.balance(&s.token_a, &s.fee_recipient), 10);
    assert_eq!(s.balance(&s.token_b, &s.customer), 980);
    assert_eq!(s.refund.get_refund(&rid).fee_amount, Some(10));
    assert_eq!(
        s.refund.get_refund_payout(&rid),
        Some(RefundPayout {
            token: s.token_b.clone(),
            amount: 980,
            swapped: true
        })
    );
}

#[test]
fn test_bulk_process_pays_preferred_token() {
    let s = setup();
    let pid1 = s.completed_payment(300);
    let pid2 = s.completed_payment(200);
    let r1 = s.request_in(pid1, 300, &s.token_b);
    let r2 = s
        .refund
        .request_refund(&s.customer, &pid2, &200, &s.reason(), &0);
    s.refund.approve_refund(&s.admin, &r1);
    s.refund.approve_refund(&s.admin, &r2);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.refund
        .bulk_process_refunds(&s.admin, &vec![&s.env, r1, r2]);

    assert_eq!(s.balance(&s.token_b, &s.customer), 600);
    assert_eq!(s.balance(&s.token_a, &s.customer), a_before + 200);
    assert_eq!(s.refund.get_refund_payout(&r1).unwrap().swapped, true);
    assert_eq!(s.refund.get_refund_payout(&r2).unwrap().swapped, false);
}

#[test]
fn test_preferred_token_equal_to_payment_token_is_plain_refund() {
    let s = setup();
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_a);
    assert_eq!(s.refund.get_refund_preferred_token(&rid), None);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
    // Router never touched.
    assert_eq!(s.balance(&s.token_a, &s.router.address), 0);
}

#[test]
fn test_plain_request_refund_records_original_token_payout() {
    let s = setup();
    let pid = s.completed_payment(500);
    let rid = s
        .refund
        .request_refund(&s.customer, &pid, &500, &s.reason(), &0);
    assert_eq!(s.refund.get_refund_preferred_token(&rid), None);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
}

// ---------------------------------------------------------------------------
// request_refund_in_token validation errors
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "TokenNotAllowed")]
fn test_request_rejects_non_whitelisted_preferred_token() {
    let s = setup();
    s.whitelist(false, None);
    let pid = s.completed_payment(500);
    s.request_in(pid, 500, &s.token_b);
}

#[test]
#[should_panic(expected = "SwapPairNotAllowed")]
fn test_request_rejects_disallowed_swap_pair() {
    let s = setup();
    s.whitelist(true, None);
    let pid = s.completed_payment(500);
    s.request_in(pid, 500, &s.token_b);
}

#[test]
#[should_panic(expected = "SwapRouterNotConfigured")]
fn test_request_rejects_when_no_router_configured() {
    let s = setup_with(0, false);
    let pid = s.completed_payment(500);
    s.request_in(pid, 500, &s.token_b);
}

#[test]
fn test_rejected_request_moves_no_funds() {
    let s = setup_with(0, false);
    let pid = s.completed_payment(500);
    let a_before = s.balance(&s.token_a, &s.customer);
    let res =
        s.refund
            .try_request_refund_in_token(&s.customer, &pid, &500, &s.reason(), &0, &s.token_b);
    assert!(res.is_err());
    assert_eq!(s.balance(&s.token_a, &s.customer), a_before);
    assert_eq!(s.refund.get_refund_counter(), 0);
}

// ---------------------------------------------------------------------------
// Processing: fallback to the original token
// ---------------------------------------------------------------------------

#[test]
fn test_fallback_when_quote_fails() {
    let s = setup();
    s.router.configure(&20_000, &20_000, &true, &false, &false);
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
}

#[test]
fn test_fallback_when_swap_reverts() {
    let s = setup();
    s.router.configure(&20_000, &20_000, &false, &true, &false);
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
    // The reverted swap's transfer_from was rolled back.
    assert_eq!(s.balance(&s.token_a, &s.router.address), 0);
}

#[test]
fn test_fallback_when_slippage_exceeds_tolerance() {
    let s = setup();
    // Executes 5% below the quote; default tolerance is 1%.
    s.router.configure(&20_000, &19_000, &false, &false, &false);
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
}

#[test]
fn test_admin_slippage_setting_controls_guard() {
    let s = setup();
    // Executes 5% below the quote: fails at the 1% default, passes at 6%.
    s.router.configure(&20_000, &19_000, &false, &false, &false);
    s.refund.set_refund_swap_max_slippage_bps(&s.admin, &600);
    assert_eq!(s.refund.get_refund_swap_max_slippage_bps(), 600);
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    s.approve_and_process(rid);

    assert_eq!(s.balance(&s.token_b, &s.customer), 950);
}

#[test]
fn test_whitelist_pair_slippage_cap_tightens_global_tolerance() {
    let s = setup();
    // Executes 0.6% below the quote: within the 1% global tolerance but
    // outside the pair's 0.5% cap.
    s.router.configure(&20_000, &19_880, &false, &false, &false);
    s.whitelist(true, Some(50));
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
}

#[test]
fn test_fallback_when_router_cleared_after_request() {
    let s = setup();
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    s.refund.set_swap_router(&s.admin, &None);
    assert_eq!(s.refund.get_swap_router(), None);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
}

#[test]
fn test_fallback_when_preferred_token_delisted_after_request() {
    let s = setup();
    let wl = s.whitelist(true, Some(100));
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    wl.remove_token(&s.admin, &s.token_b);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
}

#[test]
fn test_fallback_when_swap_pair_disallowed_after_request() {
    let s = setup();
    let wl = s.whitelist(true, Some(100));
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    wl.disallow_swap_pair(&s.admin, &s.token_a, &s.token_b);

    let a_before = s.balance(&s.token_a, &s.customer);
    s.approve_and_process(rid);
    s.assert_fell_back(rid, 500, a_before);
}

#[test]
fn test_router_under_delivery_aborts_processing() {
    let s = setup();
    // Broken router: ignores min_amount_out and delivers 50% of the quote.
    s.router.configure(&20_000, &10_000, &false, &false, &true);
    let pid = s.completed_payment(500);
    let rid = s.request_in(pid, 500, &s.token_b);
    s.refund.approve_refund(&s.admin, &rid);

    let a_before = s.balance(&s.token_a, &s.customer);
    let res = s.refund.try_process_refund(&s.admin, &rid);
    assert!(res.is_err());

    // Whole transaction reverted: still Approved, funds still escrowed.
    assert_eq!(s.refund.get_refund(&rid).status, RefundStatus::Approved);
    assert_eq!(s.balance(&s.token_a, &s.customer), a_before);
    assert_eq!(s.balance(&s.token_b, &s.customer), 0);
    assert_eq!(s.balance(&s.token_a, &s.refund.address), 500);
    assert_eq!(s.refund.get_refund_payout(&rid), None);

    // Admin clears the broken router; the refund then falls back.
    s.refund.set_swap_router(&s.admin, &None);
    s.refund.process_refund(&s.admin, &rid);
    s.assert_fell_back(rid, 500, a_before);
}

// ---------------------------------------------------------------------------
// Admin configuration
// ---------------------------------------------------------------------------

#[test]
fn test_swap_config_getters_and_defaults() {
    let s = setup_with(0, false);
    assert_eq!(s.refund.get_swap_router(), None);
    assert_eq!(s.refund.get_refund_swap_max_slippage_bps(), 100);

    s.refund
        .set_swap_router(&s.admin, &Some(s.router.address.clone()));
    assert_eq!(s.refund.get_swap_router(), Some(s.router.address.clone()));
    s.refund.set_refund_swap_max_slippage_bps(&s.admin, &0);
    assert_eq!(s.refund.get_refund_swap_max_slippage_bps(), 0);
}

#[test]
#[should_panic(expected = "Only admin")]
fn test_set_swap_router_requires_admin() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.refund
        .set_swap_router(&stranger, &Some(s.router.address.clone()));
}

#[test]
#[should_panic(expected = "InvalidSwapRouter")]
fn test_set_swap_router_rejects_self() {
    let s = setup();
    s.refund
        .set_swap_router(&s.admin, &Some(s.refund.address.clone()));
}

#[test]
#[should_panic(expected = "Only admin")]
fn test_set_slippage_requires_admin() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.refund.set_refund_swap_max_slippage_bps(&stranger, &50);
}

#[test]
#[should_panic(expected = "InvalidMaxSlippageBps")]
fn test_set_slippage_rejects_full_range() {
    let s = setup();
    s.refund.set_refund_swap_max_slippage_bps(&s.admin, &10_000);
}
