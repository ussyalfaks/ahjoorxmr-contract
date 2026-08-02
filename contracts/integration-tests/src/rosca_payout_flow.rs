use soroban_sdk::{
    testutils::{Address as _, Ledger, LedgerInfo},
    token, Address, Env, Vec,
};

use ahjoor_rosca::{AhjoorContract, AhjoorContractClient, RoscaConfig, PayoutStrategy, VotingMode};

/// Shared test harness: deploys the ROSCA contract plus a mock token,
/// and wires up funded member accounts so each test starts from a clean,
/// fully-funded group ready to contribute.
struct TestEnvironment<'a> {
    env: Env,
    rosca: AhjoorContractClient<'a>,
    token_client: token::Client<'a>,
    token_admin: token::StellarAssetClient<'a>,
    admin: Address,
    members: Vec<Address>,
}

impl<'a> TestEnvironment<'a> {
    /// Deploys the ROSCA contract and a Stellar Asset Contract token,
    /// mints starting balances to `member_count` member accounts.
    fn setup(member_count: u32, contribution_amount: i128, round_duration: u64) -> Self {
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

        // Deploy a Stellar Asset Contract to use as the base token.
        let token_admin_addr = Address::generate(&env);
        let token_contract_id = env.register_stellar_asset_contract_v2(token_admin_addr.clone());
        let token_client = token::Client::new(&env, &token_contract_id.address());
        let token_admin = token::StellarAssetClient::new(&env, &token_contract_id.address());

        // Deploy the ROSCA contract.
        let rosca_id = env.register(AhjoorContract, ());
        let rosca = AhjoorContractClient::new(&env, &rosca_id);

        // Generate and fund member accounts well above what they'll owe,
        // so insufficient-balance failures never mask the behavior under test.
        let mut members = Vec::new(&env);
        for _ in 0..member_count {
            let m = Address::generate(&env);
            token_admin.mint(&m, &(contribution_amount * 10));
            members.push_back(m);
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
            round_duration_seconds: round_duration,
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
            &contribution_amount,
            &token_contract_id.address(),
            &round_duration,
            &config,
            &None,
        );

        TestEnvironment {
            env,
            rosca,
            token_client,
            token_admin,
            admin,
            members,
        }
    }

    /// Has every member contribute their full required amount for the current round.
    fn all_members_contribute(&self, amount: i128) {
        for m in self.members.iter() {
            self.rosca.contribute(&m, &self.token_client.address, &amount);
        }
    }

    fn advance_past_deadline(&self, seconds: u64) {
        self.env.ledger().with_mut(|li| {
            li.timestamp += seconds + 1;
        });
    }
}

/// End-to-end: initialize a ROSCA group, have every member contribute their
/// full share, and verify the round auto-completes a payout — the recipient's
/// on-chain token balance increases by the expected pot, PaidMembers resets,
/// and CurrentRound advances. This exercises the full path from public
/// `contribute()` calls through to `internals::complete_round_payout`
/// without reaching into private contract internals.
#[test]
fn test_rosca_round_completes_payout_on_full_contribution() {
    const CONTRIBUTION: i128 = 1_000;
    const MEMBER_COUNT: u32 = 4;
    const ROUND_DURATION: u64 = 86_400;

    let t = TestEnvironment::setup(MEMBER_COUNT, CONTRIBUTION, ROUND_DURATION);

    // Determine the expected first-round recipient before payout triggers,
    // since the round counter will have advanced afterward.
    let (round_before, _, _, _, _) = t.rosca.get_state();
    assert_eq!(round_before, 0);

    let expected_recipient = t.members.get(0).unwrap();
    let recipient_balance_before = t.token_client.balance(&expected_recipient);

    t.all_members_contribute(CONTRIBUTION);

    // All four members contributing the full amount should auto-trigger
    // complete_round_payout inside the final contribute() call.
    let (round_after, paid_members_after, _, _, _) = t.rosca.get_state();
    assert_eq!(round_after, 1, "round should have advanced after full contribution");
    assert_eq!(
        paid_members_after.len(),
        0,
        "PaidMembers should reset for the new round"
    );

    let recipient_balance_after = t.token_client.balance(&expected_recipient);
    let expected_pot = CONTRIBUTION * (MEMBER_COUNT as i128);
    // The recipient is also a contributing member this round, so their net
    // gain is the pot minus the contribution they themselves paid in.
    let expected_net_gain = expected_pot - CONTRIBUTION;
    assert_eq!(
        recipient_balance_after - recipient_balance_before,
        expected_net_gain,
        "recipient should net the full pot minus their own contribution (no fee configured)"
    );

    // Round history should record exactly one payout for round 0.
    let history = t.rosca.get_round_history(&0u32, &0u32, &100u32);
    assert_eq!(history.len(), 1);
    let record = history.get(0).unwrap();
    assert_eq!(record.recipient, expected_recipient);
    assert_eq!(record.amount, expected_pot);
}

/// End-to-end: a member who never contributes is identified as a defaulter
/// once the deadline has passed and admin calls finalize_round — verifying
/// state propagates correctly across contribute() → finalize_round() →
/// payout/default bookkeeping, with a partial pot still triggering payout.
#[test]
fn test_rosca_finalize_round_flags_defaulter_and_pays_partial_pot() {
    const CONTRIBUTION: i128 = 1_000;
    const MEMBER_COUNT: u32 = 3;
    const ROUND_DURATION: u64 = 86_400;

    let t = TestEnvironment::setup(MEMBER_COUNT, CONTRIBUTION, ROUND_DURATION);

    // Only 2 of 3 members contribute; the third defaults.
    let paying_members: std::vec::Vec<Address> =
        std::vec![t.members.get(0).unwrap(), t.members.get(1).unwrap()];
    let defaulter = t.members.get(2).unwrap();

    for m in paying_members.iter() {
        t.rosca.contribute(m, &t.token_client.address, &CONTRIBUTION);
    }

    t.advance_past_deadline(ROUND_DURATION);
    t.rosca.finalize_round();

    let status = t.rosca.get_member_status(&defaulter);
    assert!(!status.has_paid_this_round || status.default_count >= 1);
    assert_eq!(status.default_count, 1, "defaulter's default_count should increment");

    // finalize_round pays out before applying new suspensions, so round
    // history should still show a completed payout for round 0 even though
    // contributions were partial.
    let history = t.rosca.get_round_history(&0u32, &0u32, &100u32);
    assert_eq!(history.len(), 1);

    let (round_after, _, _, _, _) = t.rosca.get_state();
    assert_eq!(round_after, 1, "round should advance after finalize_round");
}