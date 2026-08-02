# Contract Error Codes

Consolidated reference for every custom contract error exposed by the Ahjoor
smart contracts. Each error is produced via Soroban's `panic_with_error!`
mechanism and is identified by a numeric `u32` code.

> **Note on numbering.** Soroban's `#[contracterror]` macro caps each enum at
> 50 variants. Where a contract exceeded that limit the overflow was split into
> a separate `ExtError` / `ExtError2` enum with its own (non-contiguous) code
> range. Codes are therefore unique **per enum**, not globally.

---

## ahjoor-rosca

### `Error` (codes 1-50)

Defined in `contracts/ahjoor-rosca/src/errors.rs`.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | AlreadyInitialized | ahjoor-rosca | Contract/group has already been initialized. |
| 2 | TokenNotApproved | ahjoor-rosca | Token is not on the approved token whitelist. |
| 3 | CustomOrderLengthMismatch | ahjoor-rosca | Custom contribution order length does not match the member count. |
| 4 | CustomOrderNonMember | ahjoor-rosca | Custom order contains an address that is not a group member. |
| 5 | AmountMustBePositive | ahjoor-rosca | Contribution amount must be strictly positive. |
| 6 | RoundDeadlinePassed | ahjoor-rosca | The current round's deadline has already passed. |
| 7 | MemberHasExited | ahjoor-rosca | Member has already exited the group. |
| 8 | NotAMember | ahjoor-rosca | Caller is not a member of the group. |
| 9 | AlreadyContributed | ahjoor-rosca | Member has already contributed to the current round. |
| 10 | InvalidExchangeRate | ahjoor-rosca | Provided exchange rate is not an acceptable positive value. |
| 11 | ExceedsTokenLimit | ahjoor-rosca | Contribution exceeds the per-round token limit. |
| 12 | ExceedsRemainingContribution | ahjoor-rosca | Contribution exceeds the remaining amount due for the round. |
| 13 | DeadlineNotPassed | ahjoor-rosca | Operation requires the round deadline to have passed, but it has not. |
| 14 | PenaltyDisabled | ahjoor-rosca | Penalty mechanism is disabled for this group. |
| 15 | NotADefaulter | ahjoor-rosca | Member has not defaulted; penalty action not applicable. |
| 16 | CannotChangeMidRound | ahjoor-rosca | Configuration cannot be changed while a round is in progress. |
| 17 | AlreadyAMember | ahjoor-rosca | Address is already a member of the group. |
| 18 | NoRewardsToClaim | ahjoor-rosca | Member has no rewards available to claim. |
| 19 | OnlyMembersAllowed | ahjoor-rosca | Only group members may perform this action. |
| 20 | ProposalNotFound | ahjoor-rosca | Governance proposal not found. |
| 21 | VotingDeadlinePassed | ahjoor-rosca | The proposal's voting deadline has passed. |
| 22 | ProposalNotPending | ahjoor-rosca | Proposal is no longer in the pending state. |
| 23 | AlreadyVoted | ahjoor-rosca | Member has already voted on this proposal. |
| 24 | VotingNotEnded | ahjoor-rosca | Voting period has not yet ended. |
| 25 | ContractPaused | ahjoor-rosca | Contract is paused. |
| 26 | AllMembersSuspended | ahjoor-rosca | All members are currently suspended. |
| 27 | AlreadyPaused | ahjoor-rosca | Contract is already paused. |
| 28 | NotPaused | ahjoor-rosca | Contract is not paused; unpause is invalid. |
| 29 | MemberAlreadyExited | ahjoor-rosca | Member has already submitted an exit request. |
| 30 | ExitRequestPending | ahjoor-rosca | An exit request is already pending for this member. |
| 31 | NoExitRequestFound | ahjoor-rosca | No exit request found for this member. |
| 32 | ExitNotAllowedMidRound | ahjoor-rosca | Exit is not allowed while a round is in progress. |
| 33 | ContributionWindowClosed | ahjoor-rosca | Contribution rejected because the round deadline has passed. |
| 34 | FeeExceedsMaximum | ahjoor-rosca | Fee basis points exceeds 500 bps (5%) maximum. |
| 35 | InvalidMaxDefaults | ahjoor-rosca | max_defaults must be at least 1. |
| 36 | GroupFull | ahjoor-rosca | Maximum members reached. |
| 37 | InvalidMaxMembers | ahjoor-rosca | Invalid maximum member count (must be between 1 and 100). |
| 38 | DelegationAlreadyExists | ahjoor-rosca | Delegation already exists for this delegator. |
| 39 | NoDelegationFound | ahjoor-rosca | No delegation found for this delegator. |
| 40 | CannotVoteWithActiveDelegation | ahjoor-rosca | Delegator cannot vote while delegation is active. |
| 41 | CannotSubDelegate | ahjoor-rosca | Delegate cannot further sub-delegate. |
| 42 | InviteNotFound | ahjoor-rosca | Invite not found or expired. |
| 43 | InviteAlreadyRedeemed | ahjoor-rosca | Invite has already been redeemed. |
| 44 | InviteWrongRecipient | ahjoor-rosca | Invite is for a different address. |
| 45 | AdminActionNotFound | ahjoor-rosca | Admin action not found. |
| 46 | AdminActionAlreadyExecuted | ahjoor-rosca | Admin action has already been executed. |
| 47 | AdminActionExpired | ahjoor-rosca | Admin action has expired. |
| 48 | AdminAlreadyApproved | ahjoor-rosca | Admin has already approved this action. |
| 49 | InsufficientApprovals | ahjoor-rosca | Insufficient approvals for admin action. |
| 50 | NotACoAdmin | ahjoor-rosca | Caller is not a co-admin. |

### `ExtError` (codes 51-92, 118)

Overflow from `Error` due to the 50-variant `#[contracterror]` limit.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 51 | InvalidTier | ahjoor-rosca | Tier must be at least 1 bps. |
| 52 | InsurancePoolNegative | ahjoor-rosca | Insurance pool balance would go negative. |
| 53 | InvalidInsuranceContribution | ahjoor-rosca | Invalid insurance contribution amount. |
| 54 | SkipLimitReached | ahjoor-rosca | Member has reached the maximum allowed skips for the current cycle. |
| 55 | AlreadySkipped | ahjoor-rosca | Member has already requested a skip for this round. |
| 56 | InsufficientWeight | ahjoor-rosca | Member has zero contribution weight in weighted voting mode. |
| 57 | EmergencyPayoutRequested | ahjoor-rosca | Emergency payout already requested for this member in this cycle. |
| 58 | EmergencyPayoutQuorumNotMet | ahjoor-rosca | Emergency payout quorum not met. |
| 59 | EmergencyPayoutVoteExpired | ahjoor-rosca | Emergency payout vote window expired. |
| 60 | EmergencyPayoutAlreadyExecuted | ahjoor-rosca | Emergency payout already executed for this member in this cycle. |
| 61 | EmergencyPayoutLimitReached | ahjoor-rosca | Maximum emergency payouts per cycle reached. |
| 62 | GroupAlreadyDissolved | ahjoor-rosca | Group is already dissolved. |
| 63 | DissolutionVoteInProgress | ahjoor-rosca | Dissolution vote already in progress. |
| 64 | DissolutionQuorumNotMet | ahjoor-rosca | Dissolution quorum not met. |
| 65 | DissolutionVoteExpired | ahjoor-rosca | Dissolution vote window expired. |
| 66 | NoFundsToDistribute | ahjoor-rosca | No funds to distribute during dissolution. |
| 67 | InvalidEmergencyConfig | ahjoor-rosca | Invalid emergency payout configuration. |
| 68 | InvalidDissolutionConfig | ahjoor-rosca | Invalid dissolution configuration. |
| 69 | GroupNotYetActive | ahjoor-rosca | Group start time is in the future. |
| 70 | OnlyAdminAllowed | ahjoor-rosca | Action requires admin privileges. |
| 71 | InvalidAmount | ahjoor-rosca | Invalid amount or index range. |
| 72 | CoSignerAlreadySet | ahjoor-rosca | Co-signer already set for this member. |
| 73 | NoCoSignerFound | ahjoor-rosca | No co-signer found for this member. |
| 74 | CoSignerNotAccepted | ahjoor-rosca | Co-signer has not accepted the designation. |
| 75 | NotTheCoSigner | ahjoor-rosca | Not the designated co-signer for this member. |
| 76 | CoSignerWindowNotOpen | ahjoor-rosca | Co-signer window has not opened (member has not defaulted). |
| 77 | CoSignerWindowExpired | ahjoor-rosca | Co-signer window has expired. |
| 78 | GroupFrozen | ahjoor-rosca | Group is frozen by contract-level admin pending investigation. |
| 79 | GroupNotFrozen | ahjoor-rosca | Group is not currently frozen. |
| 80 | SnapshotTooSoon | ahjoor-rosca | Snapshot taken too soon; min_snapshot_interval_ledgers not elapsed (#243). |
| 81 | TierNotFound | ahjoor-rosca | Tier ID does not exist in this group's tier definitions (#267). |
| 82 | InvalidTierDefinition | ahjoor-rosca | Tier definition is invalid (e.g. zero contribution_amount or payout_weight) (#267). |
| 83 | InsufficientCreditScore | ahjoor-rosca | Member's credit score is below the group's minimum threshold (#269). |
| 84 | RoundDurationOutOfBounds | ahjoor-rosca | Round duration is out of the configured bounds. |
| 85 | DelegationExpired | ahjoor-rosca | Contribution delegation has passed its expiry ledger (#330). |
| 86 | NotContribDelegate | ahjoor-rosca | Caller is not the registered proxy for this member (#330). |
| 87 | SplitProposalNotFound | ahjoor-rosca | Split proposal not found (#331). |
| 88 | SplitMembersInvalid | ahjoor-rosca | Member list for split is invalid (overlap or missing members) (#331). |
| 89 | SplitConfirmationWindowClosed | ahjoor-rosca | Split confirmation window has closed (#331). |
| 90 | SourceGroupAlreadySplit | ahjoor-rosca | Group has already been split (#331). |
| 91 | SplitAlreadyConfirmed | ahjoor-rosca | Member already confirmed split participation (#331). |
| 92 | SplitNotFullyConfirmed | ahjoor-rosca | Not all members have confirmed; cannot execute split yet (#331). |
| 118 | ProxyRoundsExhausted | ahjoor-rosca | Proxy has consumed all authorized rounds (#403). |

### `ExtError2` (codes 101-117)

Overflow from `ExtError` due to the 50-variant `#[contracterror]` limit.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 101 | AuctionNotEnabled | ahjoor-rosca | Slot auction feature is not enabled. |
| 102 | AuctionNotOpen | ahjoor-rosca | No auction is currently open. |
| 103 | AuctionWindowClosed | ahjoor-rosca | Auction bidding window has closed. |
| 104 | IncorrectContributionAmount | ahjoor-rosca | Contribution amount does not match the required amount. |
| 105 | InvalidSlotIndex | ahjoor-rosca | Slot index is out of range. |
| 106 | MigrationAlreadyExecuted | ahjoor-rosca | Migration has already been executed. |
| 107 | MigrationAlreadyPending | ahjoor-rosca | A migration request is already pending for this member. |
| 108 | MigrationNotApproved | ahjoor-rosca | Migration has not been approved by the target group. |
| 109 | MigrationNotFound | ahjoor-rosca | No migration request found for this member. |
| 110 | NoBidFound | ahjoor-rosca | No bid found for the given criteria. |
| 111 | SlotOccupied | ahjoor-rosca | Target slot is already occupied by another member. |
| 112 | TokenMismatch | ahjoor-rosca | Token mismatch between source and target groups. |
| 113 | OutstandingLoanExists | ahjoor-rosca | Member already has an outstanding emergency loan. |
| 114 | NoCopayersRegistered | ahjoor-rosca | No co-payer splits registered for this member. |
| 115 | CopayerAmountsMismatch | ahjoor-rosca | Co-payer split amounts do not sum to the required contribution amount. |
| 116 | ReceiptNotFound | ahjoor-rosca | Contribution receipt not found for the given ID. |
| 117 | CopayerSplitsAlreadySet | ahjoor-rosca | Member has already registered co-payer splits; revoke first. |

### `SavingsGoalError` (codes 1-13)

Defined in `contracts/ahjoor-rosca/src/savings_goal_tracking.rs` as part of the
savings-goal tracking feature of the ROSCA contract.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | GoalNotFound | ahjoor-rosca | Savings goal not found. |
| 2 | GoalCompleted | ahjoor-rosca | Savings goal is already completed. |
| 3 | GoalAbandoned | ahjoor-rosca | Savings goal has been abandoned. |
| 4 | InvalidGoalAmount | ahjoor-rosca | Goal target amount is invalid (e.g. non-positive). |
| 5 | InvalidMilestone | ahjoor-rosca | Milestone definition is invalid. |
| 6 | MilestoneNotFound | ahjoor-rosca | Milestone not found for the given goal. |
| 7 | UnauthorizedAccess | ahjoor-rosca | Caller is not authorized to access this goal. |
| 8 | GoalExpired | ahjoor-rosca | Savings goal has passed its target date. |
| 9 | InvalidContribution | ahjoor-rosca | Contribution amount is invalid. |
| 10 | CelebrationFailed | ahjoor-rosca | On-chain milestone celebration failed. |
| 11 | RewardIssuanceFailed | ahjoor-rosca | Reward issuance for milestone failed. |
| 12 | InvalidGoalStatus | ahjoor-rosca | Goal is not in a valid status for this operation. |
| 13 | MilestoneAlreadyCompleted | ahjoor-rosca | Milestone has already been completed. |

---

## ahjoor-escrow

### `EscrowError` (codes 1-50)

Defined in `contracts/ahjoor-escrow/src/lib.rs`.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | InvalidDeadline | ahjoor-escrow | The supplied deadline is invalid (e.g. in the past or out of range). |
| 2 | InvalidTrancheIndex | ahjoor-escrow | Tranche index is out of range for the escrow. |
| 3 | TrancheAlreadyClaimed | ahjoor-escrow | Tranche has already been claimed by the beneficiary. |
| 4 | AlreadyInitialized | ahjoor-escrow | Already initialized. |
| 5 | AtLeastOneBuyerIsRequired | ahjoor-escrow | At least one buyer is required. |
| 6 | DeadlineMustBeFuture | ahjoor-escrow | Deadline must be in the future. |
| 7 | BuyerContributionMustBePositive | ahjoor-escrow | Buyer contribution must be positive. |
| 8 | DuplicateBuyerInList | ahjoor-escrow | Duplicate buyer in buyer list. |
| 9 | BatchMustContainAtLeastOneEscrowConfig | ahjoor-escrow | Batch must contain at least one escrow config. |
| 10 | BatchSizeExceedsMaximum10Escrows | ahjoor-escrow | Batch size exceeds maximum of 10 escrows. |
| 11 | OnlySellerCanMarkComplete | ahjoor-escrow | Only seller can mark complete. |
| 12 | EscrowIsNotActive | ahjoor-escrow | Escrow is not active. |
| 13 | NoInspectorSetUseReleaseEscrowDirectly | ahjoor-escrow | No inspector set; use release_escrow directly. |
| 14 | EscrowIsNotAwaitingInspection | ahjoor-escrow | Escrow is not awaiting inspection. |
| 15 | OnlyAssignedInspectorCanSubmitReport | ahjoor-escrow | Only the assigned inspector can submit a report. |
| 16 | OnlyBuyerOrSellerCanProposeInspectorReplacement | ahjoor-escrow | Only buyer or seller can propose inspector replacement. |
| 17 | NoInspectorSetEscrow | ahjoor-escrow | No inspector set on this escrow. |
| 18 | OnlyAdminCanSetInspectorScoreThreshold | ahjoor-escrow | Only admin can set inspector score threshold. |
| 19 | MinScoreBpsExceedsMaximum | ahjoor-escrow | Min_score_bps must be <= 10000. |
| 20 | OnlyAdminCanAppealInspectorRuling | ahjoor-escrow | Only admin can appeal inspector ruling. |
| 21 | InspectorRulingAlreadyAppealedEscrow | ahjoor-escrow | Inspector ruling already appealed for this escrow. |
| 22 | InspectorScoreBelowMinimumThresholdHighValueEscrow | ahjoor-escrow | Inspector score below minimum threshold for high-value escrow. |
| 23 | EscrowAmountMustBePositive | ahjoor-escrow | Escrow amount must be positive. |
| 24 | DeadlineMustBeAfterMinLockUntil | ahjoor-escrow | Deadline must be after min_lock_until. |
| 25 | ArbiterFeeExceedsMaximum1000Bps | ahjoor-escrow | Arbiter fee exceeds maximum of 1000 bps. |
| 26 | IncompleteReleaseCondition | ahjoor-escrow | Incomplete release condition. |
| 27 | ReleaseConditionThresholdMustBePositive | ahjoor-escrow | Release condition threshold must be positive. |
| 28 | InvalidReleaseComparison | ahjoor-escrow | Invalid release comparison. |
| 29 | Maximum5SellersAllowed | ahjoor-escrow | Maximum 5 sellers allowed. |
| 30 | SellerAllocationsMustSumTo10000Bps | ahjoor-escrow | Seller allocations must sum to 10000 bps. |
| 31 | DisputeTimeoutSecondsMustBePositive | ahjoor-escrow | Dispute_timeout_seconds must be positive. |
| 32 | OnlyCurrentHolderCanTransferReceipt | ahjoor-escrow | Only current holder can transfer receipt. |
| 33 | ActiveMilestoneInProgress | ahjoor-escrow | Active milestone in progress. |
| 34 | InspectionPending | ahjoor-escrow | InspectionPending: inspector must submit report before release. |
| 35 | OnlyListedBuyerCanApproveMultiBuyerRelease | ahjoor-escrow | Only a listed buyer can approve multi-buyer release. |
| 36 | BuyerHasAlreadyApprovedRelease | ahjoor-escrow | Buyer has already approved release. |
| 37 | OnlyBuyerOrArbiterCanReleaseEscrow | ahjoor-escrow | Only buyer or arbiter can release escrow. |
| 38 | SellerVetoActive | ahjoor-escrow | SellerVetoActive: admin must override_seller_veto before release. |
| 39 | SellerVetoActive2 | ahjoor-escrow | SellerVetoActive: seller has blocked fund release. |
| 40 | ConditionNotMet | ahjoor-escrow | Condition not met. |
| 41 | OnlyBuyerOrSellerCanWaiveCondition | ahjoor-escrow | Only buyer or seller can waive condition. |
| 42 | NoConditionalReleaseSetEscrow | ahjoor-escrow | No conditional release set for this escrow. |
| 43 | OnlyBuyerOrSellerCanSubmitEvidence | ahjoor-escrow | Only buyer or seller can submit evidence. |
| 44 | MaximumEvidenceEntriesReachedParty | ahjoor-escrow | Maximum evidence entries reached for this party. |
| 45 | OnlyBuyerCanSetRenewalAllowance | ahjoor-escrow | Only buyer can set renewal allowance. |
| 46 | AutoRenewIsNotEnabledEscrow | ahjoor-escrow | Auto-renew is not enabled for this escrow. |
| 47 | OnlyBuyerCanCancelAutoRenew | ahjoor-escrow | Only buyer can cancel auto-renew. |
| 48 | OnlyBuyerCanCancelAutoRenewal | ahjoor-escrow | Only buyer can cancel auto-renewal. |
| 49 | NoAutoRenewConfigSetEscrow | ahjoor-escrow | No AutoRenewConfig set on this escrow. |
| 50 | ReleaseAmountMustBePositive | ahjoor-escrow | Release amount must be positive. |

### `EscrowErrorExt` (codes 1-50)

Defined in `contracts/ahjoor-escrow/src/lib.rs`. Split from `EscrowError` because
`#[contracterror]` is bounded by the Soroban XDR 50-case limit.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | ReleaseAmountExceedsEscrowBalance | ahjoor-escrow | Release amount exceeds escrow balance. |
| 2 | AtLeastOneMilestoneRequired | ahjoor-escrow | At least one milestone required. |
| 3 | TooManyMilestones | ahjoor-escrow | Too many milestones. |
| 4 | MilestoneAmountMustBePositive | ahjoor-escrow | Milestone amount must be positive. |
| 5 | NewMilestonesMustStartAsPending | ahjoor-escrow | New milestones must start as Pending. |
| 6 | EscrowAlreadyTerminal | ahjoor-escrow | Escrow already terminal. |
| 7 | OnlyBuyerOrArbiterCanApproveMilestones | ahjoor-escrow | Only buyer or arbiter can approve milestones. |
| 8 | MilestoneIndexOutRange | ahjoor-escrow | Milestone index out of range. |
| 9 | MilestoneNotPending | ahjoor-escrow | Milestone not pending. |
| 10 | OnlyBuyerOrSellerCanDisputeEscrow | ahjoor-escrow | Only buyer or seller can dispute escrow. |
| 11 | DisputeAmountOutOfRange | ahjoor-escrow | Dispute_amount must be > 0 and <= escrow amount. |
| 12 | BuyerPercentMustBeBetween0And100 | ahjoor-escrow | Buyer_percent must be between 0 and 100. |
| 13 | EscrowIsNotDisputed | ahjoor-escrow | Escrow is not disputed. |
| 14 | OnlyArbiterCanResolveDispute | ahjoor-escrow | Only arbiter can resolve dispute. |
| 15 | EscrowIsNotCoolingOffState | ahjoor-escrow | Escrow is not in cooling-off state. |
| 16 | OnlyBuyerOrSellerCanFlagResolutionError | ahjoor-escrow | Only buyer or seller can flag a resolution error. |
| 17 | CoolingOffWindowHasExpired | ahjoor-escrow | Cooling-off window has expired. |
| 18 | ResolutionAlreadyFlagged | ahjoor-escrow | Resolution already flagged. |
| 19 | CoolingOffWindowHasNotElapsed | ahjoor-escrow | Cooling-off window has not elapsed. |
| 20 | ResolutionIsFlaggedAdminMustReviewBeforeFinalization | ahjoor-escrow | Resolution is flagged; admin must review before finalization. |
| 21 | OnlyAdminCanClearResolutionFlags | ahjoor-escrow | Only admin can clear resolution flags. |
| 22 | NoFlagToClear | ahjoor-escrow | No flag to clear. |
| 23 | OnlyAdminCanConfigureCoolingOffPeriod | ahjoor-escrow | Only admin can configure cooling-off period. |
| 24 | FeeConfigurationExceedsEscrowAmount | ahjoor-escrow | Fee configuration exceeds escrow amount. |
| 25 | TimeoutMustBePositive | ahjoor-escrow | Timeout must be positive. |
| 26 | MultiplierMustBePositive | ahjoor-escrow | Multiplier must be positive. |
| 27 | DeadlineMustBePositive | ahjoor-escrow | Deadline must be positive. |
| 28 | DisputeAlreadyResolved | ahjoor-escrow | Dispute already resolved. |
| 29 | DisputeTimeoutDeadlineHasNotPassedYet | ahjoor-escrow | Dispute timeout deadline has not passed yet. |
| 30 | MaxOracleAgeMustBePositive | ahjoor-escrow | Max_oracle_age must be positive. |
| 31 | InsuranceTriggerDaysMustBePositive | ahjoor-escrow | Insurance_trigger_days must be positive. |
| 32 | InsuranceContributionMustBePositive | ahjoor-escrow | Insurance contribution must be positive. |
| 33 | OnlyBuyerOrSellerCanClaimInsurance | ahjoor-escrow | Only buyer or seller can claim insurance. |
| 34 | InsuranceAlreadyClaimed | ahjoor-escrow | Insurance already claimed. |
| 35 | AdminConfirmationRequired | ahjoor-escrow | Admin confirmation required. |
| 36 | InsuranceTriggerPeriodNotReached | ahjoor-escrow | Insurance trigger period not reached. |
| 37 | EscrowTokenNotCoveredByInsurancePool | ahjoor-escrow | Escrow token not covered by insurance pool. |
| 38 | InsurancePoolHasInsufficientBalance | ahjoor-escrow | Insurance pool has insufficient balance. |
| 39 | FeeExceedsMaximum200Bps | ahjoor-escrow | Fee exceeds maximum of 200 bps. |
| 40 | WithdrawalAmountMustBePositive | ahjoor-escrow | Withdrawal amount must be positive. |
| 41 | InsufficientAccruedFees | ahjoor-escrow | Insufficient accrued fees. |
| 42 | ReleaseConditionNotMet | ahjoor-escrow | Release condition not met. |
| 43 | EscrowHasNotExpiredYet | ahjoor-escrow | Escrow has not expired yet. |
| 44 | OnlyBuyerOrSellerCanProposeDeadlineExtension | ahjoor-escrow | Only buyer or seller can propose deadline extension. |
| 45 | CannotExtendDeadlineWhileEscrowIsDisputed | ahjoor-escrow | Cannot extend deadline while escrow is disputed. |
| 46 | NewDeadlineMustBeGreaterThanCurrentDeadline | ahjoor-escrow | New deadline must be greater than current deadline. |
| 47 | OnlyBuyerOrSellerCanAcceptDeadlineExtension | ahjoor-escrow | Only buyer or seller can accept deadline extension. |
| 48 | ProposerCannotAcceptTheirOwnDeadlineExtension | ahjoor-escrow | Proposer cannot accept their own deadline extension. |
| 49 | DeadlineExtensionProposalHasExpired | ahjoor-escrow | Deadline extension proposal has expired. |
| 50 | OnlyBuyerOrSellerCanProposeAmendment | ahjoor-escrow | Only buyer or seller can propose amendment. |

### `EscrowErrorExt2` (codes 1-50)

Defined in `contracts/ahjoor-escrow/src/lib.rs`. Split from `EscrowError` because
`#[contracterror]` is bounded by the Soroban XDR 50-case limit.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | CannotAmendTerminalEscrow | ahjoor-escrow | Cannot amend terminal escrow. |
| 2 | NewAmountMustBePositive | ahjoor-escrow | New amount must be positive. |
| 3 | OnlyBuyerOrSellerCanSignAmendment | ahjoor-escrow | Only buyer or seller can sign amendment. |
| 4 | AmendmentNonceMismatch | ahjoor-escrow | Amendment nonce mismatch. |
| 5 | AmendmentProposalHasExpired | ahjoor-escrow | Amendment proposal has expired. |
| 6 | AmendmentRequiresBuyerAndSellerSignatures | ahjoor-escrow | Amendment requires buyer and seller signatures. |
| 7 | OnlyBuyerCanTopUpEscrow | ahjoor-escrow | Only buyer can top up escrow. |
| 8 | EscrowIsNotActiveOrAwaitingInspection | ahjoor-escrow | Escrow is not active or awaiting inspection. |
| 9 | AdditionalAmountMustBePositive | ahjoor-escrow | Additional amount must be positive. |
| 10 | TopUpLimitExceeded | ahjoor-escrow | Top-up limit exceeded. |
| 11 | OnlySellerCanAcknowledgeTopUp | ahjoor-escrow | Only seller can acknowledge top-up. |
| 12 | OnlySellerCanRequestPartialRelease | ahjoor-escrow | Only seller can request partial release. |
| 13 | PartialReleaseOnlyAllowedActiveEscrow | ahjoor-escrow | Partial release only allowed on active escrow. |
| 14 | PartialReleaseAmountMustBePositive | ahjoor-escrow | Partial release amount must be positive. |
| 15 | PartialReleaseAmountCannotExceedEscrowAmount | ahjoor-escrow | Partial release amount cannot exceed escrow amount. |
| 16 | RequestAlreadyPending | ahjoor-escrow | Request already pending. |
| 17 | OnlyBuyerCanApprovePartialRelease | ahjoor-escrow | Only buyer can approve partial release. |
| 18 | InvalidRequestID | ahjoor-escrow | Invalid request ID. |
| 19 | DelegateMustBeDifferentFromSeller | ahjoor-escrow | Delegate must be different from seller. |
| 20 | SellerNotPartOfEscrow | ahjoor-escrow | Seller is not part of this escrow. |
| 21 | CanOnlyDelegateBeforeEscrowIsReleased | ahjoor-escrow | Can only delegate before escrow is released. |
| 22 | OnlyBuyerCanRejectPartialRelease | ahjoor-escrow | Only buyer can reject partial release. |
| 23 | OnlySellerCanEscalatePartialRelease | ahjoor-escrow | Only seller can escalate partial release. |
| 24 | ResponseDeadlineNotYetPassed | ahjoor-escrow | Response deadline not yet passed. |
| 25 | NewBuyerMustBeDifferentFromCurrentBuyer | ahjoor-escrow | New buyer must be different from current buyer. |
| 26 | BuyerTransferOnlyAllowedActiveEscrows | ahjoor-escrow | Buyer transfer only allowed for active escrows. |
| 27 | OnlyCurrentBuyerCanTransferBuyerRole | ahjoor-escrow | Only current buyer can transfer buyer role. |
| 28 | OnlyBuyerOrSellerCanUpdateMetadata | ahjoor-escrow | Only buyer or seller can update metadata. |
| 29 | OnlyAdminCanUpgradeContract | ahjoor-escrow | Only admin can upgrade contract. |
| 30 | OnlyAdminCanMigrateContract | ahjoor-escrow | Only admin can migrate contract. |
| 31 | MigrationAlreadyCompletedVersion | ahjoor-escrow | Migration already completed for this version. |
| 32 | UnlockAtMustBeFuture | ahjoor-escrow | Unlock_at must be in the future. |
| 33 | AlreadyClaimed | ahjoor-escrow | Already claimed. |
| 34 | OnlyBeneficiaryCanClaim | ahjoor-escrow | Only beneficiary can claim. |
| 35 | UnlockTimeHasNotPassed | ahjoor-escrow | Unlock time has not passed. |
| 36 | EscrowNotActive | ahjoor-escrow | Escrow not active. |
| 37 | PastUnlockTimeUseClaimTimelocked | ahjoor-escrow | Past unlock time; use claim_timelocked. |
| 38 | OnlyBuyerCanCancel | ahjoor-escrow | Only buyer can cancel. |
| 39 | DisputeActive | ahjoor-escrow | Dispute active. |
| 40 | OnlyAdminCanSetTokenWhitelistContract | ahjoor-escrow | Only admin can set token whitelist contract. |
| 41 | ContractAlreadyPaused | ahjoor-escrow | Contract already paused. |
| 42 | ContractIsNotPaused | ahjoor-escrow | Contract is not paused. |
| 43 | TokenNotAllowed | ahjoor-escrow | Token not allowed. |
| 44 | DeadlineDurationMustBePositive | ahjoor-escrow | Deadline_duration must be positive. |
| 45 | TemplateIsDeactivated | ahjoor-escrow | Template is deactivated. |
| 46 | ArbiterAlreadyPool | ahjoor-escrow | Arbiter already in pool. |
| 47 | ArbiterNotPool | ahjoor-escrow | Arbiter not in pool. |
| 48 | ArbiterPoolIsEmpty | ahjoor-escrow | Arbiter pool is empty. |
| 49 | OnlyTemplateCreatorCanUpdate | ahjoor-escrow | Only template creator can update. |
| 50 | OnlyTemplateCreatorCanDeactivate | ahjoor-escrow | Only template creator can deactivate. |

### `EscrowErrorExt3` (codes 1-50)

Defined in `contracts/ahjoor-escrow/src/lib.rs`. Split from `EscrowError` because
`#[contracterror]` is bounded by the Soroban XDR 50-case limit.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | TemplateAlreadyDeactivated | ahjoor-escrow | Template already deactivated. |
| 2 | InactivityReleaseIsNotEnabledEscrow | ahjoor-escrow | Inactivity release is not enabled for this escrow. |
| 3 | OnlyEscrowSellerCanClaimInactivityRelease | ahjoor-escrow | Only the escrow seller can claim inactivity release. |
| 4 | BuyerInactivityWindowHasNotElapsed | ahjoor-escrow | Buyer inactivity window has not elapsed. |
| 5 | PenaltyCannotExceed10000Bps | ahjoor-escrow | Penalty cannot exceed 10000 bps. |
| 6 | ResponseWindowMustBePositive | ahjoor-escrow | Response window must be positive. |
| 7 | OnlyBuyerOrSellerCanRequestCancellation | ahjoor-escrow | Only buyer or seller can request cancellation. |
| 8 | NoPendingCancellationEscrow | ahjoor-escrow | No pending cancellation for this escrow. |
| 9 | InitiatorCannotAcceptTheirOwnCancellationRequest | ahjoor-escrow | Initiator cannot accept their own cancellation request. |
| 10 | OnlyBuyerOrSellerCanAcceptCancellation | ahjoor-escrow | Only buyer or seller can accept cancellation. |
| 11 | InitiatorCannotRejectTheirOwnCancellationRequest | ahjoor-escrow | Initiator cannot reject their own cancellation request. |
| 12 | OnlyBuyerOrSellerCanRejectCancellation | ahjoor-escrow | Only buyer or seller can reject cancellation. |
| 13 | ResponseWindowHasNotElapsed | ahjoor-escrow | Response window has not elapsed. |
| 14 | BountyAmountMustBePositive | ahjoor-escrow | Bounty amount must be positive. |
| 15 | ClaimDeadlineMustBeFuture | ahjoor-escrow | Claim deadline must be in the future. |
| 16 | SubmissionDeadlineMustBeAfterClaimDeadline | ahjoor-escrow | Submission deadline must be after claim deadline. |
| 17 | TokenNotWhitelisted | ahjoor-escrow | Token not whitelisted. |
| 18 | BountyIsNotAvailableClaiming | ahjoor-escrow | Bounty is not available for claiming. |
| 19 | ClaimDeadlineHasPassed | ahjoor-escrow | Claim deadline has passed. |
| 20 | BountyIsNotClaimedStatus | ahjoor-escrow | Bounty is not in claimed status. |
| 21 | OnlyAssignedSolverCanSubmitWork | ahjoor-escrow | Only the assigned solver can submit work. |
| 22 | SubmissionDeadlineHasPassed | ahjoor-escrow | Submission deadline has passed. |
| 23 | OnlyBuyerCanApproveSubmission | ahjoor-escrow | Only buyer can approve submission. |
| 24 | NoSubmissionHasBeenMade | ahjoor-escrow | No submission has been made. |
| 25 | OnlyBuyerCanRejectSubmission | ahjoor-escrow | Only buyer can reject submission. |
| 26 | MaximumRejectionRoundsReached | ahjoor-escrow | Maximum rejection rounds reached. |
| 27 | OnlyBuyerCanCancelBounty | ahjoor-escrow | Only buyer can cancel bounty. |
| 28 | CannotCancelBountyCurrentState | ahjoor-escrow | Cannot cancel bounty in current state. |
| 29 | OnlyAdminCanSetMaxBountyRejectionRounds | ahjoor-escrow | Only admin can set max bounty rejection rounds. |
| 30 | BountyMustHaveAtLeastOneMilestone | ahjoor-escrow | Bounty must have at least one milestone. |
| 31 | BountyMustBeClaimedBeforeSubmittingMilestones | ahjoor-escrow | Bounty must be claimed before submitting milestones. |
| 32 | OnlySolverCanSubmitMilestones | ahjoor-escrow | Only the solver can submit milestones. |
| 33 | MilestoneIndexOutBounds | ahjoor-escrow | Milestone index out of bounds. |
| 34 | PreviousMilestoneNotYetVerified | ahjoor-escrow | Previous milestone not yet verified. |
| 35 | MilestoneIsNotAwaitingSubmission | ahjoor-escrow | Milestone is not awaiting submission. |
| 36 | MilestoneIsNotAwaitingVerification | ahjoor-escrow | Milestone is not awaiting verification. |
| 37 | OnlyBountyCreatorCanReplaceVerifier | ahjoor-escrow | Only the bounty creator can replace a verifier. |
| 38 | VerifierCanOnlyBeReplacedBeforeMilestoneIsSubmitted | ahjoor-escrow | Verifier can only be replaced before the milestone is submitted. |
| 39 | OnlyBuyerCanConfigureCollateralHealth | ahjoor-escrow | Only buyer can configure collateral health. |
| 40 | MinCollateralRatioBpsOutOfRange | ahjoor-escrow | Min_collateral_ratio_bps must be 1-10000. |
| 41 | TopUpAmountMustBePositive | ahjoor-escrow | Top-up amount must be positive. |
| 42 | EscrowIsNotActiveOrUnderCollateralized | ahjoor-escrow | Escrow is not active or under-collateralized. |
| 43 | OnlyBuyerCanConfigureMultiPartyApproval | ahjoor-escrow | Only buyer can configure multi-party approval. |
| 44 | ApproversCountMustBeBetween2And10 | ahjoor-escrow | Approvers count must be between 2 and 10. |
| 45 | ThresholdMustBeBetween1AndApproversCount | ahjoor-escrow | Threshold must be between 1 and approvers count. |
| 46 | CannotReconfigureApprovalsAlreadyProgress | ahjoor-escrow | Cannot reconfigure: approvals already in progress. |
| 47 | CallerIsNotAuthorizedApproverEscrow | ahjoor-escrow | Caller is not an authorized approver for this escrow. |
| 48 | ApproverHasAlreadyApprovedEscrow | ahjoor-escrow | Approver has already approved this escrow. |
| 49 | ReleaseScheduleMustContainAtLeastOneTranche | ahjoor-escrow | Release schedule must contain at least one tranche. |
| 50 | EachTrancheAmountMustBePositive | ahjoor-escrow | Each tranche amount must be positive. |

### `EscrowErrorExt4` (codes 1-48)

Defined in `contracts/ahjoor-escrow/src/lib.rs`. Split from `EscrowError` because
`#[contracterror]` is bounded by the Soroban XDR 50-case limit.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | EachTrancheUnlockAtMustBeFuture | ahjoor-escrow | Each tranche unlock_at must be in the future. |
| 2 | OnlyBeneficiarySellerCanClaimScheduledReleases | ahjoor-escrow | Only the beneficiary (seller) can claim scheduled releases. |
| 3 | EscrowIsNotClaimableState | ahjoor-escrow | Escrow is not in a claimable state. |
| 4 | NoTranchesAreCurrentlyClaimable | ahjoor-escrow | No tranches are currently claimable. |
| 5 | ContractIsPaused | ahjoor-escrow | Contract is paused. |
| 6 | OnlyAdminCanPauseContract | ahjoor-escrow | Only admin can pause contract. |
| 7 | OnlyAdminCanResumeContract | ahjoor-escrow | Only admin can resume contract. |
| 8 | EscrowStillLocked | ahjoor-escrow | Escrow still locked. |
| 9 | OraclePriceIsStale | ahjoor-escrow | Oracle price is stale. |
| 10 | InvalidOraclePrice | ahjoor-escrow | Invalid oracle price. |
| 11 | InsufficientRenewalAllowance | ahjoor-escrow | Insufficient renewal allowance. |
| 12 | EscrowRenewalDurationMustBePositive | ahjoor-escrow | Escrow renewal duration must be positive. |
| 13 | OnlyAdminCanSetMaxTopUpBps | ahjoor-escrow | Only admin can set max top-up bps. |
| 14 | CollateralForfeitBpsCannotExceed10000 | ahjoor-escrow | Collateral_forfeit_bps cannot exceed 10000. |
| 15 | AtLeastOneSellerRequired | ahjoor-escrow | At least one seller required. |
| 16 | OnlySellerCanDepositCollateral | ahjoor-escrow | Only seller can deposit collateral. |
| 17 | EscrowIsNotAwaitingCollateral | ahjoor-escrow | Escrow is not awaiting collateral. |
| 18 | CollateralDepositWindowHasExpired | ahjoor-escrow | Collateral deposit window has expired. |
| 19 | RatingMustBeBetween1And5 | ahjoor-escrow | Rating must be between 1 and 5. |
| 20 | RatingOnlyAllowedAfterEscrowIsReleasedOrResolved | ahjoor-escrow | Rating only allowed after escrow is Released or Resolved. |
| 21 | OnlyBuyerOrSellerCanSubmitRating | ahjoor-escrow | Only buyer or seller can submit a rating. |
| 22 | RatingAlreadySubmittedEscrow | ahjoor-escrow | Rating already submitted for this escrow. |
| 23 | OnlySellerCanSubmitDeliveryProof | ahjoor-escrow | Only seller can submit delivery proof. |
| 24 | ProofSubmissionLockedEscrowIsUnderDispute | ahjoor-escrow | Proof submission locked: escrow is under dispute. |
| 25 | InvalidDeliveryProof | ahjoor-escrow | Invalid delivery proof. |
| 26 | OnlyEscrowSellerCanRaiseVeto | ahjoor-escrow | Only the escrow seller can raise a veto. |
| 27 | VetoCooldownActive | ahjoor-escrow | VetoCooldownActive: seller must wait for cooldown before raising another veto. |
| 28 | OnlyBuyerCanApprove | ahjoor-escrow | Only the buyer can approve. |
| 29 | NoPendingSellerTransfer | ahjoor-escrow | No pending seller transfer. |
| 30 | OnlyAdminCanSetVetoOverrideWindow | ahjoor-escrow | Only admin can set veto override window. |
| 31 | WindowSecondsMustBePositive | ahjoor-escrow | Window_seconds must be positive. |
| 32 | OnlyEscrowSellerCanCancelVeto | ahjoor-escrow | Only the escrow seller can cancel a veto. |
| 33 | VetoWindowElapsed | ahjoor-escrow | VetoWindowElapsed: override window has passed; only admin can act now. |
| 34 | OnlyAdminCanOverrideSellerVeto | ahjoor-escrow | Only admin can override a seller veto. |
| 35 | ActiveDisputeExists | ahjoor-escrow | ActiveDisputeExists: resolve the dispute before overriding veto. |
| 36 | VetoWindowNotElapsed | ahjoor-escrow | VetoWindowNotElapsed: override window has not yet passed. |
| 37 | VetoWindowHasNotExpiredYet | ahjoor-escrow | Veto window has not expired yet. |
| 38 | OnlyCurrentSellerCanInitiateTransfer | ahjoor-escrow | Only the current seller can initiate a transfer. |
| 39 | EscrowMustBeActiveToTransferSellerRole | ahjoor-escrow | Escrow must be active to transfer seller role. |
| 40 | OnlyBuyerCanVeto | ahjoor-escrow | Only the buyer can veto. |
| 41 | OnlyAdminCanSetVetoWindow | ahjoor-escrow | Only admin can set the veto window. |
| 42 | ReleaseBpsMustBePositiveEachMilestone | ahjoor-escrow | Release_bps must be positive for each milestone. |
| 43 | MilestoneReleaseBpsMustSumTo10000 | ahjoor-escrow | Milestone release_bps must sum to 10 000. |
| 44 | OnlyEscrowSellerMaySubmitMilestones | ahjoor-escrow | Only the escrow seller may submit milestones. |
| 45 | MilestoneMustBePendingOrRejectedToSubmit | ahjoor-escrow | Milestone must be Pending or Rejected to submit. |
| 46 | MilestoneMustBeSubmittedBeforeApproval | ahjoor-escrow | Milestone must be Submitted before approval. |
| 47 | OnlyEscrowBuyerMayRejectMilestones | ahjoor-escrow | Only the escrow buyer may reject milestones. |
| 48 | OnlySubmittedMilestoneCanBeRejected | ahjoor-escrow | Only a Submitted milestone can be rejected. |

---

## ahjoor-payments

### `Error` (codes 1-60)

Defined in `contracts/ahjoor-payments/src/lib.rs`.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | RateLimitExceeded | ahjoor-payments | Operation exceeded the configured rate limit. |
| 2 | SubscriptionPaused | ahjoor-payments | Subscription is paused. |
| 3 | OracleConditionNotMet | ahjoor-payments | Required oracle price condition was not met. |
| 4 | SubscriptionInTrial | ahjoor-payments | Subscription's trial period has not elapsed; charging is deferred (#133). |
| 5 | TokenNotAllowed | ahjoor-payments | Token is not on the allowed list for this merchant. |
| 6 | DuplicateExternalId | ahjoor-payments | A payment with this external ID already exists. |
| 7 | MultisigNotRequired | ahjoor-payments | Operation expected a multisig requirement that is not configured. |
| 8 | AlreadyApproved | ahjoor-payments | Payment/spending has already been approved. |
| 9 | NotASigner | ahjoor-payments | Caller is not a registered signer. |
| 10 | VoucherExpired | ahjoor-payments | Voucher has expired. |
| 11 | VoucherExhausted | ahjoor-payments | Voucher has been fully redeemed. |
| 12 | VoucherRevoked | ahjoor-payments | Voucher has been revoked. |
| 13 | VoucherNotFound | ahjoor-payments | Voucher not found. |
| 14 | WithdrawalRateLimitExceeded | ahjoor-payments | Merchant withdrawal rate limit exceeded. |
| 15 | ReferralAlreadyExists | ahjoor-payments | Referred merchant already has a merchant record (#242). |
| 16 | NoCommissionToClaim | ahjoor-payments | No pending commission to claim (#242). |
| 17 | DynamicPaymentExpired | ahjoor-payments | Dynamic payment has expired (#246). |
| 18 | TippingNotEnabled | ahjoor-payments | Tip supplied on a payment that does not have tipping_enabled (#265). |
| 19 | TipExceedsMaxBps | ahjoor-payments | Tip amount exceeds the admin-configured maximum tip bps of the base amount (#265). |
| 20 | MerchantVolumeCapped | ahjoor-payments | Merchant cumulative volume cap would be exceeded. |
| 21 | SlippageExceeded | ahjoor-payments | Slippage tolerance exceeded on dynamic payment settlement (#246). |
| 22 | OracleNotWhitelisted | ahjoor-payments | Oracle address is not on the admin whitelist (#246). |
| 23 | CustomerSpendLimitExceeded | ahjoor-payments | Customer cumulative spend would exceed the merchant-configured cap (#235). |
| 24 | CapturePastDeadline | ahjoor-payments | Capture attempted after the authorized capture deadline ledger. |
| 25 | EvidenceWindowClosed | ahjoor-payments | Evidence submission window has closed (#308). |
| 26 | EvidenceLimitReached | ahjoor-payments | Evidence submission limit reached for this party (#308). |
| 27 | CoolingOffExpired | ahjoor-payments | Cooling-off period has expired (#309). |
| 28 | NotInCoolingOff | ahjoor-payments | Payment is not in cooling-off status (#309). |
| 29 | CoolingOffExceedsMax | ahjoor-payments | Cooling-off period exceeds maximum allowed (#309). |
| 30 | PauseCountExceeded | ahjoor-payments | Subscription pause count exceeded (#327). |
| 31 | UnauthorizedPause | ahjoor-payments | Unauthorized to pause subscription (#327). |
| 32 | InsufficientMerchantReserve | ahjoor-payments | Merchant refund reserve is below the configured minimum (#334). |
| 33 | KYBVerificationRequired | ahjoor-payments | KYB verification required but merchant not verified (#310). |
| 34 | RetryNotDue | ahjoor-payments | retry_failed_debit called before back-off interval has elapsed (#329). |
| 35 | DebitRecordNotFound | ahjoor-payments | Failed debit record not found (#329). |
| 36 | DebitAlreadyAbandoned | ahjoor-payments | Debit record is already abandoned; no further retries (#329). |
| 37 | DebitAlreadySucceeded | ahjoor-payments | Debit record already succeeded; no retry needed (#329). |
| 38 | InvalidPaymentStatus | ahjoor-payments | Payment is not in a pending state and cannot be extended. |
| 39 | MaxExtensionsReached | ahjoor-payments | Maximum number of extensions reached for this payment. |
| 40 | MaxExtensionLedgersExceeded | ahjoor-payments | Additional ledgers exceed the maximum allowed per extension. |
| 50 | CustomerBlocked | ahjoor-payments | Customer is blocked by merchant. |
| 51 | DaoNotConfigured | ahjoor-payments | DAO mediation has not been configured by admin. |
| 52 | NotADaoMember | ahjoor-payments | Caller is not a registered DAO mediator member. |
| 53 | DaoAlreadyEscalated | ahjoor-payments | Payment dispute has already been escalated to the DAO. |
| 54 | DaoVoteWindowOpen | ahjoor-payments | DAO vote window is still open; verdict cannot be executed yet. |
| 55 | DaoVoteWindowClosed | ahjoor-payments | DAO vote window has closed; no further votes accepted. |
| 56 | DaoAlreadyVoted | ahjoor-payments | This DAO member has already cast a vote on this case. |
| 57 | DaoCaseAlreadyExecuted | ahjoor-payments | DAO verdict has already been executed for this case. |
| 58 | DaoMinVotesNotMet | ahjoor-payments | Minimum number of DAO votes has not been reached. |
| 59 | PaymentNotDisputed | ahjoor-payments | Payment is not in Disputed status; cannot escalate. |
| 60 | MerchantKYBExpired | ahjoor-payments | Merchant has KYB on record, but it is now expired. |

### `ExtError` (code 71)

Overflow from `Error` due to the 50-variant `#[contracterror]` limit.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 71 | InvalidAmount | ahjoor-payments | Batch size exceeds the allowed cap or amount is out of range. |

### `MultiTokenInvoiceError` (codes 1-13)

Defined in `contracts/ahjoor-payments/src/multi_token_invoice.rs` as part of the
multi-token invoice feature.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | InvoiceNotFound | ahjoor-payments | Invoice not found. |
| 2 | InvalidInvoiceStatus | ahjoor-payments | Invoice is not in a valid status for this operation. |
| 3 | PaymentExceedsInvoiceAmount | ahjoor-payments | Payment amount exceeds the remaining invoice balance. |
| 4 | TokenNotAccepted | ahjoor-payments | Payment token is not in the invoice's accepted token list. |
| 5 | ConversionRateNotSet | ahjoor-payments | Required conversion rate has not been set. |
| 6 | InvoiceExpired | ahjoor-payments | Invoice has passed its due date. |
| 7 | UnauthorizedAccess | ahjoor-payments | Caller is not authorized to perform this action on the invoice. |
| 8 | InvalidLineItem | ahjoor-payments | Invoice line item is invalid. |
| 9 | SettlementFailed | ahjoor-payments | Settlement processing failed. |
| 10 | InvalidConversionRate | ahjoor-payments | Provided conversion rate is invalid. |
| 11 | SlippageExceeded | ahjoor-payments | Slippage exceeded during settlement. |
| 12 | OracleNotConfigured | ahjoor-payments | Oracle is not configured for this token pair. |
| 13 | OraclePriceUnavailable | ahjoor-payments | Oracle price is unavailable or stale. |

### `SpendingAllowanceError` (codes 1-14)

Defined in `contracts/ahjoor-payments/src/pre_approved_spending.rs` as part of
the pre-approved spending feature.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | AllowanceNotFound | ahjoor-payments | Spending allowance not found. |
| 2 | AllowanceExpired | ahjoor-payments | Spending allowance has expired. |
| 3 | AllowanceExhausted | ahjoor-payments | Spending allowance is fully exhausted. |
| 4 | AllowanceRevoked | ahjoor-payments | Spending allowance has been revoked. |
| 5 | TransactionExceedsLimit | ahjoor-payments | Transaction amount exceeds the allowance limit. |
| 6 | DailyLimitExceeded | ahjoor-payments | Daily spending limit exceeded. |
| 7 | PerTransactionLimitExceeded | ahjoor-payments | Per-transaction spending limit exceeded. |
| 8 | UnauthorizedAccess | ahjoor-payments | Caller is not authorized to use this allowance. |
| 9 | ConsentNotFound | ahjoor-payments | Consent record not found. |
| 10 | ConsentExpired | ahjoor-payments | Consent record has expired. |
| 11 | ConsentRevoked | ahjoor-payments | Consent record has been revoked. |
| 12 | InvalidConsentHash | ahjoor-payments | Consent hash does not match the on-chain record. |
| 13 | AllowancePaused | ahjoor-payments | Spending allowance is paused. |
| 14 | InvalidAllowanceAmount | ahjoor-payments | Allowance amount is invalid (e.g. non-positive). |

---

## ahjoor-refund

The `ahjoor-refund` contract does **not** define any custom `#[contracterror]`
enum. All failure modes are signaled via plain `panic!` with string literals
(e.g. `"PaymentContractError: payment not found"`), which are **not** exposed as
numeric error codes. See `contracts/ahjoor-refund/src/lib.rs`.

---

## ahjoor-token-whitelist

### `Error` (codes 1-8)

Defined in `contracts/ahjoor-token-whitelist/src/lib.rs`.

| Code | Name | Contract | Description |
| ---- | ---- | -------- | ----------- |
| 1 | NotInitialized | ahjoor-token-whitelist | Contract has not been initialized. |
| 2 | AlreadyInitialized | ahjoor-token-whitelist | Contract has already been initialized. |
| 3 | Unauthorized | ahjoor-token-whitelist | Caller is not authorized to perform this action. |
| 4 | TokenAlreadyWhitelisted | ahjoor-token-whitelist | Token is already on the whitelist. |
| 5 | TokenNotWhitelisted | ahjoor-token-whitelist | Token is not on the whitelist. |
| 6 | QuotaExceeded | ahjoor-token-whitelist | Token volume quota for the current period has been exceeded. |
| 7 | TokenAlreadyHasQuota | ahjoor-token-whitelist | Token already has a quota configured. |
| 8 | TokenHasNoQuota | ahjoor-token-whitelist | Token has no quota configured. |
