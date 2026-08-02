# Ahjoor Documentation Index

Welcome to the Ahjoor contract documentation directory. Below is a structured index of all documentation files, categorized by topic and smart contract module.

---

## 1. General & Platform Reference

- [**Contract Error Codes**](errors.md) — Consolidated reference of every numeric `#[contracterror]` code exposed across all Ahjoor smart contracts.
- [**ROSCA Contract Migration Guide**](migration-guide.md) — Step-by-step guide for upgrading and migrating active ROSCA contract deployments.
- [**State Archival Troubleshooting**](state-archival.md) — Guide for checking archived status, restoring dormant contracts via Stellar CLI, and managing storage TTL.

---

## 2. ROSCA Contract (`ahjoor-rosca`)

- [**Cosigner Guarantee in ROSCA**](rosca-cosigner-guarantee.md) — Overview of co-signer nomination, acceptance, and default coverage logic for community savings groups.
- [**Contribution Delegation in ROSCA**](rosca-contribution-delegation.md) — Overview of contribution and voting weight delegation, proxy execution, limits, and revocation.
- [**Co-Payer Contribution Splitting in ROSCA**](rosca-co-payer-splitting.md) — How co-payers are registered for a member's slot, how the split ratio is set and enforced, and what happens if a co-payer fails to contribute.

---

## 3. Escrow Contract (`ahjoor-escrow`)

- [**Escrow Dispute Flow**](escrow-dispute-flow.md) — Multi-step escrow dispute lifecycle, arbiter assignment, timeout enforcement, default winner rules, and resolution cooling-off periods.
- [**Inspector Role and Scoring System**](inspector-role.md) — How an inspector is assigned to an escrow, their responsibilities and powers over the inspection gate, and how their accuracy score is calculated, updated on rulings and appeals, and enforced as a threshold for high-value escrows.

---

## 4. Payments Contract (`ahjoor-payments`)

- [**Payments Authorization and Capture Flow**](payments-flow.md) — Two-step payment authorization and capture lifecycle, buyer trust tiers, and merchant collateral rules.
- [**Merchant KYB Verification**](merchant-kyb.md) — KYB verification flow, payment creation gating, on-chain status checks, renewal and revocation.
- [**DAO Mediation for Disputed Payments**](dao-mediation.md) — On-chain DAO voting and resolution process for disputed merchant payments.
- [**Multi-Token Invoice**](multi-token-invoice.md) — Guide to multi-token invoicing, oracle-based price feeds, slippage tolerance, and cross-token settlement.

---

## 5. Refund Contract (`ahjoor-refund`)

- [**Refund Contract Guide**](refund.md) — Refund request, approval, and claim workflows, senior arbiter escalation, and customer abuse score tracking.
