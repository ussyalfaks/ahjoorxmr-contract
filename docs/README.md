# Ahjoor Documentation Index

Welcome to the Ahjoor contract documentation directory. Below is a structured index of all documentation files, categorized by topic and smart contract module.

---

## 1. General & Platform Reference

- [**Contract Error Codes**](errors.md) — Consolidated reference of every numeric `#[contracterror]` code exposed across all Ahjoor smart contracts.
- [**ROSCA Contract Migration Guide**](migration-guide.md) — Step-by-step guide for upgrading and migrating active ROSCA contract deployments.
- [**State Archival Troubleshooting**](state-archival.md) — Guide for checking archived status, restoring dormant contracts via Stellar CLI, and managing storage TTL.

---

## 2. ROSCA Contract (`ahjoor-rosca`)

- [**Contribution Receipts in ROSCA**](rosca-contribution-receipts.md) — NFT-style contribution receipt data format, automatic minting on round finalization, event emissions, and member retrieval/verification functions.
- [**Cosigner Guarantee in ROSCA**](rosca-cosigner-guarantee.md) — Overview of co-signer nomination, acceptance, and default coverage logic for community savings groups.
- [**Contribution Delegation in ROSCA**](rosca-contribution-delegation.md) — Overview of contribution and voting weight delegation, proxy execution, limits, and revocation.
- [**Co-Payer Contribution Splitting in ROSCA**](rosca-co-payer-splitting.md) — How co-payers are registered for a member's slot, how the split ratio is set and enforced, and what happens if a co-payer fails to contribute.

- [**ROSCA Waitlist Flow**](rosca-waitlist.md) — Lifecycle of group waitlists, queueing rules, FIFO and reputation-weighted promotion, catch-up contributions, and cancellation.

- [**Weighted Voting in ROSCA Governance**](rosca-weighted-voting.md) — How a member's vote weight is computed from round contributions, applied during proposal voting, and tallied against dynamic quorum thresholds vs. equal voting.
- [**ROSCA Governance Quorum Requirements**](rosca-governance-quorum.md) — Specification of per-ProposalType quorum thresholds, default percentages, admin overrides, resolution formulas, and administrative controls.
- [**ROSCA Group Split Flow**](rosca-group-split.md) — Proposal creation, member assignment and confirmation, expiry handling, execution, refunds, and resulting group identifiers.
- [**ROSCA Reinvestment Flow**](rosca-reinvestment.md) — How a member rolls a round payout forward into the next round as their contribution, the deadline constraint, and how over/under-payment is handled.
- [**ROSCA Group Snapshots**](rosca-snapshot.md) — Snapshot creation, captured group state, immutable audit records, and the recovery process.
- [**ROSCA Slot Auctions**](rosca-slot-auctions.md) — Comprehensive guide to plain open-bid and commit-reveal sealed-bid slot auctions in Ahjoor ROSCA groups.
- [**ROSCA Audit Trail**](rosca-audit-trail.md) — On-chain cycle records, storage layout, archival behavior, query helpers, and emitted events for ROSCA round history.
- [**ROSCA Round-Skip Mechanism**](rosca-skip-round.md) — Member eligibility, request flow, per-cycle skip limits, skip fee handling, and downstream settlement effects.
- [**ROSCA Emergency Loan**](rosca-emergency-loan.md) — How members can draw emergency loans from the group reserve, repayment terms, default handling, and reserve management.

---

## 3. Escrow Contract (`ahjoor-escrow`)

- [**Escrow Dispute Flow**](escrow-dispute-flow.md) — Multi-step escrow dispute lifecycle, arbiter assignment, timeout enforcement, default winner rules, and resolution cooling-off periods.
- [**Escrow Dispute Timeout Handling**](escrow-dispute-timeout.md) — The dispute timeout window (global default and per-escrow override) and the default resolution outcome when `enforce_dispute_timeout` is triggered.
- [**Inspector Role and Scoring System**](inspector-role.md) — How an inspector is assigned to an escrow, their responsibilities and powers over the inspection gate, and how their accuracy score is calculated, updated on rulings and appeals, and enforced as a threshold for high-value escrows.
- [**Escrow Bounty Board**](bounty-board.md) — Escrow bounty milestone payouts and related milestone-based release behavior.
- [**Seller Veto Mechanism**](escrow-seller-veto.md) — How the seller raises a veto to block release, the cooldown window that limits re-vetoes, and how the admin overrides a veto.
- [**Escrow Auto-Renewal**](escrow-auto-renewal.md) — How buyers can pre-approve renewal cycles for recurring service agreements, how auto-renewals are triggered on release, and how buyers can cancel future renewals.

---

## 4. Payments Contract (`ahjoor-payments`)

- [**Payments Authorization and Capture Flow**](payments-flow.md) — Two-step payment authorization and capture lifecycle, buyer trust tiers, and merchant collateral rules.
- [**Merchant KYB Verification**](merchant-kyb.md) — KYB verification flow, payment creation gating, on-chain status checks, renewal and revocation.
- [**Merchant Referral Program**](payments-referral.md) — Referral registration, commission calculation on platform fees, accrual windows, and claiming.
- [**DAO Mediation for Disputed Payments**](dao-mediation.md) — On-chain DAO voting and resolution process for disputed merchant payments.
- [**Multi-Token Invoice**](multi-token-invoice.md) — Guide to multi-token invoicing, oracle-based price feeds, slippage tolerance, and cross-token settlement.
- [**Merchant Ban and Suspension Flow**](payments-merchant-ban.md) — Suspension and ban triggers, merchant appeals, reinstatement cooling-off, and re-appeal cooldowns.

---

## 5. Refund Contract (`ahjoor-refund`)

- [**Refund Contract Guide**](refund.md) — Refund request, approval, and claim workflows, deadline boundary enforcement, senior arbiter escalation, and customer abuse score tracking.
- [**Merchant Reserve Fund**](reserve-fund.md) — Refund contract merchant reserve balances and how they are tracked and used.

