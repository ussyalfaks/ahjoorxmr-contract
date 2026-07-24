use soroban_sdk::{contracttype, Address, BytesN, Map, String, Vec};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum PayoutStrategy {
    RoundRobin = 0,
    AdminAssigned = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum DistributionType {
    Equal = 0,
    Proportional = 1,
    Weighted = 2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum VotingMode {
    Equal = 0,
    WeightedByContributions = 1,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoscaConfig {
    pub strategy: PayoutStrategy,
    pub custom_order: Option<Vec<Address>>,
    pub penalty_amount: i128,
    pub exit_penalty_bps: u32,
    pub collective_goal: Option<i128>,
    pub member_goals: Option<Map<Address, i128>>,
    /// Protocol fee in basis points (e.g., 100 = 1%, 500 = 5%). Max 500 bps.
    pub fee_bps: u32,
    /// Address that receives protocol fees
    pub fee_recipient: Option<Address>,
    /// Number of consecutive missed rounds before suspension (default: 3)
    pub max_defaults: u32,
    /// Additional ledgers before penalties are applied after deadline (ledger-mode groups).
    pub grace_period_ledgers: u32,
    pub use_timestamp_schedule: bool,
    pub round_duration_seconds: u64,
    pub max_members: Option<u32>,
    pub skip_fee: i128,
    pub max_skips_per_cycle: u32,
    pub voting_mode: VotingMode,
    /// Late fee in basis points applied to contributions during the grace period.
    /// Collected from the late contributor and distributed to on-time members.
    /// 0 = no late fee (grace period is free). Max 1000 bps (10%).
    pub late_fee_bps: u32,
    /// Grace period duration in seconds (timestamp-based schedule).
    /// Used when use_timestamp_schedule = true. 0 = no grace period.
    pub grace_period_seconds: u64,
    /// Enable the slot auction mechanism for this group.
    /// When true, an auction opens at the start of each new cycle.
    pub auction_enabled: bool,
    /// Number of ledger timestamps (seconds) the bidding window stays open.
    /// Ignored when auction_enabled = false.
    pub auction_window_ledgers: u64,
    /// Enable verifiable on-chain payout order randomization (#315)
    pub randomize_payout_order: bool,
    /// Enable emergency reserve for this group (#313)
    pub reserve_enabled: bool,
    /// Surcharge percentage (bps) on each contribution routed to emergency reserve (#313)
    pub reserve_contribution_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupInfo {
    pub members: Vec<Address>,
    pub contribution_amount: i128,
    pub token: Address,
    pub current_round: u32,
    pub total_rounds: u32,
    pub paid_members: Vec<Address>,
    pub next_recipient: Address,
    /// Timestamp (seconds) by which all contributions for the current round must be received.
    pub round_deadline: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayoutRecord {
    pub recipient: Address,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExitRequest {
    pub member: Address,
    pub rounds_contributed: u32,
    /// Computed dynamically in `approve_exit` from rounds_contributed, payout history, and
    /// exit_penalty_bps; not stored at request time.
    pub refund_amount: i128,
    pub approved: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberStatus {
    pub is_member: bool,
    pub is_suspended: bool,
    pub is_exited: bool,
    pub contributions_this_round: i128,
    pub has_paid_this_round: bool,
    pub default_count: u32,
    pub lifetime_contributions: i128,
    pub claimable_rewards: i128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ProposalType {
    PenaltyAppeal = 0,
    RuleChange = 1,
    MemberRemoval = 2,
    MaxMembersUpdate = 3,
    Reinstatement = 4, // #218
    MemberFreeze = 5,  // Member-initiated emergency freeze
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ProposalStatus {
    Pending = 0,
    Approved = 1,
    Rejected = 2,
    Executed = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub id: u32,
    pub proposal_type: ProposalType,
    pub creator: Address,
    pub description: String,
    pub target_member: Address,
    pub votes_for: i128,
    pub votes_against: i128,
    pub created_at: u64,
    pub deadline: u64,
    pub status: ProposalStatus,
    pub execution_data: Option<i128>,
    pub required_quorum: u32, // bps (e.g. 5100 = 51%)
}

/// Storage key classification:
///
/// INSTANCE (config + active round state — bounded, shared TTL):
///   Admin, Members, PayoutOrder, Strategy, ContributionAmt, Token,
///   CurrentRound, PaidMembers, RoundDuration, RoundDeadline, Defaulters,
///   PenaltyAmount, DefaultCount, SuspendedMembers, ApprovedTokens,
///   RewardPool, TotalParticipations, MemberParticipation, ClaimedRewards,
///   RewardWeights, RewardDistType, ExitedMembers, ExitPenaltyBps,
///   IsPaused, PauseReason, PauseTimestamp, CollectiveGoal, TotalCollected,
///   MemberGoals, MemberCollected, MilestonesReached, ExchangeRates,
///   TokenLimits, ProposalCounter, Proposals, ProposalVotes,
///   VotingDeadline, QuorumPercentage, MemberContributions
///
/// PERSISTENT (unbounded growth — individual TTL per key):
///   RoundHistory — appended every round; must outlive instance TTL
///
/// TEMPORARY (short-lived in-progress state — auto-expires):\
///   ExitRequests — pending admin approval; no long-term retention needed
#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    // --- Instance ---
    Admin,                   // Address
    Members,                 // Vec<Address>
    PayoutOrder,             // Vec<Address>
    Strategy,                // PayoutStrategy
    ContributionAmt,         // i128
    Token,                   // Address
    CurrentRound,            // u32
    PaidMembers,             // Vec<Address>
    RoundDuration,           // u64
    RoundDeadline,           // u64
    Defaulters,              // Vec<Address>
    PenaltyAmount,           // i128
    DefaultCount,            // Map<Address, u32>
    SuspendedMembers,        // Vec<Address>
    ApprovedTokens,          // Vec<Address>
    RewardPool,              // i128
    TotalParticipations,     // u32
    MemberParticipation,     // Map<Address, u32>
    ClaimedRewards,          // Map<Address, i128>
    RewardWeights,           // Map<Address, u32>
    RewardDistType,          // DistributionType
    ExitedMembers,           // Vec<Address>
    ExitPenaltyBps,          // u32 (basis points, e.g. 1000 = 10%)
    Paused,                  // bool (global pause alias)
    IsPaused,                // bool
    PauseReason,             // String
    PauseTimestamp,          // u64
    CollectiveGoal,          // i128
    TotalCollected,          // i128
    MemberGoals,             // Map<Address, i128>
    MemberCollected,         // Map<Address, i128>
    MilestonesReached,       // Vec<u32> (e.g. 25, 50, 75, 100)
    ExchangeRates,           // Map<Address, i128>
    TokenLimits,             // Map<Address, i128>
    ProposalCounter,         // u32
    Proposals,               // Map<u32, Proposal>
    ProposalVotes,           // Map<u32, Map<Address, bool>>
    VotingDeadline,          // u64
    QuorumPercentage,        // u32 (e.g., 51 for 51%)
    MemberContributions,     // Map<Address, i128> cumulative per round
    FeeBps,                  // u32 — protocol fee in basis points
    MaxDefaults,             // u32 — suspension threshold
    RoundDeadlineTimestamp,  // u64
    MaxMembers,              // u32
}

/// Overflow key enum — DataKey is capped at 50 variants by the soroban XDR limit.
/// Less-frequently-used instance keys go here.
#[derive(Clone)]
#[contracttype]
pub enum DataKey2 {
    // Preserved discriminants for keys moved from `DataKey` to maintain storage slot identity.
    ProposedAdmin = 40,      // Address — proposed new admin (pending acceptance)
    ContractVersion = 41,    // u32
    FeeRecipient = 43,       // Address — receives protocol fees
    UseTimestampSchedule = 45,    // bool
    RoundDurationSeconds = 46,    // u64
    MemberTiers = 49,             // Map<Address, u32>

    InsurancePool = 50,
    InsuranceContributionBps = 51,
    SkipFee = 52,
    MaxSkipsPerCycle = 53,
    SkipRequests = 54,
    MemberSkips = 55,
    QuorumConfig = 56,
    VotingMode = 57,
    ReinvestPreference = 58,
    ExitRequests = 59,
    TokenWhitelistContract = 60,
    CycleRecords = 61,
    CycleRecordRetentionWindow = 62,
    ArchivedCycleRecords = 63,
    CycleStartTimestamps = 64,
    EmergencyPayoutConfig = 65,
    EmergencyPayoutRequests = 66,
    EmergencyPayoutVotes = 67,
    EmergencyPayoutCount = 68,
    EmergencyPayoutApproved = 69,
    GroupStatus = 70,
    DissolutionConfig = 71,
    DissolutionVotes = 72,
    DissolutionVoteCount = 73,
    DissolutionDeadline = 74,
    SlotSwapCounter = 75,
    SlotSwaps = 76,
    SlotSwapRequiresAdmin = 77,
    SlotSwapExpirySeconds = 78,
    InsuranceCoverageMode = 79,
    InsuranceClaims = 80,
    ReinstatementFee = 81,
    PendingReinstatementFee = 82,
    ActiveReinstatementProposal = 83,
    Waitlist = 84,
    CatchUpDebt = 85,
}

/// Overflow key enum for merge and round-duration keys (#230, #227).
#[derive(Clone)]
#[contracttype]
pub enum DataKey4 {
    MergeProposalCounter = 86,
    MergeProposals = 87,
    GroupMergedInto = 88,
    CycleBonusAmount = 89,
    PendingRoundDuration = 90,
    MinRoundDuration = 91,
    MaxRoundDuration = 92,
    StartAt = 93,
    GroupActivationEmitted = 94,
    GracePeriodLedgers = 95,
    PendingPenalties = 96,
    LastRoundDeadline = 97,
    CoSigners = 98,
    CoSignerWindowLedgers = 99,
}

/// Waitlist ordering mode (#456).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum WaitlistMode {
    /// First-in-first-out; earliest `joined_at` is promoted first (default).
    Fifo = 0,
    /// Highest `PersistentKey::ReputationScores` candidate is promoted first.
    ReputationWeighted = 1,
}

/// Overflow key enum — DataKey2 is capped at 50 variants by the soroban XDR limit.
#[derive(Clone)]
#[contracttype]
pub enum DataKey3 {
    // #315: Payout Order Randomization
    RandomizePayoutOrder,    // bool — enable randomization for this group
    PayoutOrderSeed,         // BytesN<32> — seed for Fisher-Yates shuffle
    PayoutOrderFinalized,    // bool — track if order has been finalized
    // #352: Contribution Rebalancing
    BasePoolTarget,          // i128 — immutable payout target per cycle (initial_members × contribution_amount)
    CoSignerWindowStart,     // Map<Address, u32> — member → ledger when window opened (#240)
    ProxyAuthorizations,     // Map<(u32, Address), ProxyAuthorization> — (group_id, member)
    IsFrozen,                // bool — group is frozen by contract-level admin (#236)
    // #267: Tiered Contribution Levels
    GroupTiers,              // Vec<Tier> — named tier definitions
    MemberTierIndex,         // Map<Address, u32> — member → tier_id
    PendingTierChange,       // Map<Address, u32> — queued tier changes for next cycle
    // #269: On-Chain Member Credit Score
    ScoreWeights,            // ScoreWeights — admin-configurable scoring formula weights
    MinCreditScore,          // i128 — minimum score required to join this group
    // #398: Contribution-weight voting delegation
    ContribDelegations,      // Map<Address, ContribDelegationRecord>
    // Member freeze proposal context
    MemberFreezeReasons,     // Map<u32, BytesN<32>> — proposal_id -> freeze reason hash
    // #390: Timestamp-mode grace period
    GracePeriodSeconds,      // u64 — grace window in seconds (used when UseTimestampSchedule=true)
    // Reputation-gated fee discount
    RepFeeDiscount,
    // Slot Auction
    AuctionEnabled,
    AuctionWindowLedgers,
    AuctionOpenUntil,
    AuctionBids,
    AuctionRound,
    // Cross-Group Migration
    MigrationRequests,
    IncomingMigrations,
    MigratedMembers,
    VacantSlots,
    // #313: Emergency Liquidity Reserve
    ReserveEnabled,
    EmergencyReserveBalance,
    EmergencyLoanCounter,
    EmergencyLoan(u32),
    MemberOutstandingLoan(Address),
    // #314: Group treasury
    TreasuryConfig,
    TreasuryBalance,
    TreasuryRoundProposal(u32),
    TreasuryRoundVotes(u32, Address),
    // #331: Group Split
    SplitProposalCounter,
    SplitProposals,
    SplitConfirmationWindow,
    // #356: Penalty-Based Slot Demotion
    LateContributionCount,
    LateContribThreshold,
    // #359: Savings goal milestone reward pool
    SavingsRewardPool,
    SavingsMilestonesClaimed(u32, Address),
    // #375: Sealed-bid (commit-reveal) slot auction
    SealedAuction,
    SlotBidCommit(u32, Address),
    SealedCommitters(u32),
    SealedRevealedBids(u32),
    // Co-payer contribution splitting
    CoPayerSplits(Address),
    // NFT-style contribution receipts
    ContributionReceiptCounter,
    ContributionReceipt(u32),
    MemberReceiptIds,
    // #456: Waitlist priority ordering mode
    WaitlistPriorityMode,       // WaitlistMode — settable by admin
}

/// Overflow key enum — DataKey3 is capped at 50 variants by the soroban XDR limit.
#[derive(Clone)]
#[contracttype]
pub enum DataKey5 {
    // #454: Collective goal reward pool
    GoalRewardPool,             // i128 — funded by admin, distributed once collective_goal is reached
    GoalRewardDistributed,
}


// ── #330: Contribution Delegation ────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum ExpiryMode {
    Ledger = 0,
    Timestamp = 1,
}

/// Delegation record granting a proxy the right to act for a member.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContribDelegationRecord {
    pub proxy: Address,
    pub expiry: u64,
    pub expiry_mode: ExpiryMode,
}

// ── #331: Group Split ─────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum SplitProposalStatus {
    Pending = 0,
    Executed = 1,
    Expired = 2,
}

/// Proposal to divide one ROSCA group into two independent sub-groups.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SplitProposal {
    pub id: u32,
    pub group_a_members: Vec<Address>,
    pub group_b_members: Vec<Address>,
    pub split_reason_hash: BytesN<32>,
    pub confirmations: Vec<Address>,
    pub status: SplitProposalStatus,
    pub created_at_ledger: u32,
    pub expiry_ledger: u32,
}

/// Persistent storage keys — kept separate because DataKey was hitting
/// the 64-variant limit enforced by the `#[contracttype]` macro.
#[derive(Clone)]
#[contracttype]
pub enum PersistentKey {
    RoundHistory,              // Vec<PayoutRecord> — grows every round
    ReputationScores,          // Map<Address, i128> — cumulative member reliability score
    FreezeLog,                 // Vec<FreezeRecord> — append-only freeze audit log
    SnapshotLog,               // Vec<GroupSnapshot> — append-only snapshot log (#243)
    LastSnapshotLedger,        // u32 — last snapshot ledger for spam guard (#243)
    MinSnapshotIntervalLedgers, // u32 — min interval between snapshots (#243)
    MemberCreditScores,        // Map<Address, MemberScore> — per-member credit score (#269)
    /// #364: Point-in-time cycle snapshot keyed by cycle number
    CycleSnapshot(u32),        // cycle_number → CycleSnapshotData
    CreditScoreUpdatedAt(Address), // u32 — ledger sequence of last credit score update
    /// #448: Member contribution receipt IDs index
    MemberReceiptIds(Address), // Vec<u32> — list of receipt IDs for a member
}

/// #364: Immutable point-in-time snapshot of group state at cycle end.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CycleSnapshotData {
    pub cycle_number: u32,
    pub members: Vec<Address>,
    pub contribution_amounts: Map<Address, i128>,
    pub payout_queue: Vec<Address>,
    pub pool_balance: i128,
    pub timestamp: u64,
    pub snapshot_hash: BytesN<32>,
}

/// Record of a single freeze/unfreeze cycle for a group.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreezeRecord {
    pub frozen_at_ledger: u32,
    pub frozen_by: Address,
    pub reason_hash: BytesN<32>,
    pub unfrozen_at_ledger: Option<u32>,
    pub resolution_hash: Option<BytesN<32>>,
}

/// On-chain group state snapshot for immutable audit (#243).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupSnapshot {
    pub snapshot_id: u32,
    pub taken_at_ledger: u32,
    pub taken_by: Address,
    pub round_number: u32,
    pub pooled_balance: i128,
    pub member_statuses: Vec<MemberStatus>,
    pub payout_order: Vec<Address>,
    pub state_hash: BytesN<32>,
}

// #240: Co-Signer Guarantee

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum CoSignerStatus {
    Pending = 0,   // set by member, not yet accepted
    Active = 1,    // accepted by co-signer
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoSignerRecord {
    pub co_signer: Address,
    pub status: CoSignerStatus,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProxyAuthorization {
    pub proxy: Address,
    pub max_rounds: u32,
    pub used_rounds: u32,
}

// ── Audit Trail ────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionEntry {
    pub member: Address,
    pub amount: i128,
    pub timestamp: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CycleRecord {
    pub cycle_number: u32,
    pub total_pool_amount: i128,
    pub payout_recipient: Address,
    pub payout_amount: i128,
    pub contributions: Vec<ContributionEntry>,
    pub defaulters: Vec<Address>,
    pub skippers: Vec<Address>,
    pub penalties_collected: i128,
    pub fee_collected: i128,
    pub insurance_drawn: i128,
    pub cycle_start_timestamp: u64,
    pub cycle_end_timestamp: u64,
}

// --- Emergency Payout Types ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyPayoutRequest {
    pub requester: Address,
    pub reason_hash: BytesN<32>,
    pub created_at: u64,
    pub deadline: u64,
    pub votes_for: i128,
    pub votes_against: i128,
    pub executed: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyPayoutConfig {
    pub emergency_quorum_bps: u32,      // e.g., 6667 = 66.67%
    pub vote_window_seconds: u64,       // how long voting lasts
    pub max_emergency_per_cycle: u32,   // max emergency payouts per cycle
}

// --- Emergency Liquidity Reserve Types (#313) ---

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmergencyLoan {
    pub loan_id: u32,
    pub borrower: Address,
    pub amount: i128,
    pub created_at_ledger: u32,
    pub repayment_deadline_ledger: u32,
    pub repaid_amount: i128,
    pub defaulted: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum GroupStatus {
    Active = 0,
    Dissolved = 1,
    /// Group was merged into another group; all further interactions are rejected.
    Merged = 2,
    /// Group was split into two sub-groups; no further operations permitted.
    Split = 3,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DissolutionConfig {
    pub dissolution_quorum_bps: u32,    // e.g., 7500 = 75%
    pub vote_window_seconds: u64,
}

/// #230: Merge proposal between two ROSCA groups.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MergeProposal {
    pub id: u32,
    pub group_a_admin: Address,
    pub group_b_id: u32,
    pub proposed_at: u64,
    pub accepted: bool,
}

// #213: Payout Slot Swap
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum SlotSwapStatus {
    Pending = 0,
    Accepted = 1,
    Rejected = 2,
    Executed = 3,
    Expired = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotSwap {
    pub id: u32,
    pub initiator: Address,
    pub counterparty: Address,
    pub round_a: u32,
    pub round_b: u32,
    pub status: SlotSwapStatus,
    pub created_at: u64,
    pub expiry_at: u64,
    pub admin_approved: bool,
}

// #214: Insurance Coverage Mode & Claims
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum InsuranceCoverageMode {
    None = 0,
    Partial = 1,
    Full = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InsuranceClaim {
    pub round: u32,
    pub defaulter: Address,
    pub amount_covered: i128,
}

// ── #267: Tiered Contribution Levels ──────────────────────────────────────────

/// A contribution tier definition — name, fixed contribution amount, and payout weight.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tier {
    pub name: soroban_sdk::Symbol,
    pub contribution_amount: i128,
    pub payout_weight: u32,
}

// ── #269: On-Chain Member Credit Score ────────────────────────────────────────

/// Accumulated contribution-behaviour record for a member (#269).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemberScore {
    pub on_time_contributions: u32,
    pub late_contributions: u32,
    pub defaults: u32,
    pub early_exits: u32,
    pub groups_completed: u32,
    /// Computed numeric score derived from the above counters and ScoreWeights.
    pub score: i128,
}

/// Admin-configurable weights used to compute the credit score (#269).
/// Positive weights increase score; negative weights decrease it.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoreWeights {
    pub on_time_weight: i128,
    pub late_weight: i128,
    pub default_weight: i128,
    pub exit_weight: i128,
    pub completion_weight: i128,
}

// ── Slot Auction (#slot-auction) ──────────────────────────────────────────────

/// A single bid placed during a slot auction.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SlotBid {
    /// The member who placed this bid.
    pub bidder: Address,
    /// The payout-order slot index the bidder wants to move into.
    pub desired_slot: u32,
    /// Amount of base token deposited as the bid.
    pub amount: i128,
    /// Ledger timestamp at which the bid was placed (used for tie-breaking).
    pub placed_at: u64,
}

// ── #375: Sealed-Bid (Commit-Reveal) Slot Auction ─────────────────────────────

/// #375: Configuration and live phase state for a commit-reveal sealed-bid
/// slot auction. A single struct keeps the auction's tunables and the current
/// phase deadlines together under one storage key.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedAuctionState {
    /// Whether sealed-bid auctions are enabled for this group.
    pub enabled: bool,
    /// Duration (seconds) of the commit phase once an auction is opened.
    pub commit_duration: u64,
    /// Duration (seconds) of the reveal phase that follows the commit phase.
    pub reveal_duration: u64,
    /// Minimum reserve price; the winning bid must strictly exceed this.
    pub min_reserve: i128,
    /// Round this auction targets (meaningful only while `open`).
    pub round: u32,
    /// Timestamp at which the commit phase closes.
    pub commit_until: u64,
    /// Timestamp at which the reveal phase closes.
    pub reveal_until: u64,
    /// Whether an auction is currently open (awaiting settlement).
    pub open: bool,
}

/// #375: A stored commitment for a single sealed bid.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedCommit {
    /// sha256(bid_amount.to_be_bytes() || salt) committed during the commit phase.
    pub commit_hash: BytesN<32>,
    /// Collateral deposited at commit time; also the upper bound on the bid the
    /// bidder may later reveal.
    pub deposit: i128,
    /// Whether this commitment has already been revealed.
    pub revealed: bool,
}

// ── Cross-Group Member Migration ───────────────────────────────────────────────

/// Approval state for a pending cross-group migration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[contracttype]
pub enum MigrationApprovalState {
    /// Neither admin has approved yet.
    Pending = 0,
    /// Source admin approved; waiting for destination admin.
    SourceApproved = 1,
    /// Destination admin approved; waiting for source admin.
    DestApproved = 2,
    /// Both admins approved — ready to execute.
    BothApproved = 3,
    /// Migration has been executed.
    Executed = 4,
}

/// A pending cross-group migration request stored on the **source** contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigrationRequest {
    /// The member who wants to migrate.
    pub member: Address,
    /// Address of the destination group contract.
    pub to_group: Address,
    /// Slot index in the destination group's payout order.
    pub target_slot: u32,
    /// Approval state.
    pub state: MigrationApprovalState,
    /// Timestamp when the request was created.
    pub created_at: u64,
}

/// Contribution history summary carried from the source group to the destination.
/// Stored on the **destination** contract as a `MigratedMember` annotation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MigratedMemberRecord {
    /// Address of the source group contract.
    pub from_group: Address,
    /// Number of rounds the member fully completed in the source group.
    pub rounds_completed: u32,
    /// Number of on-time (full, non-late) contributions in the source group.
    pub on_time_count: u32,
    /// Slot index assigned in this (destination) group.
    pub slot_index: u32,
    /// Timestamp when the migration was executed.
    pub migrated_at: u64,
}

/// Incoming migration approval stored on the **destination** contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IncomingMigration {
    /// The member being migrated in.
    pub member: Address,
    /// Address of the source group contract.
    pub from_group: Address,
    /// Slot index to insert the member at.
    pub target_slot: u32,
    /// Whether the destination admin has approved.
    pub dest_approved: bool,
}

/// Group treasury configuration (#314)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryConfig {
    pub treasury_admin: Address,
    pub enabled: bool,
}

/// Treasury round proposal (#314)
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreasuryRoundProposal {
    pub round_index: u32,
    pub purpose_hash: BytesN<32>,
    pub proposed_at: u64,
    pub votes_for: i128,
    pub votes_against: i128,
    pub confirmed: bool,
}

/// Members whose `MemberScore.score` >= `threshold` pay `discount_bps` fewer
/// protocol-fee basis points on their payout round (floor: 0 bps).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepFeeDiscountConfig {
    pub threshold: i128,
    pub discount_bps: u32,
}

/// A single co-payer who covers part of a member's ROSCA contribution.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoPayerSplit {
    pub co_payer: Address,
    pub amount: i128,
}

/// NFT-style on-chain receipt minted when a round completes for each contributing member.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionReceipt {
    pub receipt_id: u32,
    pub member: Address,
    pub round: u32,
    pub amount_contributed: i128,
    pub token: Address,
    pub minted_at: u64,
    pub receipt_hash: BytesN<32>,
}

/// Aggregate read-only statistics returned by `get_group_analytics`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GroupAnalytics {
    pub total_members: u32,
    pub active_members: u32,
    pub suspended_count: u32,
    pub exited_count: u32,
    pub current_round: u32,
    pub total_rounds: u32,
    pub paid_this_round: u32,
    pub defaulters_this_round: u32,
    pub total_contributions_collected: i128,
    pub avg_credit_score: i128,
    pub avg_reputation_score: i128,
    pub fee_bps: u32,
}
