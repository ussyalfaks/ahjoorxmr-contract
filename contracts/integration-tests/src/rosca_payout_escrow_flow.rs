use ahjoor_escrow::{AhjoorEscrowContract, AhjoorEscrowContractClient};
use ahjoor_rosca::{AhjoorContract, AhjoorContractClient, PayoutStrategy, RoscaConfig, VotingMode};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger, LedgerInfo},
    token, Address, Env, IntoVal, String, Symbol, Vec,
};

struct TestEnvironment<'a> {
    env: Env,
    rosca: AhjoorContractClient<'a>,
    escrow: AhjoorEscrowContractClient<'a>,
    token_client: token::Client<'a>,
    members: Vec<Address>,
    admin: Address,
    seller: Address,
    arbiter: Address,
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
        let seller = Address::generate(&env);
        let arbiter = Address::generate(&env);

        let token_admin_addr = Address::generate(&env);
        let token_contract_id = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
        let token_client = token::Client::new(&env, &token_contract_id.address());
        let token_admin = token::StellarAssetClient::new(&env, &token_contract_id.address());

        let rosca_id = env.register(AhjoorContract, ());
        let rosca = AhjoorContractClient::new(&env, &rosca_id);

        let escrow_id = env.register(AhjoorEscrowContract, ());
        let escrow = AhjoorEscrowContractClient::new(&env, &escrow_id);
        escrow.initialize(&admin);

        let mut members = Vec::new(&env);
        for _ in 0..2 {
            let member = Address::generate(&env);
            token_admin.mint(&member, &1_000);
            members.push_back(member);
        }

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

        rosca.init(
            &admin,
            &members,
            &100,
            &token_contract_id.address(),
            &3_600,
            &config,
            &None,
        );
        rosca.set_escrow_contract(&admin, &escrow_id);

        TestEnvironment {
            env,
            rosca,
            escrow,
            token_client,
            members,
            admin,
            seller,
            arbiter,
        }
    }

    fn contribute_round(&self) {
        for member in self.members.iter() {
            self.rosca
                .contribute(&member, &self.token_client.address, &100);
        }
    }
}

fn has_event(env: &Env, topic: &str) -> bool {
    let expected = (Symbol::new(env, topic),).into_val(env);
    env.events().all().iter().any(|event| event.1 == expected)
}

#[test]
fn test_payout_routed_into_escrow() {
    let s = TestEnvironment::setup();
    let member2 = s.members.get(1).unwrap();
    let deadline = s.env.ledger().timestamp() + 100_000;

    s.rosca
        .set_payout_escrow_target(&0, &member2, &s.seller, &s.arbiter, &deadline);
    assert!(s.rosca.get_payout_escrow_target(&member2).is_some());

    // Round 0 pays member1 directly.
    s.contribute_round();

    s.env.ledger().with_mut(|li| li.sequence_number += 1);

    // Round 1 pays member2; the payout is routed into escrow.
    s.contribute_round();

    assert!(has_event(&s.env, "payout_escrow_created"));
    assert!(s.rosca.get_payout_escrow_target(&member2).is_none());

    let escrow = s.escrow.get_escrow(&0);
    assert_eq!(escrow.buyer, member2);
    assert_eq!(escrow.seller, s.seller);
    assert_eq!(escrow.arbiter, s.arbiter);
    assert_eq!(escrow.amount, 200);
    assert_eq!(escrow.token, s.token_client.address);

    assert_eq!(s.token_client.balance(&s.escrow.address), 200);
    assert_eq!(s.token_client.balance(&member2), 800);
}

#[test]
fn test_payout_falls_back_to_direct_transfer_when_escrow_fails() {
    let s = TestEnvironment::setup();
    let member2 = s.members.get(1).unwrap();
    let deadline = s.env.ledger().timestamp() + 100_000;

    s.rosca
        .set_payout_escrow_target(&0, &member2, &s.seller, &s.arbiter, &deadline);
    s.escrow
        .pause_contract(&s.admin, &String::from_str(&s.env, "maintenance"));

    // Round 0 pays member1 directly.
    s.contribute_round();

    s.env.ledger().with_mut(|li| li.sequence_number += 1);

    // Round 1 tries escrow, fails because escrow is paused, and pays directly.
    s.contribute_round();

    assert!(has_event(&s.env, "payout_escrow_fallback"));
    assert!(s.rosca.get_payout_escrow_target(&member2).is_none());

    // Contributions total 200, then the 200 payout is sent directly.
    assert_eq!(s.token_client.balance(&member2), 1_000);
    assert_eq!(s.token_client.balance(&s.escrow.address), 0);
}
