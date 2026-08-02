# ROSCA Savings Goals

This document describes the per-member savings goal feature in the
`ahjoor-rosca` contract. Members can create personal savings targets within a
group, track progress, add milestones, and earn on-chain token rewards.

## Lifecycle

### 1. Create a Goal

Any group member can create a savings goal:

```
create_savings_goal(member, group_id, name, description, target_amount, token, target_date, priority, category, metadata)
```

The caller must authenticate as `member`. `target_amount` must be positive and
`target_date` must be in the future. On success, the goal is stored and indexed
by both member and group, and its numeric `goal_id` is returned. The goal is
created with `GoalStatus::Active`.

All fields except `member` and `group_id` are mutable after creation:
- `metadata` can be updated with `update_goal_metadata(goal_id, metadata)`.
- Milestones can be attached later with `add_savings_goal_milestones`.

The `token` field records which asset the goal is denominated in; the contract
does not auto-transfer tokens on contribution — it is an off-chain tracking
record.

### 2. Add Milestones (Optional)

Milestones define progress thresholds and optional token rewards:

```
add_savings_goal_milestones(goal_id, milestones)
```

Each `Milestone` specifies:
- `percentage` (1–100) — completion threshold.
- `amount` — the target amount at that threshold.
- `reward_bps` — basis points of each contribution amount awarded when the
  milestone is crossed (0 = no token reward).
- `name`, `description`, `reward_type`, `reward_value`, `celebration_event` —
  metadata for off-chain display.

The member who owns the goal must sign. Each milestone's `percentage` must be
between 1 and 100, and `amount` must be positive. Milestones are appended to the
goal's existing milestone list.

### 3. Contribute

Progress is recorded by calling:

```
contribute_to_savings_goal(goal_id, member, amount, source)
```

The caller must authenticate as `member`. `amount` must be positive and the
total contributed must not exceed `target_amount`. If the contribution pushes
`current_amount` >= `target_amount`, the goal is automatically marked
`GoalStatus::Completed`.

`source` is an arbitrary string (e.g. `"manual_deposit"`, `"round_payout"`,
`"bonus"`) recorded for audit purposes.

**Note:** This is a tracking record only — no tokens are transferred from the
member's wallet. The contract does not escrow savings goal contributions.

#### Milestone Reward Distribution

When a contribution crosses a milestone whose `reward_bps > 0`, the contract
may issue an on-chain token reward:

```
Reward = contribution_amount × reward_bps / 10,000
```

The reward is transferred from the savings reward pool (funded by the admin via
`fund_savings_reward_pool`) to the member. A `MilestoneReached` event is
emitted.

**Graceful pool depletion:** If the reward pool balance is less than the
calculated reward, no transfer occurs but the contribution succeeds and the
milestone is still marked claimed. This prevents griefing.

**One reward per milestone:** A bitmask (`DataKey3::SavingsMilestonesClaimed`)
tracks which milestones have been paid out. Once a milestone's bit is set, no
further rewards are issued for it, even if additional contributions are made.

If a single contribution crosses multiple milestones (e.g. a 60% contribution
crosses both a 25% and a 50% milestone), rewards for all newly crossed
milestones are distributed in one call.

### 4. Status Transitions

```text
Active ──→ Paused     (pause_goal)
Active ──→ Completed  (goal fully funded, or force-complete via complete_goal)
Active ──→ Abandoned  (abandon_goal)
Active ──→ Failed     (auto when target_date passes and goal is not complete)
Paused  ──→ Active    (resume_goal)
```

Only the goal owner can change the goal's status. `pause_goal` and
`resume_goal` let the member temporarily halt progress tracking. `abandon_goal`
permanently marks a goal as abandoned. `complete_goal` force-completes a goal
early and creates a `GoalCompleted` celebration.

When `contribute_to_goal` is called after `target_date`, the goal is
automatically set to `GoalStatus::Failed` and the contribution is rejected.

## Querying Progress

### Per-Goal Progress

```
get_goal_progress(goal_id)
```

Returns a `GoalProgress` struct with:

| Field | Meaning |
|---|---|
| `current_amount` | Amount saved so far. |
| `target_amount` | Goal target. |
| `percentage_completed` | `(current / target) × 100`. |
| `days_remaining` | Days until `target_date` (-1 if past). |
| `estimated_completion` | The goal's `target_date`. |
| `velocity` | `current_amount / days_elapsed` (0 on first day). |
| `status` | Current `GoalStatus`. |

### Member Goals

```
get_member_goals(member)
```

Returns all goals for a member across all groups they belong to.

### Group Summary

```
get_group_goals_summary(group_id)
```

Returns a `GroupGoalSummary` with aggregate stats for a group:

| Field | Meaning |
|---|---|
| `total_goals` | Number of goals in the group. |
| `completed_goals` | Goals in `Completed` status. |
| `active_goals` | Goals in `Active` status. |
| `total_saved` | Sum of `current_amount` across all goals. |
| `total_target` | Sum of `target_amount` across all goals. |
| `avg_completion_percentage` | Weighted average completion across all goals. |

### Goals by Category

```
get_goals_by_category(group_id, category)
```

Filters a group's goals by the `category` string set at creation time.

### Reward Pool Balance

```
get_savings_reward_pool()
```

Returns the current balance of the savings milestone reward pool.

### Milestone Claim Status

```
get_savings_milestones_claimed(goal_id, member)
```

Returns a `u64` bitmask where bit N is set if milestone with ID N has been
claimed for the given goal and member.

## Milestone Celebrations

When a milestone threshold is crossed (via `contribute_to_goal`), the
implementation calls `check_and_distribute_milestone_rewards` which handles
token rewards. Additionally, `check_and_celebrate_milestones` can be called to
create `MilestoneCelebration` records for each newly reached milestone.

Celebrations can also be created manually:

```
celebrate_milestone(goal_id, milestone_id, message)
```

The goal owner must sign. A `MilestoneCelebration` is stored with
`reward_issued: false`. The reward can later be marked as issued via
`issue_milestone_reward(celebration_id, reward_details)`.

## Achievement Badges

```
issue_achievement_badge(member, badge_type, metadata)
```

Creates an on-chain `GoalAchievementBadge` record. `BadgeType` values:

| Variant | Meaning |
|---|---|
| `GoalCompleted` | Member completed a goal. |
| `ConsecutiveContributions` | Member contributed consistently. |
| `HighVelocity` | Member saved faster than average. |
| `EarlyCompletion` | Goal completed well before target date. |
| `GroupLeader` | Top saver in the group. |
| `MilestoneChampion` | Member crossed multiple milestones. |

## Reward Pool

The savings milestone reward pool is a separate balance from the group's
collective goal reward pool (`DataKey5::GoalRewardPool`). It is funded by the
admin:

```
fund_savings_reward_pool(admin, amount)
```

Only the contract admin may call this. Tokens are transferred from `admin` to
the contract and credited to a running balance stored at `DataKey::RewardPool`.
The pool is drawn down as milestone rewards are distributed during
`contribute_to_savings_goal`.

## Error Codes

| Error | Value | Cause |
|---|---|---|
| `GoalNotFound` | 1 | The goal ID does not exist. |
| `GoalCompleted` | 2 | Goal is already completed. |
| `GoalAbandoned` | 3 | Goal has been abandoned. |
| `InvalidGoalAmount` | 4 | Target amount ≤ 0. |
| `InvalidMilestone` | 5 | Milestone validation failed. |
| `MilestoneNotFound` | 6 | Milestone ID not found on goal. |
| `UnauthorizedAccess` | 7 | Caller is not the goal owner. |
| `GoalExpired` | 8 | Target date has passed. |
| `InvalidContribution` | 9 | Contribution amount invalid or would exceed target. |
| `CelebrationFailed` | 10 | Celebration creation failed. |
| `RewardIssuanceFailed` | 11 | Reward issuance failed. |
| `InvalidGoalStatus` | 12 | Goal is not in the expected state. |
| `MilestoneAlreadyCompleted` | 13 | Milestone was already completed. |

## Implementation Details

- Goals, contributions, celebrations, and badges are stored in persistent
  storage with unique counters.
- A member-goal index (`member_goals_{member}` → `Vec<u32>`) and a group-goal
  index (`group_goals_{group_id}` → `Vec<u32>`) enable efficient lookups.
- The milestone reward bitmask uses a `u64` per `(goal_id, member)` pair stored
  under `DataKey3::SavingsMilestonesClaimed`.
- Some query functions (`get_goal_contributions`, `get_milestone_celebrations`,
  `get_member_badges`, `get_celebration_leaderboard`,
  `get_top_goal_contributors`) return empty results — they are stubs that
  require a production indexing layer.

## Events

### MilestoneReached

Emitted from `check_and_distribute_milestone_rewards` when a token reward is
successfully transferred for a milestone.

| Field | Meaning |
|---|---|
| `group_id` | The group the goal belongs to. |
| `member` | The member who received the reward. |
| `milestone_pct` | The milestone percentage threshold. |
| `reward_amount` | The token amount transferred. |
