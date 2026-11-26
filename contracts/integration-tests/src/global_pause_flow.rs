//! Global pause: one `pause_all` on the registry must block state changes in
//! every contract that is pointed at it, and `resume_all` must restore them.

use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, vec, Address, Env, String, Vec,
};

use ahjoor_escrow::{AhjoorEscrowContract, AhjoorEscrowContractClient};
use ahjoor_pause_registry::{PauseRegistryContract, PauseRegistryContractClient};
use ahjoor_payments::{AhjoorPaymentsContract, AhjoorPaymentsContractClient};
use ahjoor_refund::{AhjoorRefundContract, AhjoorRefundContractClient};
use ahjoor_rosca::{AhjoorContract, AhjoorContractClient, PayoutStrategy, RoscaConfig, VotingMode};

const CONTRIBUTION: i128 = 100;
const PAYMENT: i128 = 1_000;

struct TestEnvironment<'a> {
    env: Env,
    registry: PauseRegistryContractClient<'a>,
    guardians: Vec<Address>,
    escrow: AhjoorEscrowContractClient<'a>,
    payments: AhjoorPaymentsContractClient<'a>,
    refund: AhjoorRefundContractClient<'a>,
    rosca: AhjoorContractClient<'a>,
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

        let token = env
            .register_stellar_asset_contract_v2(Address::generate(&env))
            .address();
        let token_admin = token::StellarAssetClient::new(&env, &token);

        let mut members = Vec::new(&env);
        for _ in 0..3 {
            let m = Address::generate(&env);
            token_admin.mint(&m, &(PAYMENT * 10));
            members.push_back(m);
        }
        let user = members.get(0).unwrap();

        // --- pause registry: 2-of-3 guardian multisig -------------------------
        let registry_id = env.register(PauseRegistryContract, ());
        let registry = PauseRegistryContractClient::new(&env, &registry_id);
        let guardians = vec![
            &env,
            Address::generate(&env),
            Address::generate(&env),
            Address::generate(&env),
        ];
        registry.initialize(&guardians, &2);

        // --- escrow -------------------------------------------------------------
        let escrow_id = env.register(AhjoorEscrowContract, ());
        let escrow = AhjoorEscrowContractClient::new(&env, &escrow_id);
        escrow.initialize(&admin);
        escrow.add_allowed_token(&admin, &token);
        escrow.set_pause_registry(&admin, &Some(registry_id.clone()));

        // --- payments + refund --------------------------------------------------
        let payments_id = env.register(AhjoorPaymentsContract, ());
        let payments = AhjoorPaymentsContractClient::new(&env, &payments_id);
        payments.initialize(&admin, &admin, &0u32);
        payments.set_pause_registry(&admin, &Some(registry_id.clone()));

        let refund_id = env.register(AhjoorRefundContract, ());
        let refund = AhjoorRefundContractClient::new(&env, &refund_id);
        refund.initialize(&admin, &payments_id, &86_400u64, &None);
        refund.set_pause_registry(&admin, &Some(registry_id.clone()));

        // --- ROSCA --------------------------------------------------------------
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
        rosca.set_pause_registry(&admin, &Some(registry_id));

        TestEnvironment {
            env,
            registry,
            guardians,
            escrow,
            payments,
            refund,
            rosca,
            token,
            admin,
            user,
            merchant,
        }
    }

    fn quorum(&self) -> Vec<Address> {
        vec![
            &self.env,
            self.guardians.get(0).unwrap(),
            self.guardians.get(1).unwrap(),
        ]
    }

    fn pause_all(&self) {
        self.registry
            .pause_all(&self.quorum(), &String::from_str(&self.env, "incident"));
    }

    fn completed_payment(&self) -> u32 {
        let pid = self.payments.create_payment(
            &self.user, &self.merchant, &PAYMENT, &self.token, &None, &None, &None,
        );
        self.payments.complete_payment(&pid);
        pid
    }

    fn try_create_escrow(&self) -> bool {
        let deadline = self.env.ledger().timestamp() + 1_000;
        self.escrow
            .try_create_escrow(
                &self.user,
                &self.merchant,
                &Address::generate(&self.env),
                &PAYMENT,
                &self.token,
                &deadline,
                &None,
                &Vec::new(&self.env),
                &false,
                &0u32,
            )
            .is_ok()
    }

    fn try_create_payment(&self) -> bool {
        self.payments
            .try_create_payment(
                &self.user, &self.merchant, &PAYMENT, &self.token, &None, &None, &None,
            )
            .is_ok()
    }

    fn try_request_refund(&self, pid: u32) -> bool {
        self.refund
            .try_request_refund(
                &self.user,
                &pid,
                &PAYMENT,
                &String::from_str(&self.env, "not as described"),
                &0u32,
            )
            .is_ok()
    }

    fn try_contribute(&self) -> bool {
        self.rosca
            .try_contribute(&self.user, &self.token, &CONTRIBUTION)
            .is_ok()
    }
}

/// Pausing the registry blocks a state-changing call in every contract, with
/// no contract's local pause flag touched.
#[test]
fn test_registry_pause_blocks_every_contract() {
    let t = TestEnvironment::setup();
    // Created before the incident so the refund path has something to act on.
    let pid = t.completed_payment();

    t.pause_all();
    assert!(t.registry.is_paused());

    assert!(!t.try_create_escrow(), "escrow must be blocked");
    assert!(!t.try_create_payment(), "payments must be blocked");
    assert!(!t.try_request_refund(pid), "refund must be blocked");
    assert!(!t.try_contribute(), "rosca must be blocked");

    // Local pause flags stay untouched: only the registry is paused.
    assert!(!t.escrow.is_paused());
    assert!(!t.payments.is_paused());
    assert!(!t.refund.is_paused());
    assert!(!t.rosca.is_paused());
}

/// `resume_all` lifts the block everywhere.
#[test]
fn test_registry_resume_restores_every_contract() {
    let t = TestEnvironment::setup();
    let pid = t.completed_payment();

    t.pause_all();
    t.registry.resume_all(&t.quorum());
    assert!(!t.registry.is_paused());

    assert!(t.try_create_escrow());
    assert!(t.try_create_payment());
    assert!(t.try_request_refund(pid));
    assert!(t.try_contribute());
}

/// A single guardian below the 2-of-3 threshold cannot pause the protocol.
#[test]
fn test_single_guardian_cannot_pause_all() {
    let t = TestEnvironment::setup();
    let one = vec![&t.env, t.guardians.get(0).unwrap()];
    assert!(t
        .registry
        .try_pause_all(&one, &String::from_str(&t.env, "rogue"))
        .is_err());
    assert!(!t.registry.is_paused());
    assert!(t.try_create_payment());
}

/// A contract that is not pointed at the registry keeps working.
#[test]
fn test_unlinked_contract_ignores_registry() {
    let t = TestEnvironment::setup();
    t.payments.set_pause_registry(&t.admin, &None);
    t.pause_all();
    assert!(t.try_create_payment());
    assert!(!t.try_contribute());
}
