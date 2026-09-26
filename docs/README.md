# Documentation

Welcome to the project documentation. This index links to the feature and
reference docs for the repository.

## Feature docs

## `ahjoor-payments`

- [**Merchant KYB Verification**](merchant-kyb.md) — How a merchant's KYB status is set, renewed, and revoked, what `set_kyb_enforcement` gates at payment creation, and how `get_merchant_kyb_status` reports expiry.

## 2. ROSCA Contract (`ahjoor-rosca`)

- [**Contribution Receipts in ROSCA**](rosca-contribution-receipts.md) — NFT-style contribution receipt data format, automatic minting on round finalization, event emissions, and member retrieval/verification functions.
- [**Cosigner Guarantee in ROSCA**](rosca-cosigner-guarantee.md) — Overview of co-signer nomination, acceptance, and default coverage logic for community savings groups.
- [**Contribution Delegation in ROSCA**](rosca-contribution-delegation.md) — Overview of contribution-vote delegation, how it differs from general governance delegation, and how revocation restores direct voting.
- [**Co-Payer Contribution Splitting in ROSCA**](rosca-co-payer-splitting.md) — How co-payers are registered for a member's slot, how the split ratio is set and enforced, and what happens if a co-payer fails to contribute.

- [**ROSCA Waitlist Flow**](rosca-waitlist.md) — Lifecycle of group waitlists, queueing rules, FIFO and reputation-weighted promotion, catch-up contributions, and cancellation.

- [**ROSCA Waitlist Flow**](rosca-waitlist.md) — Lifecycle of group waitlists, queueing rules, FIFO and reputation-weighted promotion, catch-up contributions, and cancellation.
- [**Weighted Voting in ROSCA Governance**](rosca-weighted-voting.md) — How a member's vote weight is computed from round contributions, applied during proposal voting, and tallied against dynamic quorum thresholds vs. equal voting.
- [**ROSCA Governance Quorum Requirements**](rosca-governance-quorum.md) — Specification of per-ProposalType quorum thresholds, default percentages, admin overrides, resolution formulas, and administrative controls.
- [**ROSCA Group Split Flow**](rosca-group-split.md) — Proposal creation, member assignment and confirmation, expiry handling, execution, refunds, and resulting group identifiers.
- [**ROSCA Reinvestment Flow**](rosca-reinvestment.md) — How a member rolls a round payout forward into the next round as their contribution, the deadline constraint, and how over/under-payment is handled.
- [**ROSCA Group Snapshots**](rosca-snapshot.md) — Snapshot creation, captured group state, immutable audit records, and the recovery process.
- [**ROSCA Slot Auctions**](rosca-slot-auctions.md) — Comprehensive guide to plain open-bid and commit-reveal sealed-bid slot auctions in Ahjoor ROSCA groups.
- [**ROSCA Round-Skip Mechanism**](rosca-skip-round.md) — Member eligibility, request flow, per-cycle skip limits, skip fee handling, and downstream settlement effects.
- [**ROSCA Emergency Loan**](rosca-emergency-loan.md) — How members can draw emergency loans from the group reserve, repayment terms, default handling, and reserve management.
- [**On-Chain Audit Trail in ROSCA**](rosca-audit-trail.md) — Comprehensive cycle audit trail recording, contribution history pagination, retention window management, and archival lifecycle.

---

## `ahjoor-escrow`

- [**Escrow Auto-Renewal**](escrow-auto-renewal.md) — How buyers can pre-approve renewal cycles for recurring service agreements, how auto-renewals are triggered on release, and how buyers can cancel future renewals.
- [**Multi-Party Approval**](escrow-multiparty-approval.md) — N-of-M release approval configuration, threshold requirements, approver voting, and interactions with release and dispute flows.
- [**Dispute Timeout Enforcement**](escrow-dispute-timeout.md) — How stalled disputes are auto-resolved after a configurable timeout, per-escrow timeout overrides, default winner configuration, and the arbiter timeout counter.
- [**Seller Veto Mechanism**](escrow-seller-veto.md) — How sellers can veto a fund release within a cooldown window, how the cooldown interacts with repeated veto attempts, and how admin override resets the clock.
- [**Inspector Role**](inspector-role.md) — How a neutral inspector is assigned at escrow creation, the `submit_inspection_result` flow, dual-approval inspector replacement, and how the reputation score and threshold gate future assignments.

## `ahjoor-rosca`

- [**ROSCA Savings Milestone Rewards**](rosca-savings-milestone-rewards.md) — Automatic token rewards when savings goal milestones are crossed.
