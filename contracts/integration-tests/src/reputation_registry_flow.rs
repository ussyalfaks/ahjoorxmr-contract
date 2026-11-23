use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, Address, Env, String, Vec,
};

use ahjoor_common::reputation::{ReputationRegistryContract, ReputationRegistryContractClient};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use ahjoor_refund::{AhjoorRefundContract, AhjoorRefundContractClient};
use ahjoor_rosca::{AhjoorContract, AhjoorContractClient, PayoutStrategy, RoscaConfig, VotingMode};

const CONTRIBUTION: i128 = 100;
const PAYMENT: i128 = 1_000;
const ROSCA_WEIGHT: u32 = 2;
const REFUND_WEIGHT: u32 = 1;

/// Deploys the reputation registry next to a ROSCA group and the
/// payments + refund pair, with `user` being both a ROSCA member and a
/// payments customer. Both contracts are pointed at the registry; neither is
/// allowlisted yet.
struct TestEnvironment<'a> {
    env: Env,
    registry: ReputationRegistryContractClient<'a>,
    rosca: AhjoorContractClient<'a>,
    payments: AhjoorPaymentsContractClient<'a>,
    refund: AhjoorRefundContractClient<'a>,
    token: Address,
    admin: Address,
    user: Address,
    merchant: Address,
}

impl<'a> TestEnvironment<'a> {
    fn setup() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set(LedgerInfo {
            timestamp: 1_000_000,
            protocol_version: 23,
            sequence_number: 100,
            network_id: Default::default(),
            base_reserve: 10,
            min_temp_entry_ttl: 16,
            min_persistent_entry_ttl: 16,
            max_entry_ttl: 6_312_000,
        });

        let admin = Address::generate(&env);
        let merchant = Address::generate(&env);

        let token_admin_addr = Address::generate(&env);
        let token_contract = env.register_stellar_asset_contract_v2(token_admin_addr);
        let token = token_contract.address();
        let token_admin = token::StellarAssetClient::new(&env, &token);

        let mut members = Vec::new(&env);
        for _ in 0..3 {
            let m = Address::generate(&env);
            token_admin.mint(&m, &(PAYMENT * 10));
            members.push_back(m);
        }
        let user = members.get(0).unwrap();

        // --- registry ---------------------------------------------------------
        let registry_id = env.register(ReputationRegistryContract, ());
        let registry = ReputationRegistryContractClient::new(&env, &registry_id);
        registry.initialize(&admin);

        // --- ROSCA ------------------------------------------------------------
        let rosca_id = env.register(AhjoorContract, ());
        let rosca = AhjoorContractClient::new(&env, &rosca_id);
        let config = RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 3_600,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        };
        rosca.init(&admin, &members, &CONTRIBUTION, &token, &3_600, &config, &None);
        rosca.set_reputation_registry(&admin, &Some(registry_id.clone()));

        // --- payments + refund ------------------------------------------------
        let payments_id = env.register(AhjoorPaymentsContract, ());
        let payments = AhjoorPaymentsContractClient::new(&env, &payments_id);
        payments.initialize(&admin, &admin, &0u32);

        let refund_id = env.register(AhjoorRefundContract, ());
        let refund = AhjoorRefundContractClient::new(&env, &refund_id);
        refund.initialize(&admin, &payments_id, &86_400u64, &None);
        refund.set_reputation_registry(&admin, &Some(registry_id));

        TestEnvironment {
            env,
            registry,
            rosca,
            payments,
            refund,
            token,
            admin,
            user,
            merchant,
        }
    }

    fn allowlist_both(&self) {
        self.registry
            .allow_source(&self.admin, &self.rosca.address, &ROSCA_WEIGHT);
        self.registry
            .allow_source(&self.admin, &self.refund.address, &REFUND_WEIGHT);
    }

    /// `user` contributes on time to the current ROSCA round (+10 credit).
    fn contribute_on_time(&self) {
        self.rosca.contribute(&self.user, &self.token, &CONTRIBUTION);
    }

    /// `user` pays the merchant, requests a refund and the admin rejects it
    /// (+10 abuse score).
    fn rejected_refund(&self) {
        let pid = self.payments.create_payment(
            &self.user,
            &self.merchant,
            &PAYMENT,
            &self.token,
            &None,
            &None,
            &None,
        );
        self.payments.complete_payment(&pid);
        let rid = self.refund.request_refund(
            &self.user,
            &pid,
            &PAYMENT,
            &String::from_str(&self.env, "not as described"),
            &0u32,
        );
        self.refund
            .reject_refund(&self.admin, &rid, &String::from_str(&self.env, "denied"));
    }
}

/// ROSCA and refund both report to the registry; `get_reputation` returns a
/// per-source score for each and a weighted combined score.
#[test]
fn test_rosca_and_refund_signals_combine_in_registry() {
    let t = TestEnvironment::setup();
    t.allowlist_both();

    t.contribute_on_time();
    t.rejected_refund();

    // Native scores in each contract.
    assert_eq!(t.rosca.get_credit_score(&t.user).score, 10);
    assert_eq!(t.refund.get_customer_abuse_score(&t.user).abuse_score, 10);

    let rep = t.registry.get_reputation(&t.user);
    assert_eq!(rep.user, t.user);
    assert_eq!(rep.sources.len(), 2);

    let rosca_score = rep.sources.get(0).unwrap();
    assert_eq!(rosca_score.source, t.rosca.address);
    assert_eq!(rosca_score.raw_score, 10);
    // 5_000 neutral + 10 credit points * 10.
    assert_eq!(rosca_score.score, 5_100);
    assert_eq!(rosca_score.weight, ROSCA_WEIGHT);

    let refund_score = rep.sources.get(1).unwrap();
    assert_eq!(refund_score.source, t.refund.address);
    assert_eq!(refund_score.raw_score, 10);
    // 10_000 - 10 abuse points * 100.
    assert_eq!(refund_score.score, 9_000);
    assert_eq!(refund_score.weight, REFUND_WEIGHT);

    // (5_100 * 2 + 9_000 * 1) / 3
    assert_eq!(rep.combined_score, Some(6_400));

    // The user can read their own raw signals too.
    assert_eq!(t.registry.get_signals(&t.user).len(), 2);
}

/// A contract that is not allowlisted cannot write to the registry, and its
/// rejected report does not block the ROSCA or refund operation.
#[test]
fn test_non_allowlisted_contracts_cannot_write() {
    let t = TestEnvironment::setup();

    t.contribute_on_time();
    t.rejected_refund();

    // Both operations succeeded locally...
    assert_eq!(t.rosca.get_credit_score(&t.user).score, 10);
    assert_eq!(t.refund.get_customer_abuse_score(&t.user).abuse_score, 10);
    // ...but nothing reached the registry.
    assert_eq!(t.registry.get_signals(&t.user).len(), 0);
    assert_eq!(t.registry.get_reputation(&t.user).combined_score, None);

    // Only the refund contract is allowlisted: only it can write.
    t.registry
        .allow_source(&t.admin, &t.refund.address, &REFUND_WEIGHT);
    t.refund.reset_customer_abuse_score(&t.admin, &t.user);

    let rep = t.registry.get_reputation(&t.user);
    assert_eq!(rep.sources.len(), 1);
    assert_eq!(rep.sources.get(0).unwrap().source, t.refund.address);
    assert_eq!(rep.combined_score, Some(10_000));
}

/// Removing a source from the allowlist drops it from the combined score.
#[test]
fn test_removed_source_drops_out_of_combined_score() {
    let t = TestEnvironment::setup();
    t.allowlist_both();

    t.contribute_on_time();
    t.rejected_refund();

    t.registry.remove_source(&t.admin, &t.refund.address);

    let rep = t.registry.get_reputation(&t.user);
    assert_eq!(rep.sources.len(), 1);
    assert_eq!(rep.sources.get(0).unwrap().source, t.rosca.address);
    assert_eq!(rep.combined_score, Some(5_100));
}
