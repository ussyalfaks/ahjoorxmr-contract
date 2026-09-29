use soroban_sdk::contracterror;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    AlreadyInitialized = 1,
    TokenNotApproved = 2,
    CustomOrderLengthMismatch = 3,
    CustomOrderNonMember = 4,
    AmountMustBePositive = 5,
    RoundDeadlinePassed = 6,
    MemberHasExited = 7,
    NotAMember = 8,
    AlreadyContributed = 9,
    InvalidExchangeRate = 10,
    ExceedsTokenLimit = 11,
    ExceedsRemainingContribution = 12,
    DeadlineNotPassed = 13,
    PenaltyDisabled = 14,
    NotADefaulter = 15,
    CannotChangeMidRound = 16,
    AlreadyAMember = 17,
    NoRewardsToClaim = 18,
    OnlyMembersAllowed = 19,
    ProposalNotFound = 20,
    VotingDeadlinePassed = 21,
    ProposalNotPending = 22,
    AlreadyVoted = 23,
    VotingNotEnded = 24,
    ContractPaused = 25,
    AllMembersSuspended = 26,
    AlreadyPaused = 27,
    NotPaused = 28,
    MemberAlreadyExited = 29,
    ExitRequestPending = 30,
    NoExitRequestFound = 31,
    ExitNotAllowedMidRound = 32,
    /// Contribution rejected because the round deadline has passed.
    ContributionWindowClosed = 33,
    /// Fee basis points exceeds maximum allowed (500 bps = 5%).
    FeeExceedsMaximum = 34,
    /// Max defaults must be at least 1.
    InvalidMaxDefaults = 35,
    /// Maximum members reached.
    GroupFull = 36,
    /// Invalid maximum member count (must be between 1 and 100).
    InvalidMaxMembers = 37,
    /// Delegation already exists for this delegator.
    DelegationAlreadyExists = 38,
    /// No delegation found for this delegator.
    NoDelegationFound = 39,
    /// Delegator cannot vote while delegation is active.
    CannotVoteWithActiveDelegation = 40,
    /// Delegate cannot further sub-delegate.
    CannotSubDelegate = 41,
    /// Invite not found or expired.
    InviteNotFound = 42,
    /// Invite has already been redeemed.
    InviteAlreadyRedeemed = 43,
    /// Invite is for a different address.
    InviteWrongRecipient = 44,
    /// Admin action not found.
    AdminActionNotFound = 45,
    /// Admin action has already been executed.
    AdminActionAlreadyExecuted = 46,
    /// Admin action has expired.
    AdminActionExpired = 47,
    /// Admin has already approved this action.
    AdminAlreadyApproved = 48,
    /// Insufficient approvals for admin action.
    InsufficientApprovals = 49,
    /// Not a co-admin.
    NotACoAdmin = 50,
}

/// Extension error codes 51-56 — split from Error because #[contracterror]
/// is bounded by the soroban XDR 50-case limit.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtError {
    /// Tier must be at least 1 bps.
    InvalidTier = 51,
    /// Insurance pool balance would go negative.
    InsurancePoolNegative = 52,
    /// Invalid insurance contribution amount.
    InvalidInsuranceContribution = 53,
    /// Member has reached the maximum allowed skips for the current cycle.
    SkipLimitReached = 54,
    /// Member has already requested a skip for this round.
    AlreadySkipped = 55,
    /// Member has zero contribution weight in weighted voting mode.
    InsufficientWeight = 56,
    /// Action requires admin privileges.
    OnlyAdminAllowed = 70,
    /// Invalid amount or index range.
    InvalidAmount = 71,
    /// Emergency payout already requested for this member in this cycle.
    EmergencyPayoutRequested = 57,
    /// Emergency payout quorum not met.
    EmergencyPayoutQuorumNotMet = 58,
    /// Emergency payout vote window expired.
    EmergencyPayoutVoteExpired = 59,
    /// Emergency payout already executed for this member in this cycle.
    EmergencyPayoutAlreadyExecuted = 60,
    /// Maximum emergency payouts per cycle reached.
    EmergencyPayoutLimitReached = 61,
    /// Group is already dissolved.
    GroupAlreadyDissolved = 62,
    /// Dissolution vote already in progress.
    DissolutionVoteInProgress = 63,
    /// Dissolution quorum not met.
    DissolutionQuorumNotMet = 64,
    /// Dissolution vote window expired.
    DissolutionVoteExpired = 65,
    /// No funds to distribute during dissolution.
    NoFundsToDistribute = 66,
    /// Invalid emergency payout configuration.
    InvalidEmergencyConfig = 67,
    /// Invalid dissolution configuration.
    InvalidDissolutionConfig = 68,
    /// Group start time is in the future.
    GroupNotYetActive = 69,
    /// Co-signer already set for this member.
    CoSignerAlreadySet = 72,
    /// No co-signer found for this member.
    NoCoSignerFound = 73,
    /// Co-signer has not accepted the designation.
    CoSignerNotAccepted = 74,
    /// Not the designated co-signer for this member.
    NotTheCoSigner = 75,
    /// Co-signer window has not opened (member has not defaulted).
    CoSignerWindowNotOpen = 76,
    /// Co-signer window has expired.
    CoSignerWindowExpired = 77,
    /// Group is frozen by contract-level admin pending investigation.
    GroupFrozen = 78,
    /// Group is not currently frozen.
    GroupNotFrozen = 79,
    /// Snapshot taken too soon; min_snapshot_interval_ledgers not elapsed (#243).
    SnapshotTooSoon = 80,
    /// Tier ID does not exist in this group's tier definitions (#267).
    TierNotFound = 81,
    /// Tier definition is invalid (e.g. zero contribution_amount or payout_weight) (#267).
    InvalidTierDefinition = 82,
    /// Member's credit score is below the group's minimum threshold (#269).
    InsufficientCreditScore = 83,
    /// Round duration is out of the configured bounds.
    RoundDurationOutOfBounds = 84,
    /// Contribution delegation has passed its expiry ledger (#330).
    DelegationExpired = 85,
    /// Caller is not the registered proxy for this member (#330).
    NotContribDelegate = 86,
    /// Split proposal not found (#331).
    SplitProposalNotFound = 87,
    /// Member list for split is invalid (overlap or missing members) (#331).
    SplitMembersInvalid = 88,
    /// Split confirmation window has closed (#331).
    SplitConfirmationWindowClosed = 89,
    /// Group has already been split (#331).
    SourceGroupAlreadySplit = 90,
    /// Member already confirmed split participation (#331).
    SplitAlreadyConfirmed = 91,
    /// Not all members have confirmed; cannot execute split yet (#331).
    SplitNotFullyConfirmed = 92,
    /// Proxy has consumed all authorized rounds (#403).
    ProxyRoundsExhausted = 118,
}

/// Extension error codes 101+ — overflow from ExtError (50-variant limit).
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtError2 {
    /// Slot auction feature is not enabled.
    AuctionNotEnabled = 101,
    /// No auction is currently open.
    AuctionNotOpen = 102,
    /// Auction bidding window has closed.
    AuctionWindowClosed = 103,
    /// Contribution amount does not match the required amount.
    IncorrectContributionAmount = 104,
    /// Slot index is out of range.
    InvalidSlotIndex = 105,
    /// Migration has already been executed.
    MigrationAlreadyExecuted = 106,
    /// A migration request is already pending for this member.
    MigrationAlreadyPending = 107,
    /// Migration has not been approved by the target group.
    MigrationNotApproved = 108,
    /// No migration request found for this member.
    MigrationNotFound = 109,
    /// No bid found for the given criteria.
    NoBidFound = 110,
    /// Target slot is already occupied by another member.
    SlotOccupied = 111,
    /// Token mismatch between source and target groups.
    TokenMismatch = 112,
    /// Member already has an outstanding emergency loan.
    OutstandingLoanExists = 113,
    /// No co-payer splits registered for this member.
    NoCopayersRegistered = 114,
    /// Co-payer split amounts do not sum to the required contribution amount.
    CopayerAmountsMismatch = 115,
    /// Contribution receipt not found for the given ID.
    ReceiptNotFound = 116,
    /// Member has already registered co-payer splits; revoke first.
    CopayerSplitsAlreadySet = 117,
    /// close_round has been called too many times in a row without an
    /// intervening finalize_round — the pot must be paid out (and audit
    /// trail/receipts recorded) before the round can advance again.
    RoundPendingFinalization = 119,
    /// Slot swap not found for the given id (#748).
    SwapNotFound = 120,
    /// Voluntary exit notice period has not elapsed yet (#792).
    ExitNoticeNotElapsed = 121,
    /// No pending voluntary exit request for this member (#792).
    NoVoluntaryExitRequest = 122,
    /// Member already has a pending voluntary exit request (#792).
    VoluntaryExitPending = 123,
    /// num_rounds must be positive for prepay (#790).
    InvalidPrepayRounds = 124,
    /// Insufficient prepaid balance to withdraw (#790).
    InsufficientPrepaidBalance = 125,
    // ── Scoped Co-Admin Role ──────────────────────────────────────────────────
    /// The caller is not a co-admin of this group.
    NotACoAdminRole = 126,
    /// The co-admin does not hold the required permission for this action.
    CoAdminPermissionDenied = 127,
    /// The address is already registered as a co-admin.
    CoAdminAlreadyExists = 128,
    /// No co-admin record found for this address.
    CoAdminNotFound = 129,
    /// The permissions list provided is empty; a co-admin must have at least one permission.
    CoAdminEmptyPermissions = 130,
    // ── Concurrent Group Membership Cap ──────────────────────────────────────
    /// The address has reached its concurrent active group membership cap.
    MembershipCapReached = 131,
    /// The membership cap value is out of the allowed range (0 = unlimited, max 255).
    InvalidMembershipCap = 132,
    // ── Group Cloning ─────────────────────────────────────────────────────────
    /// The source contract address provided for cloning is invalid (same as this contract).
    CloneSourceInvalid = 133,
    // ── Payout Beneficiary Nomination ─────────────────────────────────────────
    /// Beneficiary changes are locked while the member's payout round is in progress.
    BeneficiaryLocked = 134,
    /// The beneficiary address is invalid (e.g. the contract itself).
    InvalidBeneficiary = 135,
    // ── Group Charter ─────────────────────────────────────────────────────────
    /// No charter has been set for this group.
    CharterNotSet = 136,
    /// The acknowledged version does not match the current charter version.
    CharterVersionMismatch = 137,
    /// The member has not acknowledged the current charter version.
    CharterNotAcknowledged = 138,
    // ── Membership Succession ─────────────────────────────────────────────────
    /// Successor must not be the member itself or an existing member of the group.
    InvalidSuccessor = 139,
    /// No successor designation exists for this member / successor pair.
    SuccessorNotDesignated = 140,
    /// The designated successor has already accepted.
    SuccessionAlreadyAccepted = 141,
    /// The designated successor has not accepted yet.
    SuccessionNotAccepted = 142,
    /// The member has not missed enough consecutive contributions yet.
    SuccessionThresholdNotMet = 143,
    /// Succession trigger rounds must be positive.
    InvalidSuccessionTrigger = 144,
}

/// Extension error codes 151+ — overflow from ExtError2 (50-variant limit).
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExtError3 {
    // ── Contribution Streak Bonus ─────────────────────────────────────────────
    /// streak_bonus_bps must be between 0 and 10_000.
    InvalidStreakBonusBps = 151,
    /// The cycle has not completed yet, so no allocation exists.
    StreakCycleNotCompleted = 152,
    /// The member was not eligible for the cycle's streak bonus.
    StreakNotEligible = 153,
    /// The member already claimed the cycle's streak bonus.
    StreakBonusAlreadyClaimed = 154,
    // ── Payout Vesting ────────────────────────────────────────────────────────
    /// The member has no vesting record.
    NoVestingRecord = 155,
    /// Nothing has vested since the last claim.
    NothingVested = 156,
}
