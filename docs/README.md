# Documentation

Welcome to the project documentation. This index links to the feature and
reference docs for the repository, grouped by contract.

## `ahjoor-payments`

- [**Payout Address Rotation**](payments-payout-address-rotation.md) — How merchants can securely change their payout address with a delay period for stolen-key protection.
- [**Failed Auto-Debit Retry Queue in ahjoor-payments**](payments-retry-queue.md) — Configurable exponential back-off retry queue for failed recurring debits, early customer retry trigger, max attempt handling, and recurring invoice cycle integration.
- [**Merchant Collateral in ahjoor-payments**](payments-collateral.md) — Overview of required collateral deposits, minimum balances, withdrawals, and dispute slashing.
- [**Merchant Referral Program in ahjoor-payments**](payments-referral.md) — Referral registration, commission accrual on referred-merchant fees, the commission window, and claiming accrued balances.
- [**Tipping in ahjoor-payments**](payments-tip-subscription.md) — Tip-enabled payments, the global basis-point tip cap, and merchant-configured tip splits across multiple recipients.
- [**Multi-Token Invoice in ahjoor-payments**](multi-token-invoice.md) — Invoices payable in any combination of accepted tokens, per-token conversion rates, partial payment tracking, and batch settlement.
- [**Merchant KYB Verification**](merchant-kyb.md) — Setting, renewing and revoking merchant KYB status, and how KYB enforcement gates merchant activity.
- [**DAO Mediation**](dao-mediation.md) — Routing unresolved payment disputes to a DAO mediator panel, member voting, and verdict execution after the voting window.

---

## `ahjoor-rosca`

- [**Group Charter Acknowledgement in ROSCA**](rosca-group-charter.md) — Charter versioning, pre- vs post-activation update flows, the governance proposal path for charter changes, and every entry point gated on member acknowledgement.
- [**Group Dissolution and Emergency Payout Voting in ROSCA**](rosca-dissolution-and-emergency-payout.md) — Admin-initiated and member-voted dissolution flows, pro-rata fund distribution on dissolution, vote thresholds, and the emergency payout request/vote/execute cycle.
- [**Contribution Receipts in ROSCA**](rosca-contribution-receipts.md) — NFT-style contribution receipt data format, automatic minting on round finalization, event emissions, and member retrieval/verification functions.
- [**Cosigner Guarantee in ROSCA**](rosca-cosigner-guarantee.md) — Overview of co-signer nomination, acceptance, and default coverage logic for community savings groups.
- [**Contribution Delegation in ROSCA**](rosca-contribution-delegation.md) — Overview of contribution-vote delegation, how it differs from general governance delegation, and how revocation restores direct voting.
- [**Co-Payer Contribution Splitting in ROSCA**](rosca-co-payer-splitting.md) — How co-payers are registered for a member's slot, how the split ratio is set and enforced, and what happens if a co-payer fails to contribute.
- [**ROSCA Governance Quorum Requirements**](rosca-governance-quorum.md) — Specification of per-ProposalType quorum thresholds, default percentages, admin overrides, resolution formulas, and administrative controls.
- [**ROSCA Group Split Flow**](rosca-group-split.md) — Proposal creation, member assignment and confirmation, expiry handling, execution, refunds, and resulting group identifiers.
- [**ROSCA Reinvestment Flow**](rosca-reinvestment.md) — How a member rolls a round payout forward into the next round as their contribution, the deadline constraint, and how over/under-payment is handled.
- [**ROSCA Slot Auctions**](rosca-slot-auctions.md) — Comprehensive guide to plain open-bid and commit-reveal sealed-bid slot auctions in Ahjoor ROSCA groups.
- [**ROSCA Round-Skip Mechanism**](rosca-skip-round.md) — Member eligibility, request flow, per-cycle skip limits, skip fee handling, and downstream settlement effects.
- [**ROSCA Emergency Loan**](rosca-emergency-loan.md) — How members can draw emergency loans from the group reserve, repayment terms, default handling, and reserve management.
- [**On-Chain Audit Trail in ROSCA**](rosca-audit-trail.md) — Comprehensive cycle audit trail recording, contribution history pagination, retention window management, and archival lifecycle.
- [**ROSCA Collective Savings Goals**](rosca-savings-goals.md) — Group-wide savings targets, progress tracking against the target, and goal finalization.
- [**ROSCA Savings Milestone Rewards**](rosca-savings-milestone-rewards.md) — Automatic token rewards when savings goal milestones are crossed.

---

## `ahjoor-escrow`

- [**Escrow Time-Based Release**](escrow-time-based-release.md) — Comparison of three time-driven release modes: time-locked escrows, inactivity-triggered release, and scheduled tranche releases.
- [**Open Bounty Board & Milestone Bounties**](bounty-board.md) — Open competitive work assignment flow, solver claiming, submission review, rejection rounds, cancellation refund, and milestone-gated verifier sign-offs.
- [**Escrow Auto-Renewal**](escrow-auto-renewal.md) — How buyers can pre-approve renewal cycles for recurring service agreements, how auto-renewals are triggered on release, and how buyers can cancel future renewals.
- [**Multi-Party Approval**](escrow-multiparty-approval.md) — N-of-M release approval configuration, threshold requirements, approver voting, and interactions with release and dispute flows.
- [**Dispute Timeout Enforcement**](escrow-dispute-timeout.md) — How stalled disputes are auto-resolved after a configurable timeout, per-escrow timeout overrides, default winner configuration, and the arbiter timeout counter.
- [**Seller Veto Mechanism**](escrow-seller-veto.md) — How sellers can veto a fund release within a cooldown window, how the cooldown interacts with repeated veto attempts, and how admin override resets the clock.
- [**Inspector Role**](inspector-role.md) — How a neutral inspector is assigned at escrow creation, the `submit_inspection_result` flow, dual-approval inspector replacement, and how the reputation score and threshold gate future assignments.

---

## `ahjoor-refund`

- [**Merchant Reserve Fund in ahjoor-refund**](reserve-fund.md) — Reserve requirements as a ratio of merchant payment volume, reserve deposits and withdrawals, and how approved refunds draw on the reserve.
