use crate::DistributionType;
use soroban_sdk::{contractevent, Address, BytesN, Env, Symbol, Vec};

/// Event: Rosca initialized
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoscaInitialized {
    pub member_count: u32,
    pub contribution_amount: i128,
}

/// Event: Group activated after delayed start
#[contractevent]
#[derive(Clone, Debug)]
pub struct GroupActivated {
    pub start_at: u64,
}

/// Event: Contribution received
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContributionReceived {
    pub contributor: Address,
    pub round: u32,
    pub token: Address,
    pub amount: i128,
}

/// Event: Savings milestone reached
#[contractevent]
#[derive(Clone, Debug)]
pub struct SavingsMilestoneReached {
    pub milestone: u32,
    pub total_collected: i128,
}

/// Event: Round closed (deadline passed, defaulters identified)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundClosed {
    pub round: u32,
    pub defaulters: Vec<Address>,
}

/// Event: Payout order finalized via randomization (#315)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PayoutOrderFinalized {
    pub round: u32,
    pub payout_order: Vec<Address>,
}

/// Event: PayoutOrder compacted after a member exit (#389).
///
/// Emitted when `approve_exit` runs the compactor. `skipped_reason` is `0`
/// when compaction ran, `1` when compaction was skipped because the exiting
/// member's slot had already been paid out by an earlier round (in which
/// case `old_len == new_len`), and `2` when the layout was left alone
/// because the member was absent from `PayoutOrder` (defensive).
#[contractevent]
#[derive(Clone, Debug)]
pub struct PayoutOrderCompacted {
    pub exited_member: Address,
    pub slot_index: u32,
    pub old_len: u32,
    pub new_len: u32,
    pub current_round: u32,
    pub skipped_reason: u32,
}

/// Event: Member defaulted on a round
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberDefaulted {
    pub member: Address,
    pub round: u32,
    pub penalty_amount: i128,
    pub default_count: u32,
}

/// Event: Member suspended due to multiple defaults
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberSuspended {
    pub member: Address,
    pub default_count: u32,
}

/// Event: New member added
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberAdded {
    pub member: Address,
    pub member_count: u32,
}

/// Event: Member removed by admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberRemoved {
    pub member: Address,
    pub member_count: u32,
}

/// Event: Token approved for contributions
#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenApproved {
    pub token: Address,
}

/// Event: Token removed from approved list
#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenRemoved {
    pub token: Address,
}

/// Event: Exchange rate updated for a token
#[contractevent]
#[derive(Clone, Debug)]
pub struct ExchangeRateSet {
    pub token: Address,
    pub rate: i128,
}

/// Event: Contribution limit set for a token
#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenLimitSet {
    pub token: Address,
    pub limit: i128,
}

/// Event: Rewards deposited into the pool
#[contractevent]
#[derive(Clone, Debug)]
pub struct RewardDeposited {
    pub depositor: Address,
    pub amount: i128,
}

/// Event: Reward distribution configuration updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct RewardConfigUpdated {
    pub dist_type: DistributionType,
}

/// Event: Rewards claimed by a member
#[contractevent]
#[derive(Clone, Debug)]
pub struct RewardClaimed {
    pub member: Address,
    pub amount: i128,
}

/// Event: New governance proposal created
#[contractevent]
#[derive(Clone, Debug)]
pub struct ProposalCreated {
    pub proposal_id: u32,
    pub creator: Address,
    pub target_member: Address,
    pub created_at: u64,
    pub deadline: u64,
}

/// Event: Vote cast on a proposal
#[contractevent]
#[derive(Clone, Debug)]
pub struct VoteCast {
    pub proposal_id: u32,
    pub voter: Address,
    pub vote_for: bool,
}

/// Event: Weighted vote cast on a proposal
#[contractevent]
#[derive(Clone, Debug)]
pub struct WeightedVoteCast {
    pub member: Address,
    pub proposal_id: u32,
    pub weight: i128,
}

/// Event: Proposal rejected
#[contractevent]
#[derive(Clone, Debug)]
pub struct ProposalRejected {
    pub proposal_id: u32,
    pub reason: Symbol,
    pub votes_for: i128,
    pub votes_against: i128,
}

/// Event: Proposal executed
#[contractevent]
#[derive(Clone, Debug)]
pub struct ProposalExecuted {
    pub proposal_id: u32,
    pub proposal_type: u32,
    pub target_member: Address,
}

/// Event: Penalty appeal successfully approved
#[contractevent]
#[derive(Clone, Debug)]
pub struct PenaltyAppealApproved {
    pub member: Address,
}

/// Event: Voting quorum percentage updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct QuorumUpdated {
    pub new_quorum: i128,
}

/// Event: Quorum configured for a specific proposal type
#[contractevent]
#[derive(Clone, Debug)]
pub struct QuorumConfigUpdated {
    pub proposal_type: crate::ProposalType,
    pub quorum_bps: u32,
}

/// Event: Member removed via proposal execution
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberRemovalExecuted {
    pub member: Address,
}

/// Event: Deadline reminder emitted
#[contractevent]
#[derive(Clone, Debug)]
pub struct DeadlineReminder {
    pub round: u32,
    pub time_remaining: u64,
    pub non_contributors: Vec<Address>,
    pub interval: Symbol,
}

/// Event: Contract paused
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractPaused {
    pub reason: soroban_sdk::String,
}

/// Event: Contract resumed
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractResumed {
    pub reason: soroban_sdk::String,
}

/// Event: Round finalized by admin after deadline (payout executed with partial contributions)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundFinalized {
    pub round: u32,
    pub defaulters: Vec<Address>,
}

/// Event: Emergency exit requested
#[contractevent]
#[derive(Clone, Debug)]
pub struct ExitRequested {
    pub member: Address,
    pub round: u32,
}

/// Event: Emergency exit approved
#[contractevent]
#[derive(Clone, Debug)]
pub struct ExitApproved {
    pub member: Address,
    pub refund_amount: i128,
}

/// Event: Emergency exit rejected
#[contractevent]
#[derive(Clone, Debug)]
pub struct ExitRejected {
    pub member: Address,
}

/// Event: Round payout completed
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundCompleted {
    pub round: u32,
    pub recipient: Address,
    pub payout_amount: i128,
}

/// Event: Payout reinvested into next round
#[contractevent]
#[derive(Clone, Debug)]
pub struct PayoutReinvested {
    pub member: Address,
    pub round: u32,
    pub amount: i128,
}

/// Event: Member requested to skip a round
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundSkipRequested {
    pub member: Address,
    pub round: u32,
    pub fee_paid: i128,
}

/// Event: Protocol fee collected from round payout
#[contractevent]
#[derive(Clone, Debug)]
pub struct FeeCollected {
    pub round: u32,
    pub fee_amount: i128,
    pub fee_recipient: Address,
}

/// Event: Partial contribution received (installment payment)
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialContributionReceived {
    pub member: Address,
    pub round: u32,
    pub amount: i128,
    pub remaining: i128,
}

/// Event: Suspension threshold configured
#[contractevent]
#[derive(Clone, Debug)]
pub struct SuspensionThresholdSet {
    pub max_defaults: u32,
}

/// Event: Defaulter penalty deferred due to grace period
#[contractevent]
#[derive(Clone, Debug)]
pub struct GracePeriodWarning {
    pub member: Address,
    pub round: u32,
    pub expires_at_ledger: u64,
}

/// Event: Member reputation score changed by protocol logic.
#[contractevent]
#[derive(Clone, Debug)]
pub struct ReputationUpdated {
    pub member: Address,
    pub old_score: i128,
    pub new_score: i128,
    pub reason: Symbol,
}

/// Event: Round state reset for next round
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundReset {
    pub round: u32,
}

/// Event: Admin transfer proposed
#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferProposed {
    pub current_admin: Address,
    pub proposed_admin: Address,
}

/// Event: Admin transfer accepted
#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
}

/// Event: Contract WASM upgraded
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractUpgraded {
    pub old_version: u32,
    pub new_version: u32,
    pub by_admin: Address,
}

/// Event: Member contribution tier updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberTierSet {
    pub member: Address,
    pub tier_bps: u32,
}

// --- Helper Emission Functions ---

/// Event: Round deadline timestamp set
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundDeadlineTimestampSet {
    pub round: u32,
    pub timestamp: u64,
}

/// Event: Maximum member limit updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct MaxMembersUpdated {
    pub old_max: u32,
    pub new_max: u32,
}

pub fn emit_rosc_init(e: &Env, member_count: u32, contribution_amount: i128) {
    RoscaInitialized {
        member_count,
        contribution_amount,
    }
    .publish(e);
}

pub fn emit_group_activated(e: &Env, start_at: u64) {
    GroupActivated { start_at }.publish(e);
}

pub fn emit_round_deadline_timestamp_set(e: &Env, round: u32, timestamp: u64) {
    RoundDeadlineTimestampSet { round, timestamp }.publish(e);
}

pub fn emit_max_members_upd(e: &Env, old_max: u32, new_max: u32) {
    MaxMembersUpdated { old_max, new_max }.publish(e);
}

pub fn emit_member_tier_set(e: &Env, member: Address, tier_bps: u32) {
    MemberTierSet { member, tier_bps }.publish(e);
}

pub fn emit_contrib(e: &Env, contributor: Address, round: u32, token: Address, amount: i128) {
    ContributionReceived {
        contributor,
        round,
        token,
        amount,
    }
    .publish(e);
}

pub fn emit_milestone(e: &Env, milestone: u32, total_collected: i128) {
    SavingsMilestoneReached {
        milestone,
        total_collected,
    }
    .publish(e);
}

pub fn emit_closed(e: &Env, round: u32, defaulters: Vec<Address>) {
    RoundClosed { round, defaulters }.publish(e);
}

pub fn emit_payout_order_finalized(e: &Env, round: u32, payout_order: Vec<Address>) {
    PayoutOrderFinalized { round, payout_order }.publish(e);
}

pub fn emit_payout_order_compacted(
    e: &Env,
    exited_member: Address,
    slot_index: u32,
    old_len: u32,
    new_len: u32,
    current_round: u32,
    skipped_reason: u32,
) {
    PayoutOrderCompacted {
        exited_member,
        slot_index,
        old_len,
        new_len,
        current_round,
        skipped_reason,
    }
    .publish(e);
}

pub fn emit_defaulted(
    e: &Env,
    member: Address,
    round: u32,
    penalty_amount: i128,
    default_count: u32,
) {
    MemberDefaulted {
        member,
        round,
        penalty_amount,
        default_count,
    }
    .publish(e);
}

pub fn emit_suspended(e: &Env, member: Address, default_count: u32) {
    MemberSuspended {
        member,
        default_count,
    }
    .publish(e);
}

pub fn emit_mem_add(e: &Env, member: Address, member_count: u32) {
    MemberAdded {
        member,
        member_count,
    }
    .publish(e);
}

pub fn emit_mem_rmv(e: &Env, member: Address, member_count: u32) {
    MemberRemoved {
        member,
        member_count,
    }
    .publish(e);
}

pub fn emit_tok_add(e: &Env, token: Address) {
    TokenApproved { token }.publish(e);
}

pub fn emit_tok_rmv(e: &Env, token: Address) {
    TokenRemoved { token }.publish(e);
}

pub fn emit_rate_set(e: &Env, token: Address, rate: i128) {
    ExchangeRateSet { token, rate }.publish(e);
}

pub fn emit_lim_set(e: &Env, token: Address, limit: i128) {
    TokenLimitSet { token, limit }.publish(e);
}

pub fn emit_rew_dep(e: &Env, depositor: Address, amount: i128) {
    RewardDeposited { depositor, amount }.publish(e);
}

pub fn emit_rew_cfg(e: &Env, dist_type: DistributionType) {
    RewardConfigUpdated { dist_type }.publish(e);
}

pub fn emit_rew_clm(e: &Env, member: Address, amount: i128) {
    RewardClaimed { member, amount }.publish(e);
}

pub fn emit_prop_new(
    e: &Env,
    proposal_id: u32,
    creator: Address,
    target_member: Address,
    created_at: u64,
    deadline: u64,
) {
    ProposalCreated {
        proposal_id,
        creator,
        target_member,
        created_at,
        deadline,
    }
    .publish(e);
}

pub fn emit_voted(e: &Env, proposal_id: u32, voter: Address, vote_for: bool) {
    VoteCast {
        proposal_id,
        voter,
        vote_for,
    }
    .publish(e);
}

pub fn emit_weighted_vote_cast(e: &Env, member: Address, proposal_id: u32, weight: i128) {
    WeightedVoteCast {
        member,
        proposal_id,
        weight,
    }
    .publish(e);
}

pub fn emit_prop_rej(
    e: &Env,
    proposal_id: u32,
    reason: Symbol,
    votes_for: i128,
    votes_against: i128,
) {
    ProposalRejected {
        proposal_id,
        reason,
        votes_for,
        votes_against,
    }
    .publish(e);
}

pub fn emit_prop_exec(e: &Env, proposal_id: u32, proposal_type: u32, target_member: Address) {
    ProposalExecuted {
        proposal_id,
        proposal_type,
        target_member,
    }
    .publish(e);
}

pub fn emit_appeal_ok(e: &Env, member: Address) {
    PenaltyAppealApproved { member }.publish(e);
}

pub fn emit_rule_upd(e: &Env, new_quorum: i128) {
    QuorumUpdated { new_quorum }.publish(e);
}

pub fn emit_quorum_config_updated(e: &Env, proposal_type: crate::ProposalType, quorum_bps: u32) {
    QuorumConfigUpdated {
        proposal_type,
        quorum_bps,
    }
    .publish(e);
}

pub fn emit_mem_del(e: &Env, member: Address) {
    MemberRemovalExecuted { member }.publish(e);
}

pub fn emit_reminder(
    e: &Env,
    round: u32,
    time_remaining: u64,
    non_contributors: Vec<Address>,
    interval: Symbol,
) {
    DeadlineReminder {
        round,
        time_remaining,
        non_contributors,
        interval,
    }
    .publish(e);
}

pub fn emit_paused(e: &Env, reason: soroban_sdk::String) {
    ContractPaused { reason }.publish(e);
}

pub fn emit_resumed(e: &Env, reason: soroban_sdk::String) {
    ContractResumed { reason }.publish(e);
}

pub fn emit_round_finalized(e: &Env, round: u32, defaulters: Vec<Address>) {
    RoundFinalized { round, defaulters }.publish(e);
}

pub fn emit_exit_req(e: &Env, member: Address, round: u32) {
    ExitRequested { member, round }.publish(e);
}

pub fn emit_exit_ok(e: &Env, member: Address, refund_amount: i128) {
    ExitApproved {
        member,
        refund_amount,
    }
    .publish(e);
}

pub fn emit_exit_no(e: &Env, member: Address) {
    ExitRejected { member }.publish(e);
}

pub fn emit_rd_done(e: &Env, round: u32, recipient: Address, payout_amount: i128) {
    RoundCompleted {
        round,
        recipient,
        payout_amount,
    }
    .publish(e);
}

pub fn emit_payout_reinvested(e: &Env, member: Address, round: u32, amount: i128) {
    PayoutReinvested {
        member,
        round,
        amount,
    }
    .publish(e);
}

pub fn emit_reset(e: &Env, round: u32) {
    RoundReset { round }.publish(e);
}

pub fn emit_admin_transfer_proposed(e: &Env, current_admin: Address, proposed_admin: Address) {
    AdminTransferProposed {
        current_admin,
        proposed_admin,
    }
    .publish(e);
}

pub fn emit_admin_transferred(e: &Env, old_admin: Address, new_admin: Address) {
    AdminTransferred {
        old_admin,
        new_admin,
    }
    .publish(e);
}

pub fn emit_contract_upgraded(e: &Env, old_version: u32, new_version: u32, by_admin: Address) {
    ContractUpgraded {
        old_version,
        new_version,
        by_admin,
    }
    .publish(e);
}

pub fn emit_fee_collected(e: &Env, round: u32, fee_amount: i128, fee_recipient: Address) {
    FeeCollected {
        round,
        fee_amount,
        fee_recipient,
    }
    .publish(e);
}

pub fn emit_partial_contribution(e: &Env, member: Address, round: u32, amount: i128, remaining: i128) {
    PartialContributionReceived {
        member,
        round,
        amount,
        remaining,
    }
    .publish(e);
}

pub fn emit_suspension_threshold_set(e: &Env, max_defaults: u32) {
    SuspensionThresholdSet { max_defaults }.publish(e);
}

pub fn emit_grace_period_warning(e: &Env, member: Address, round: u32, expires_at_ledger: u64) {
    GracePeriodWarning {
        member,
        round,
        expires_at_ledger,
    }
    .publish(e);
}

pub fn emit_reputation_updated(
    e: &Env,
    member: Address,
    old_score: i128,
    new_score: i128,
    reason: Symbol,
) {
    ReputationUpdated {
        member,
        old_score,
        new_score,
        reason,
    }
    .publish(e);
}

// --- Delegated Voting Events ---

/// Event: Vote delegation created
#[contractevent]
#[derive(Clone, Debug)]
pub struct VoteDelegated {
    pub delegator: Address,
    pub delegate: Address,
}

/// Event: Vote delegation revoked
#[contractevent]
#[derive(Clone, Debug)]
pub struct DelegationRevoked {
    pub delegator: Address,
}

pub fn emit_vote_delegated(e: &Env, delegator: Address, delegate: Address) {
    VoteDelegated { delegator, delegate }.publish(e);
}

pub fn emit_delegation_revoked(e: &Env, delegator: Address) {
    DelegationRevoked { delegator }.publish(e);
}

// --- Auto-Close Round Events ---

/// Event: Round auto-closed when all members contributed
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundAutoClosedEarly {
    pub round: u32,
    pub closed_at_ledger: u64,
}

pub fn emit_round_auto_closed_early(e: &Env, round: u32, closed_at_ledger: u64) {
    RoundAutoClosedEarly { round, closed_at_ledger }.publish(e);
}

// --- Invitation-Based Member Joining Events ---

/// Event: Invite generated for a new member
#[contractevent]
#[derive(Clone, Debug)]
pub struct InviteGenerated {
    pub invitee: Address,
    pub expires_at: u64,
}

/// Event: Invite redeemed and member joined
#[contractevent]
#[derive(Clone, Debug)]
pub struct InviteRedeemed {
    pub invitee: Address,
}

pub fn emit_invite_generated(e: &Env, invitee: Address, expires_at: u64) {
    InviteGenerated { invitee, expires_at }.publish(e);
}

pub fn emit_invite_redeemed(e: &Env, invitee: Address) {
    InviteRedeemed { invitee }.publish(e);
}

// --- Admin Multi-Sig Events ---

/// Event: Admin action proposed
#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminActionProposed {
    pub action_id: u32,
    pub action_type: Symbol,
    pub proposed_by: Address,
}

/// Event: Admin action approved by a co-admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminActionApproved {
    pub action_id: u32,
    pub approved_by: Address,
    pub approval_count: u32,
}

/// Event: Admin action executed
#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminActionExecuted {
    pub action_id: u32,
    pub action_type: Symbol,
}

pub fn emit_admin_action_proposed(e: &Env, action_id: u32, action_type: Symbol, proposed_by: Address) {
    AdminActionProposed { action_id, action_type, proposed_by }.publish(e);
}

pub fn emit_admin_action_approved(e: &Env, action_id: u32, approved_by: Address, approval_count: u32) {
    AdminActionApproved { action_id, approved_by, approval_count }.publish(e);
}

pub fn emit_admin_action_executed(e: &Env, action_id: u32, action_type: Symbol) {
    AdminActionExecuted { action_id, action_type }.publish(e);
}

// --- Insurance Pool Events ---

/// Event: Insurance pool top-up contribution
#[contractevent]
#[derive(Clone, Debug)]
pub struct InsurancePoolTopUp {
    pub contributor: Address,
    pub amount: i128,
}

/// Event: Insurance pool paid out to cover shortfall
#[contractevent]
#[derive(Clone, Debug)]
pub struct InsurancePaidOut {
    pub round: u32,
    pub shortfall: i128,
    pub remaining_pool: i128,
}

pub fn emit_insurance_top_up(e: &Env, contributor: Address, amount: i128) {
    InsurancePoolTopUp { contributor, amount }.publish(e);
}

pub fn emit_insurance_paid_out(e: &Env, round: u32, shortfall: i128, remaining_pool: i128) {
    InsurancePaidOut { round, shortfall, remaining_pool }.publish(e);
}

pub fn emit_round_skip_requested(e: &Env, member: Address, round: u32, fee_paid: i128) {
    RoundSkipRequested { member, round, fee_paid }.publish(e);
}

// --- Audit Trail Events ---

/// Event: Cycle record created
#[contractevent]
#[derive(Clone, Debug)]
pub struct CycleRecordCreated {
    pub cycle_number: u32,
    pub total_pool_amount: i128,
    pub payout_recipient: Address,
}

/// Event: Cycle record archived
#[contractevent]
#[derive(Clone, Debug)]
pub struct CycleRecordArchived {
    pub cycle_number: u32,
}

/// Event: Retention window updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct RetentionWindowUpdated {
    pub old_window: u32,
    pub new_window: u32,
}

pub fn emit_cycle_record_created(e: &Env, cycle_number: u32, total_pool_amount: i128, payout_recipient: Address) {
    CycleRecordCreated { cycle_number, total_pool_amount, payout_recipient }.publish(e);
}

pub fn emit_cycle_record_archived(e: &Env, cycle_number: u32) {
    CycleRecordArchived { cycle_number }.publish(e);
}

pub fn emit_retention_window_updated(e: &Env, old_window: u32, new_window: u32) {
    RetentionWindowUpdated { old_window, new_window }.publish(e);
}

pub fn emit_waitlist_updated(e: &Env, member: Address, joined: bool, size: u32) {
    e.events()
        .publish((Symbol::new(e, "WaitlistUpd"),), (member, joined, size));
}

pub fn emit_member_enrolled_from_waitlist(
    e: &Env,
    member: Address,
    vacated_by: Address,
    round: u32,
    catch_up_amount: i128,
) {
    e.events()
        .publish(
            (Symbol::new(e, "WaitEnroll"),),
            (member, vacated_by, round, catch_up_amount),
        );
}

// --- Emergency Payout Events ---

/// Event: Emergency payout requested
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyPayoutRequested {
    pub requester: Address,
    pub round: u32,
    pub reason_hash: BytesN<32>,
    pub deadline: u64,
}

/// Event: Vote cast on emergency payout
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyPayoutVoteCast {
    pub requester: Address,
    pub round: u32,
    pub voter: Address,
    pub approve: bool,
    pub votes_for: i128,
    pub votes_against: i128,
}

/// Event: Emergency payout approved by quorum
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyPayoutApproved {
    pub requester: Address,
    pub round: u32,
    pub payout_amount: i128,
}

/// Event: Emergency payout rejected (quorum not met or vote expired)
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyPayoutRejected {
    pub requester: Address,
    pub round: u32,
    pub reason: Symbol,
}

/// Event: Emergency payout executed
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyPayoutExecuted {
    pub requester: Address,
    pub round: u32,
    pub payout_amount: i128,
}

/// Event: Emergency payout config updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyPayoutConfigUpdated {
    pub emergency_quorum_bps: u32,
    pub vote_window_seconds: u64,
    pub max_emergency_per_cycle: u32,
}

// --- Group Dissolution Events ---

/// Event: Dissolution vote started
#[contractevent]
#[derive(Clone, Debug)]
pub struct DissolutionVoteStarted {
    pub round: u32,
    pub deadline: u64,
}

/// Event: Vote cast on dissolution
#[contractevent]
#[derive(Clone, Debug)]
pub struct DissolutionVoteCast {
    pub round: u32,
    pub voter: Address,
    pub approve: bool,
    pub votes_for: i128,
}

/// Event: Dissolution quorum reached
#[contractevent]
#[derive(Clone, Debug)]
pub struct DissolutionQuorumReached {
    pub round: u32,
    pub votes_for: i128,
}

/// Event: Group dissolved
#[contractevent]
#[derive(Clone, Debug)]
pub struct GroupDissolved {
    pub round: u32,
    pub reason_hash: BytesN<32>,
    pub total_pool: i128,
    pub member_count: u32,
}

/// Event: Member refunded during dissolution
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberRefunded {
    pub member: Address,
    pub amount: i128,
    pub contribution: i128,
    pub total_pool: i128,
}

/// Event: Dissolution config updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct DissolutionConfigUpdated {
    pub dissolution_quorum_bps: u32,
    pub vote_window_seconds: u64,
}

// --- Helper Emission Functions ---

pub fn emit_emergency_payout_requested(e: &Env, requester: Address, round: u32, reason_hash: BytesN<32>, deadline: u64) {
    EmergencyPayoutRequested { requester, round, reason_hash, deadline }.publish(e);
}

pub fn emit_emergency_payout_vote_cast(e: &Env, requester: Address, round: u32, voter: Address, approve: bool, votes_for: i128, votes_against: i128) {
    EmergencyPayoutVoteCast { requester, round, voter, approve, votes_for, votes_against }.publish(e);
}

pub fn emit_emergency_payout_approved(e: &Env, requester: Address, round: u32, payout_amount: i128) {
    EmergencyPayoutApproved { requester, round, payout_amount }.publish(e);
}

pub fn emit_emergency_payout_rejected(e: &Env, requester: Address, round: u32, reason: Symbol) {
    EmergencyPayoutRejected { requester, round, reason }.publish(e);
}

pub fn emit_emergency_payout_executed(e: &Env, requester: Address, round: u32, payout_amount: i128) {
    EmergencyPayoutExecuted { requester, round, payout_amount }.publish(e);
}

pub fn emit_emergency_payout_config_updated(e: &Env, emergency_quorum_bps: u32, vote_window_seconds: u64, max_emergency_per_cycle: u32) {
    EmergencyPayoutConfigUpdated { emergency_quorum_bps, vote_window_seconds, max_emergency_per_cycle }.publish(e);
}

pub fn emit_dissolution_vote_started(e: &Env, round: u32, deadline: u64) {
    DissolutionVoteStarted { round, deadline }.publish(e);
}

pub fn emit_dissolution_vote_cast(e: &Env, round: u32, voter: Address, approve: bool, votes_for: i128) {
    DissolutionVoteCast { round, voter, approve, votes_for }.publish(e);
}

pub fn emit_dissolution_quorum_reached(e: &Env, round: u32, votes_for: i128) {
    DissolutionQuorumReached { round, votes_for }.publish(e);
}

pub fn emit_group_dissolved(e: &Env, round: u32, reason_hash: BytesN<32>, total_pool: i128, member_count: u32) {
    GroupDissolved { round, reason_hash, total_pool, member_count }.publish(e);
}

pub fn emit_member_refunded(e: &Env, member: Address, amount: i128, contribution: i128, total_pool: i128) {
    MemberRefunded { member, amount, contribution, total_pool }.publish(e);
}

pub fn emit_dissolution_config_updated(e: &Env, dissolution_quorum_bps: u32, vote_window_seconds: u64) {
    DissolutionConfigUpdated { dissolution_quorum_bps, vote_window_seconds }.publish(e);
}

// #213: Slot Swap Events
pub fn emit_slot_swap_requested(e: &Env, swap_id: u32, initiator: Address, counterparty: Address, round_a: u32, round_b: u32) {
    e.events().publish((Symbol::new(e, "SlotSwapReq"),), (swap_id, initiator, counterparty, round_a, round_b));
}
pub fn emit_slot_swap_accepted(e: &Env, swap_id: u32, counterparty: Address) {
    e.events().publish((Symbol::new(e, "SlotSwapAcc"),), (swap_id, counterparty));
}
pub fn emit_slot_swap_rejected(e: &Env, swap_id: u32, counterparty: Address) {
    e.events().publish((Symbol::new(e, "SlotSwapRej"),), (swap_id, counterparty));
}
pub fn emit_slot_swap_executed(e: &Env, swap_id: u32, round_a: u32, round_b: u32) {
    e.events().publish((Symbol::new(e, "SlotSwapExec"),), (swap_id, round_a, round_b));
}
pub fn emit_slot_swap_expired(e: &Env, swap_id: u32) {
    e.events().publish((Symbol::new(e, "SlotSwapExp"),), (swap_id,));
}

// #214: Insurance Coverage Events
pub fn emit_insurance_claim_executed(e: &Env, round: u32, defaulter: Address, amount_covered: i128) {
    e.events().publish((Symbol::new(e, "InsClaim"),), (round, defaulter, amount_covered));
}
pub fn emit_insurance_pool_exhausted(e: &Env, round: u32, shortfall_remaining: i128) {
    e.events().publish((Symbol::new(e, "InsExhausted"),), (round, shortfall_remaining));
}
pub fn emit_insurance_coverage_mode_set(e: &Env, mode: u32) {
    e.events().publish((Symbol::new(e, "InsModeSet"),), (mode,));
}

// #218: Reinstatement Events
pub fn emit_reinstatement_requested(e: &Env, member: Address, proposal_id: u32) {
    e.events().publish((Symbol::new(e, "ReinReq"),), (member, proposal_id));
}
pub fn emit_reinstatement_approved(e: &Env, member: Address) {
    e.events().publish((Symbol::new(e, "ReinApproved"),), (member,));
}
pub fn emit_reinstatement_fee_collected(e: &Env, member: Address, amount: i128) {
    e.events().publish((Symbol::new(e, "ReinFee"),), (member, amount));
}


// #230: Group Merge Events
pub fn emit_merge_proposed(e: &Env, proposal_id: u32, group_a_admin: Address, group_b_id: u32) {
    e.events().publish((Symbol::new(e, "MergeProposed"),), (proposal_id, group_a_admin, group_b_id));
}
pub fn emit_merge_accepted(e: &Env, proposal_id: u32) {
    e.events().publish((Symbol::new(e, "MergeAccepted"),), (proposal_id,));
}
pub fn emit_merge_completed(e: &Env, proposal_id: u32, members_added: u32) {
    e.events().publish((Symbol::new(e, "MergeCompleted"),), (proposal_id, members_added));
}
pub fn emit_group_marked_merged(e: &Env, group_b_id: u32) {
    e.events().publish((Symbol::new(e, "GroupMerged"),), (group_b_id,));
}
// #224: Cycle Completion Bonus Events

/// Event: Cycle bonus amount configured by admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct CycleBonusConfigured {
    pub amount: i128,
}

/// Event: Cycle bonus paid to a qualifying member
#[contractevent]
#[derive(Clone, Debug)]
pub struct CycleBonusPaid {
    pub member: Address,
    pub amount: i128,
    pub cycle: u32,
}

/// Event: Cycle bonus prorated due to insufficient reward pool
#[contractevent]
#[derive(Clone, Debug)]
pub struct CycleBonusProrated {
    pub cycle: u32,
    pub shortfall: i128,
}

pub fn emit_cycle_bonus_configured(e: &Env, amount: i128) {
    CycleBonusConfigured { amount }.publish(e);
}

pub fn emit_cycle_bonus_paid(e: &Env, member: Address, amount: i128, cycle: u32) {
    CycleBonusPaid { member, amount, cycle }.publish(e);
}

pub fn emit_cycle_bonus_prorated(e: &Env, cycle: u32, shortfall: i128) {
    CycleBonusProrated { cycle, shortfall }.publish(e);
}

// #227: Round Duration Update Events

/// Event: Round duration update scheduled (pending, takes effect next round)
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundDurationUpdateScheduled {
    pub old_duration: u64,
    pub new_duration: u64,
    pub effective_from_round: u32,
}

/// Event: Pending round duration applied at round start
#[contractevent]
#[derive(Clone, Debug)]
pub struct RoundDurationApplied {
    pub round: u32,
    pub duration: u64,
}

pub fn emit_round_duration_update_scheduled(e: &Env, old_duration: u64, new_duration: u64, effective_from_round: u32) {
    RoundDurationUpdateScheduled { old_duration, new_duration, effective_from_round }.publish(e);
}

pub fn emit_round_duration_applied(e: &Env, round: u32, duration: u64) {
    RoundDurationApplied { round, duration }.publish(e);
}

// #240: Co-Signer Guarantee Events

pub fn emit_co_signer_set(e: &Env, group_id: u32, member: Address, co_signer: Address) {
    e.events().publish((soroban_sdk::Symbol::new(e, "CoSignerSet"),), (group_id, member, co_signer));
}

pub fn emit_co_signer_accepted(e: &Env, group_id: u32, member: Address, co_signer: Address) {
    e.events().publish((soroban_sdk::Symbol::new(e, "CoSignerAccepted"),), (group_id, member, co_signer));
}

pub fn emit_co_signer_contributed(e: &Env, group_id: u32, member: Address, co_signer: Address, amount: i128) {
    e.events().publish((soroban_sdk::Symbol::new(e, "CoSignerContributed"),), (group_id, member, co_signer, amount));
}

pub fn emit_co_signer_window_expired(e: &Env, group_id: u32, member: Address) {
    e.events().publish((soroban_sdk::Symbol::new(e, "CoSignerWinExpired"),), (group_id, member));
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ProxyAuthorized {
    pub group_id: u32,
    pub member: Address,
    pub proxy: Address,
    pub max_rounds: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ProxyContributed {
    pub group_id: u32,
    pub member: Address,
    pub proxy: Address,
    pub round_index: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ProxyRevoked {
    pub group_id: u32,
    pub member: Address,
    pub proxy: Address,
}

pub fn emit_proxy_authorized(
    e: &Env,
    group_id: u32,
    member: Address,
    proxy: Address,
    max_rounds: u32,
) {
    ProxyAuthorized {
        group_id,
        member,
        proxy,
        max_rounds,
    }
    .publish(e);
}

pub fn emit_proxy_contributed(
    e: &Env,
    group_id: u32,
    member: Address,
    proxy: Address,
    round_index: u32,
) {
    ProxyContributed {
        group_id,
        member,
        proxy,
        round_index,
    }
    .publish(e);
}

pub fn emit_proxy_revoked(e: &Env, group_id: u32, member: Address, proxy: Address) {
    ProxyRevoked {
        group_id,
        member,
        proxy,
    }
    .publish(e);
}
// #236: Group Activity Freeze Events

/// Event: Group frozen by contract-level admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct GroupFrozen {
    pub group_id: u32,
    pub reason_hash: BytesN<32>,
    pub frozen_at: u32,
}

/// Event: Group unfrozen by contract-level admin
#[contractevent]
#[derive(Clone, Debug)]
pub struct GroupUnfrozen {
    pub group_id: u32,
    pub resolution_hash: BytesN<32>,
    pub unfrozen_at: u32,
}

pub fn emit_group_frozen(e: &Env, group_id: u32, reason_hash: BytesN<32>, frozen_at: u32) {
    GroupFrozen { group_id, reason_hash, frozen_at }.publish(e);
}

pub fn emit_group_unfrozen(e: &Env, group_id: u32, resolution_hash: BytesN<32>, unfrozen_at: u32) {
    GroupUnfrozen { group_id, resolution_hash, unfrozen_at }.publish(e);
}
// #243: Group State Snapshot Events

/// Event: Group state snapshot taken
#[contractevent]
#[derive(Clone, Debug)]
pub struct SnapshotTaken {
    pub snapshot_id: u32,
    pub taken_by: Address,
    pub state_hash: BytesN<32>,
}

pub fn emit_snapshot_taken(e: &Env, snapshot_id: u32, taken_by: Address, state_hash: BytesN<32>) {
    SnapshotTaken { snapshot_id, taken_by, state_hash }.publish(e);
}

// #267: Tiered Contribution Level Events

/// Event: A new tier was defined for the group (#267)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TierDefined {
    pub tier_id: u32,
    pub name: soroban_sdk::Symbol,
    pub contribution_amount: i128,
    pub payout_weight: u32,
}

/// Event: Member joined the group with a specific tier (#267)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberJoinedWithTier {
    pub member: Address,
    pub tier_id: u32,
}

/// Event: Member's tier change queued or applied (#267)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberTierChanged {
    pub member: Address,
    pub old_tier: u32,
    pub new_tier: u32,
    pub effective_cycle: u32,
}

pub fn emit_tier_defined(e: &Env, tier_id: u32, name: soroban_sdk::Symbol, contribution_amount: i128, payout_weight: u32) {
    TierDefined { tier_id, name, contribution_amount, payout_weight }.publish(e);
}

pub fn emit_member_joined_with_tier(e: &Env, member: Address, tier_id: u32) {
    MemberJoinedWithTier { member, tier_id }.publish(e);
}

pub fn emit_member_tier_changed(e: &Env, member: Address, old_tier: u32, new_tier: u32, effective_cycle: u32) {
    MemberTierChanged { member, old_tier, new_tier, effective_cycle }.publish(e);
}

// #269: On-Chain Member Credit Score Events

/// Event: A member's credit score was updated (#269)
#[contractevent]
#[derive(Clone, Debug)]
pub struct CreditScoreUpdated {
    pub member: Address,
    pub old_score: i128,
    pub new_score: i128,
    pub reason: soroban_sdk::Symbol,
}

pub fn emit_credit_score_updated(e: &Env, member: Address, old_score: i128, new_score: i128, reason: soroban_sdk::Symbol) {
    CreditScoreUpdated { member, old_score, new_score, reason }.publish(e);
}

// ── Reputation-Gated Fee Discount Event ──────────────────────────────────────

/// Event: protocol fee reduced because recipient's credit score met the threshold.
#[contractevent]
#[derive(Clone, Debug)]
pub struct RepFeeDiscountApplied {
    pub member: Address,
    pub original_fee_bps: u32,
    pub effective_fee_bps: u32,
    pub score: i128,
}

pub fn emit_rep_fee_discount_applied(
    e: &Env,
    member: Address,
    original_fee_bps: u32,
    effective_fee_bps: u32,
    score: i128,
) {
    RepFeeDiscountApplied {
        member,
        original_fee_bps,
        effective_fee_bps,
        score,
    }
    .publish(e);
}

// ── #330: Contribution Delegation Events ─────────────────────────────────────

/// Event: Member granted contribution delegation to a proxy (#330)
#[contractevent]
#[derive(Clone, Debug)]
pub struct DelegationGranted {
    pub group_id: u32,
    pub member: Address,
    pub proxy: Address,
    pub expiry_ledger: u64,
}

/// Event: Member revoked contribution delegation (#330)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContribDelegationRevoked {
    pub group_id: u32,
    pub member: Address,
    pub proxy: Address,
}

pub fn emit_delegation_granted(e: &Env, group_id: u32, member: Address, proxy: Address, expiry_ledger: u64) {
    DelegationGranted { group_id, member, proxy, expiry_ledger }.publish(e);
}

pub fn emit_contribution_delegation_revoked(e: &Env, group_id: u32, member: Address, proxy: Address) {
    ContribDelegationRevoked { group_id, member, proxy }.publish(e);
}

// ── #331: Group Split Events ──────────────────────────────────────────────────

/// Event: Group split proposed (#331)
#[contractevent]
#[derive(Clone, Debug)]
pub struct GroupSplitProposed {
    pub source_group_id: u32,
    pub proposal_id: u32,
}

/// Event: Group split executed (#331)
#[contractevent]
#[derive(Clone, Debug)]
pub struct GroupSplitExecuted {
    pub source_group_id: u32,
    pub group_a_id: u32,
    pub group_b_id: u32,
}

pub fn emit_group_split_proposed(e: &Env, source_group_id: u32, proposal_id: u32) {
    GroupSplitProposed { source_group_id, proposal_id }.publish(e);
}

pub fn emit_group_split_executed(e: &Env, source_group_id: u32, group_a_id: u32, group_b_id: u32) {
    GroupSplitExecuted { source_group_id, group_a_id, group_b_id }.publish(e);
}


/// Event: Group treasury enabled (#314)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryEnabled {
    pub group_id: u32,
    pub treasury_admin: Address,
}

/// Event: Treasury round proposed (#314)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryRoundProposed {
    pub group_id: u32,
    pub round_index: u32,
}

/// Event: Treasury round confirmed by majority vote (#314)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryRoundConfirmed {
    pub group_id: u32,
    pub round_index: u32,
}

/// Event: Treasury payment executed (#314)
#[contractevent]
#[derive(Clone, Debug)]
pub struct TreasuryPaymentExecuted {
    pub group_id: u32,
    pub recipient: Address,
    pub amount: i128,
}


pub fn emit_treasury_enabled(env: &Env, treasury_admin: Address) {
    TreasuryEnabled { group_id: 0, treasury_admin }.publish(env);
}

pub fn emit_treasury_round_proposed(env: &Env, round_index: u32) {
    TreasuryRoundProposed { group_id: 0, round_index }.publish(env);
}

pub fn emit_treasury_round_confirmed(env: &Env, round_index: u32) {
    TreasuryRoundConfirmed { group_id: 0, round_index }.publish(env);
}

pub fn emit_treasury_payment_executed(env: &Env, recipient: Address, amount: i128) {
    TreasuryPaymentExecuted { group_id: 0, recipient, amount }.publish(env);
}

// --- Emergency Liquidity Reserve Events (#313) ---

/// Event: Emergency loan granted
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyLoanGranted {
    pub group_id: u32,
    pub member: Address,
    pub loan_id: u32,
    pub amount: i128,
    pub repayment_deadline: u32,
}

/// Event: Emergency loan repaid
#[contractevent]
#[derive(Clone, Debug)]
pub struct EmergencyLoanRepaid {
    pub group_id: u32,
    pub loan_id: u32,
    pub amount: i128,
    pub remaining: i128,
}

/// Event: Loan default deducted from payout
#[contractevent]
#[derive(Clone, Debug)]
pub struct LoanDefaultDeducted {
    pub group_id: u32,
    pub loan_id: u32,
    pub deducted_from_payout: i128,
}

pub fn emit_emergency_loan_granted(
    e: &Env,
    group_id: u32,
    member: Address,
    loan_id: u32,
    amount: i128,
    repayment_deadline: u32,
) {
    EmergencyLoanGranted {
        group_id,
        member,
        loan_id,
        amount,
        repayment_deadline,
    }
    .publish(e);
}

pub fn emit_emergency_loan_repaid(e: &Env, group_id: u32, loan_id: u32, amount: i128, remaining: i128) {
    EmergencyLoanRepaid {
        group_id,
        loan_id,
        amount,
        remaining,
    }
    .publish(e);
}

pub fn emit_loan_default_deducted(e: &Env, group_id: u32, loan_id: u32, deducted_from_payout: i128) {
    LoanDefaultDeducted {
        group_id,
        loan_id,
        deducted_from_payout,
    }
    .publish(e);
}

// ── #352: Contribution Rebalancing ───────────────────────────────────────────

/// Event: Per-member contribution amount rebalanced after membership change (#352)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContributionRebalanced {
    pub old_amount: i128,
    pub new_amount: i128,
    pub reason: soroban_sdk::Symbol,
}

pub fn emit_contribution_rebalanced(
    e: &Env,
    old_amount: i128,
    new_amount: i128,
    reason: soroban_sdk::Symbol,
) {
    ContributionRebalanced {
        old_amount,
        new_amount,
        reason,
    }
    .publish(e);
}

// ── #356: Penalty-Based Slot Demotion ────────────────────────────────────────

/// Event: A member was demoted to the back of the payout queue after reaching
/// the configured late-contribution threshold (#356).
#[contractevent]
#[derive(Clone, Debug)]
pub struct MemberDemoted {
    pub member: Address,
    pub new_slot_index: u32,
    pub late_count: u32,
}

pub fn emit_member_demoted(e: &Env, member: Address, new_slot_index: u32, late_count: u32) {
    MemberDemoted {
        member,
        new_slot_index,
        late_count,
    }
    .publish(e);
}

/// Event: A member's late-contribution count was reset due to consecutive on-time payments.
#[contractevent]
#[derive(Clone, Debug)]
pub struct LateCountReset {
    pub member: Address,
}

pub fn emit_late_count_reset(e: &Env, member: Address) {
    LateCountReset { member }.publish(e);
}

// ── #364: Cycle Snapshot Versioning ──────────────────────────────────────────

/// Event: Automated cycle snapshot created at cycle end (#364)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SnapshotCreated {
    pub group_id: u32,
    pub cycle_number: u32,
    pub snapshot_hash: BytesN<32>,
}

pub fn emit_snapshot_created(e: &Env, group_id: u32, cycle_number: u32, snapshot_hash: BytesN<32>) {
    SnapshotCreated { group_id, cycle_number, snapshot_hash }.publish(e);
}

// ── #359: Savings Goal Milestone Rewards ─────────────────────────────────────

/// Event: Member reached a savings goal milestone and received a reward (#359)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MilestoneReached {
    pub group_id: u32,
    pub member: Address,
    pub milestone_pct: u32,
    pub reward_amount: i128,
}

pub fn emit_milestone_reached(
    e: &Env,
    group_id: u32,
    member: Address,
    milestone_pct: u32,
    reward_amount: i128,
) {
    MilestoneReached { group_id, member, milestone_pct, reward_amount }.publish(e);
}

// ── #375: Sealed-Bid (Commit-Reveal) Slot Auction Events ──────────────────────

/// Event: A sealed-bid auction was opened with commit/reveal deadlines.
#[contractevent]
#[derive(Clone, Debug)]
pub struct SealedAuctionOpened {
    pub group_id: u32,
    pub round: u32,
    pub commit_until: u64,
    pub reveal_until: u64,
}

/// Event: A sealed bid was committed (hash only; amount stays hidden).
#[contractevent]
#[derive(Clone, Debug)]
pub struct SlotBidCommitted {
    pub group_id: u32,
    pub round: u32,
    pub bidder: Address,
}

/// Event: A previously committed sealed bid was revealed and validated.
#[contractevent]
#[derive(Clone, Debug)]
pub struct SlotBidRevealed {
    pub group_id: u32,
    pub round: u32,
    pub bidder: Address,
    pub desired_slot: u32,
    pub bid_amount: i128,
}

/// Event: A sealed-bid auction was settled. `winning_bid` is 0 when no bid
/// cleared the minimum reserve (slot left unallocated).
#[contractevent]
#[derive(Clone, Debug)]
pub struct SealedAuctionSettled {
    pub group_id: u32,
    pub round: u32,
    pub winner: Address,
    pub winning_bid: i128,
}

pub fn emit_sealed_auction_opened(
    e: &Env,
    group_id: u32,
    round: u32,
    commit_until: u64,
    reveal_until: u64,
) {
    SealedAuctionOpened { group_id, round, commit_until, reveal_until }.publish(e);
}

pub fn emit_slot_bid_committed(e: &Env, group_id: u32, round: u32, bidder: Address) {
    SlotBidCommitted { group_id, round, bidder }.publish(e);
}

pub fn emit_slot_bid_revealed(
    e: &Env,
    group_id: u32,
    round: u32,
    bidder: Address,
    desired_slot: u32,
    bid_amount: i128,
) {
    SlotBidRevealed { group_id, round, bidder, desired_slot, bid_amount }.publish(e);
}

pub fn emit_sealed_auction_settled(
    e: &Env,
    group_id: u32,
    round: u32,
    winner: Address,
    winning_bid: i128,
) {
    SealedAuctionSettled { group_id, round, winner, winning_bid }.publish(e);
}


// --- Slot Auction Events (open auction variant) ---

/// Event: A bid was placed in an open slot auction
#[contractevent]
#[derive(Clone, Debug)]
pub struct SlotBidPlaced {
    pub group_id: u32,
    pub bidder: Address,
    pub desired_slot: u32,
    pub bid_amount: i128,
}

pub fn emit_slot_bid_placed(e: &Env, group_id: u32, bidder: Address, desired_slot: u32, bid_amount: i128) {
    SlotBidPlaced { group_id, bidder, desired_slot, bid_amount }.publish(e);
}

/// Event: An open slot auction was resolved
#[contractevent]
#[derive(Clone, Debug)]
pub struct SlotAuctionResolved {
    pub group_id: u32,
    pub winner: Address,
    pub slot: u32,
    pub winning_bid: i128,
    pub bonus_per_member: i128,
}

pub fn emit_slot_auction_resolved(e: &Env, group_id: u32, winner: Address, slot: u32, winning_bid: i128, bonus_per_member: i128) {
    SlotAuctionResolved { group_id, winner, slot, winning_bid, bonus_per_member }.publish(e);
}

// --- Cross-Group Migration Events ---

/// Event: A member requested migration to another group
#[contractevent]
#[derive(Clone, Debug)]
pub struct MigrationRequested {
    pub member: Address,
    pub src_contract: Address,
    pub to_group: Address,
}

pub fn emit_migration_requested(e: &Env, member: Address, src_contract: Address, to_group: Address) {
    MigrationRequested { member, src_contract, to_group }.publish(e);
}

/// Event: A member migration was executed into this group
#[contractevent]
#[derive(Clone, Debug)]
pub struct MigrationExecuted {
    pub member: Address,
    pub from_group: Address,
    pub dest_contract: Address,
    pub target_slot: u32,
}

pub fn emit_migration_executed(e: &Env, member: Address, from_group: Address, dest_contract: Address, target_slot: u32) {
    MigrationExecuted { member, from_group, dest_contract, target_slot }.publish(e);
}

// --- Proxy Authorization Events ---

/// Event: A proxy authorization has expired
#[contractevent]
#[derive(Clone, Debug)]
pub struct ProxyExpired {
    pub group_id: u32,
    pub member: Address,
    pub proxy: Address,
    pub expiry_ledger: u64,
}

pub fn emit_proxy_expired(e: &Env, group_id: u32, member: Address, proxy: Address, expiry_ledger: u64) {
    ProxyExpired { group_id, member, proxy, expiry_ledger }.publish(e);
}

// ── Co-payer Contribution Splitting Events ────────────────────────────────────

/// Event: Member registered co-payer splits for their contribution obligation.
#[contractevent]
#[derive(Clone, Debug)]
pub struct CoPayerSplitRegistered {
    pub member: Address,
    pub co_payer_count: u32,
    pub total_split_amount: i128,
}

/// Event: A co-payer contributed on behalf of a member.
#[contractevent]
#[derive(Clone, Debug)]
pub struct CoPayerContributed {
    pub member: Address,
    pub co_payer: Address,
    pub amount: i128,
    pub round: u32,
}

/// Event: Member's co-payer split registration was revoked.
#[contractevent]
#[derive(Clone, Debug)]
pub struct CoPayerSplitRevoked {
    pub member: Address,
}

pub fn emit_co_payer_split_registered(e: &Env, member: Address, co_payer_count: u32, total_split_amount: i128) {
    CoPayerSplitRegistered { member, co_payer_count, total_split_amount }.publish(e);
}

pub fn emit_co_payer_contributed(e: &Env, member: Address, co_payer: Address, amount: i128, round: u32) {
    CoPayerContributed { member, co_payer, amount, round }.publish(e);
}

pub fn emit_co_payer_split_revoked(e: &Env, member: Address) {
    CoPayerSplitRevoked { member }.publish(e);
}

// ── NFT-Style Contribution Receipt Events ─────────────────────────────────────

/// Event: Contribution receipt minted for a member after a completed round.
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContributionReceiptMinted {
    pub receipt_id: u32,
    pub member: Address,
    pub round: u32,
    pub amount_contributed: i128,
    pub receipt_hash: BytesN<32>,
}

pub fn emit_contribution_receipt_minted(
    e: &Env,
    receipt_id: u32,
    member: Address,
    round: u32,
    amount_contributed: i128,
    receipt_hash: BytesN<32>,
) {
    ContributionReceiptMinted {
        receipt_id,
        member,
        round,
        amount_contributed,
        receipt_hash,
    }
    .publish(e);
}
