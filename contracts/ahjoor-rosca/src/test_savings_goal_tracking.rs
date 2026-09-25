#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    Address, Env, Map, String, Symbol, Vec,
};

// ---------------------------------------------------------------------------
//  Helpers
// ---------------------------------------------------------------------------

struct Setup<'a> {
    env: Env,
    client: AhjoorContractClient<'a>,
    admin: Address,
    token_admin: Address,
    token_addr: Address,
    token_client: TokenClient<'a>,
    token_admin_client: TokenAdminClient<'a>,
    members: Vec<Address>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_admin);
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let mut members = Vec::new(&env);
    for _ in 0..3 {
        let addr = Address::generate(&env);
        token_admin_client.mint(&addr, &10_000);
        members.push_back(addr);
    }

    // Initialize ROSCA
    client.init(
        &admin,
        &members,
        &100,
        &token_admin,
        &3600,
        &RoscaConfig {
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
            round_duration_seconds: 0,
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
        },
        &None,
    );

    Setup {
        env,
        client,
        admin,
        token_admin,
        token_addr: token_admin,
        token_client,
        token_admin_client,
        members,
    }
}

// ---------------------------------------------------------------------------
//  Tests
// ---------------------------------------------------------------------------

/// Test create_goal creates a new savings goal with correct initial state.
#[test]
fn test_create_goal() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let goal_name = String::from_str(&s.env, "Emergency Fund");
    let goal_desc = String::from_str(&s.env, "Save for emergencies");
    let target_amount = 1000i128;
    let target_date = s.env.ledger().timestamp() + 86400 * 30; // 30 days from now

    let goal_id = s.client.create_goal(
        &member,
        &0, // group_id
        &goal_name,
        &goal_desc,
        &target_amount,
        &s.token_addr,
        &target_date,
        &1, // priority
        &String::from_str(&s.env, "emergency"),
        &Map::new(&s.env),
    );

    assert_eq!(goal_id, 1);

    // Verify goal was created with correct initial state
    let member_goals = s.client.get_member_goals(&member);
    assert_eq!(member_goals.len(), 1);

    let goal = member_goals.get(0).unwrap();
    assert_eq!(goal.goal_id, 1);
    assert_eq!(goal.member, member);
    assert_eq!(goal.target_amount, target_amount);
    assert_eq!(goal.current_amount, 0);
    assert_eq!(goal.status, GoalStatus::Active);
    assert_eq!(goal.name, goal_name);
    assert_eq!(goal.description, goal_desc);
}

/// Test that create_goal panics with invalid (non-positive) target amount.
#[test]
#[should_panic(expected = "InvalidGoalAmount")]
fn test_create_goal_invalid_amount() {
    let s = setup();
    let member = s.members.get(0).unwrap();

    s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Test"),
        &String::from_str(&s.env, "Test"),
        &0, // Invalid
        &s.token_addr,
        &(s.env.ledger().timestamp() + 86400),
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );
}

/// Test that create_goal panics with target_date in the past.
#[test]
#[should_panic(expected = "GoalExpired")]
fn test_create_goal_expired_target_date() {
    let s = setup();
    let member = s.members.get(0).unwrap();

    s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Test"),
        &String::from_str(&s.env, "Test"),
        &1000,
        &s.token_addr,
        &(s.env.ledger().timestamp() - 1), // Past
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );
}

/// Test contribute_to_goal adds amount to goal and tracks contribution.
#[test]
fn test_contribute_to_goal() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_amount = 1000i128;
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    // Create goal
    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Emergency Fund"),
        &String::from_str(&s.env, "Save for emergencies"),
        &target_amount,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "emergency"),
        &Map::new(&s.env),
    );

    // Contribute to goal
    let contribution = s.client.contribute_to_goal(
        &goal_id,
        &member,
        &250,
        &String::from_str(&s.env, "manual_deposit"),
    );

    assert_eq!(contribution.amount, 250);

    // Verify progress
    let progress = s.client.get_goal_progress(&goal_id);
    assert_eq!(progress.current_amount, 250);
    assert_eq!(progress.target_amount, 1000);
    assert_eq!(progress.percentage_complete, 25);
}

/// Test multiple members can contribute to the same goal.
#[test]
fn test_multiple_members_contribute_to_goal() {
    let s = setup();
    let member1 = s.members.get(0).unwrap();
    let member2 = s.members.get(1).unwrap();
    let target_amount = 1000i128;
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    // Member 1 creates goal
    let goal_id = s.client.create_goal(
        &member1,
        &0,
        &String::from_str(&s.env, "Group Goal"),
        &String::from_str(&s.env, "Saving together"),
        &target_amount,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "group"),
        &Map::new(&s.env),
    );

    // Both members contribute
    s.client.contribute_to_goal(
        &goal_id,
        &member1,
        &300,
        &String::from_str(&s.env, "manual_deposit"),
    );

    s.client.contribute_to_goal(
        &goal_id,
        &member2,
        &200,
        &String::from_str(&s.env, "manual_deposit"),
    );

    // Verify combined progress
    let progress = s.client.get_goal_progress(&goal_id);
    assert_eq!(progress.current_amount, 500);
    assert_eq!(progress.percentage_complete, 50);
}

/// Test get_goal_progress returns correct percentage.
#[test]
fn test_get_goal_progress() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_amount = 1000i128;
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Test Goal"),
        &String::from_str(&s.env, "Testing"),
        &target_amount,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    // Initial state
    let progress = s.client.get_goal_progress(&goal_id);
    assert_eq!(progress.current_amount, 0);
    assert_eq!(progress.target_amount, 1000);
    assert_eq!(progress.percentage_complete, 0);

    // After partial contribution
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &333,
        &String::from_str(&s.env, "manual_deposit"),
    );

    let progress = s.client.get_goal_progress(&goal_id);
    assert_eq!(progress.current_amount, 333);
    assert_eq!(progress.percentage_complete, 33);

    // After reaching target
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &667,
        &String::from_str(&s.env, "manual_deposit"),
    );

    let progress = s.client.get_goal_progress(&goal_id);
    assert_eq!(progress.current_amount, 1000);
    assert_eq!(progress.percentage_complete, 100);
}

/// Test complete_goal marks goal as completed and emits celebration.
#[test]
fn test_complete_goal() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Complete Me"),
        &String::from_str(&s.env, "Will complete"),
        &500,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    // First fully fund the goal
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &500,
        &String::from_str(&s.env, "manual_deposit"),
    );

    // Complete the goal
    let celebration = s.client.complete_goal(&goal_id);

    assert_eq!(celebration.goal_id, goal_id);
    assert_eq!(celebration.celebration_type, CelebrationType::GoalCompleted);

    // Verify goal status is Completed
    let member_goals = s.client.get_member_goals(&member);
    let goal = member_goals.get(0).unwrap();
    assert_eq!(goal.status, GoalStatus::Completed);
}

/// Test that complete_goal panics if goal already completed.
#[test]
#[should_panic(expected = "GoalCompleted")]
fn test_complete_goal_already_completed() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Test"),
        &String::from_str(&s.env, "Test"),
        &100,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    s.client.contribute_to_goal(&goal_id, &member, &100, &String::from_str(&s.env, "deposit"));

    // Complete once
    s.client.complete_goal(&goal_id);

    // Try to complete again - should panic
    s.client.complete_goal(&goal_id);
}

/// Test abandon_goal stops further contributions and marks goal as abandoned.
#[test]
fn test_abandon_goal() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Abandon Me"),
        &String::from_str(&s.env, "Will abandon"),
        &500,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    // Partial contribution before abandoning
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &100,
        &String::from_str(&s.env, "manual_deposit"),
    );

    // Abandon the goal
    s.client.abandon_goal(&goal_id);

    // Verify goal status is Abandoned
    let member_goals = s.client.get_member_goals(&member);
    let goal = member_goals.get(0).unwrap();
    assert_eq!(goal.status, GoalStatus::Abandoned);
}

/// Test that contribute_to_goal panics after goal is abandoned.
#[test]
#[should_panic(expected = "GoalAbandoned")]
fn test_contribute_after_abandon_fails() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Test"),
        &String::from_str(&s.env, "Test"),
        &500,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    s.client.abandon_goal(&goal_id);

    // Try to contribute after abandon - should panic
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &100,
        &String::from_str(&s.env, "manual_deposit"),
    );
}

/// Test that abandon_goal panics for non-existent goal.
#[test]
#[should_panic(expected = "GoalNotFound")]
fn test_abandon_nonexistent_goal_fails() {
    let s = setup();
    let member = s.members.get(0).unwrap();

    s.client.abandon_goal(&999); // Non-existent goal
}

/// Test member can have multiple goals.
#[test]
fn test_member_multiple_goals() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    // Create multiple goals
    let goal_id_1 = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Goal 1"),
        &String::from_str(&s.env, "First goal"),
        &500,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    let goal_id_2 = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Goal 2"),
        &String::from_str(&s.env, "Second goal"),
        &1000,
        &s.token_addr,
        &target_date,
        &2,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    assert_ne!(goal_id_1, goal_id_2);

    // Verify both goals exist for member
    let member_goals = s.client.get_member_goals(&member);
    assert_eq!(member_goals.len(), 2);
}

/// Test contribution amount cannot exceed remaining goal amount.
#[test]
#[should_panic(expected = "InvalidContribution")]
fn test_contribution_exceeding_target_fails() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Test"),
        &String::from_str(&s.env, "Test"),
        &100,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    // Try to contribute more than target
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &200, // Exceeds target of 100
        &String::from_str(&s.env, "manual_deposit"),
    );
}

/// Test get_group_goals_summary returns correct aggregate data.
#[test]
fn test_get_group_goals_summary() {
    let s = setup();
    let member1 = s.members.get(0).unwrap();
    let member2 = s.members.get(1).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    // Member 1 creates goal with 500 target
    let goal_id_1 = s.client.create_goal(
        &member1,
        &0,
        &String::from_str(&s.env, "Goal 1"),
        &String::from_str(&s.env, "First"),
        &500,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    s.client.contribute_to_goal(&goal_id_1, &member1, &300, &String::from_str(&s.env, "dep"));

    // Member 2 creates goal with 300 target
    let goal_id_2 = s.client.create_goal(
        &member2,
        &0,
        &String::from_str(&s.env, "Goal 2"),
        &String::from_str(&s.env, "Second"),
        &300,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    s.client.contribute_to_goal(&goal_id_2, &member2, &300, &String::from_str(&s.env, "dep"));

    // Get group summary
    let summary = s.client.get_group_goals_summary(&0);

    assert_eq!(summary.total_goals, 2);
    assert_eq!(summary.active_goals, 2);
    assert_eq!(summary.total_saved, 600);
    assert_eq!(summary.total_target, 800);
}

/// Test pause and resume goal flow.
#[test]
fn test_pause_and_resume_goal() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Test"),
        &String::from_str(&s.env, "Test"),
        &500,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    // Pause goal
    s.client.pause_goal(&goal_id);

    let member_goals = s.client.get_member_goals(&member);
    let goal = member_goals.get(0).unwrap();
    assert_eq!(goal.status, GoalStatus::Paused);

    // Resume goal
    s.client.resume_goal(&goal_id);

    let member_goals = s.client.get_member_goals(&member);
    let goal = member_goals.get(0).unwrap();
    assert_eq!(goal.status, GoalStatus::Active);
}

/// Test full lifecycle: create -> contribute -> complete.
#[test]
fn test_full_lifecycle_create_contribute_complete() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    // 1. Create goal
    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Vacation Fund"),
        &String::from_str(&s.env, "Save for summer trip"),
        &2000,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "vacation"),
        &Map::new(&s.env),
    );

    // 2. Verify initial state
    let progress = s.client.get_goal_progress(&goal_id);
    assert_eq!(progress.current_amount, 0);
    assert_eq!(progress.percentage_complete, 0);

    // 3. Make contributions from multiple sources
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &500,
        &String::from_str(&s.env, "round_payout"),
    );

    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &300,
        &String::from_str(&s.env, "manual_deposit"),
    );

    // 4. Check progress
    let progress = s.client.get_goal_progress(&goal_id);
    assert_eq!(progress.current_amount, 800);
    assert_eq!(progress.percentage_complete, 40);

    // 5. Complete the goal
    s.client.contribute_to_goal(
        &goal_id,
        &member,
        &1200,
        &String::from_str(&s.env, "bonus"),
    );

    let celebration = s.client.complete_goal(&goal_id);

    // 6. Verify completion
    assert_eq!(celebration.celebration_type, CelebrationType::GoalCompleted);

    let member_goals = s.client.get_member_goals(&member);
    let goal = member_goals.get(0).unwrap();
    assert_eq!(goal.status, GoalStatus::Completed);
    assert_eq!(goal.current_amount, 2000);
}

/// Test goal with milestones creates celebrations at threshold.
#[test]
fn test_goal_with_milestones() {
    let s = setup();
    let member = s.members.get(0).unwrap();
    let target_date = s.env.ledger().timestamp() + 86400 * 30;

    let goal_id = s.client.create_goal(
        &member,
        &0,
        &String::from_str(&s.env, "Milestone Goal"),
        &String::from_str(&s.env, "Test milestones"),
        &1000,
        &s.token_addr,
        &target_date,
        &1,
        &String::from_str(&s.env, "test"),
        &Map::new(&s.env),
    );

    // Add milestones via internal function
    let milestones = Vec::new(&s.env);
    s.client.add_milestones(
        &goal_id,
        &vec![
            &s.env,
            Milestone {
                milestone_id: 1,
                percentage: 25,
                amount: 250,
                name: String::from_str(&s.env, "Quarter Way"),
                description: String::from_str(&s.env, "25% complete"),
                reward_type: RewardType::Badge,
                reward_value: 1,
                celebration_event: String::from_str(&s.env, "badge_earned"),
                reward_bps: 0,
            },
            Milestone {
                milestone_id: 2,
                percentage: 50,
                amount: 500,
                name: String::from_str(&s.env, "Halfway There"),
                description: String::from_str(&s.env, "50% complete"),
                reward_type: RewardType::Bonus,
                reward_value: 10,
                celebration_event: String::from_str(&s.env, "bonus_tokens"),
                reward_bps: 0,
            },
        ],
    );

    // Contribute to trigger milestones
    s.client.contribute_to_goal(&goal_id, &member, &250, &String::from_str(&s.env, "deposit"));
    // Should trigger first milestone celebration

    s.client.contribute_to_goal(&goal_id, &member, &250, &String::from_str(&s.env, "deposit"));
    // Should trigger second milestone celebration

    let member_goals = s.client.get_member_goals(&member);
    let goal = member_goals.get(0).unwrap();
    assert_eq!(goal.completed_milestones.len(), 2);
}