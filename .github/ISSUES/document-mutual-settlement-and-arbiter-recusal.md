---
title: "docs: Document mutual settlement offers and arbiter recusal in ahjoor-escrow"
labels: documentation, enhancement
assignees: ''
---

## Problem Statement
The `ahjoor-escrow` contract has implemented two important features (#977 and #978) that are not yet documented:
1. **Mutual settlement offers** - allowing disputing parties to resolve disputes without arbiter intervention
2. **Arbiter recusal** - allowing assigned arbiters to voluntarily step down and be replaced from the pool

These features are fully implemented in the code but lack comprehensive documentation for users and integrators.

## Proposed Solution
Create documentation that covers:

### Mutual Settlement Offers (#977)
- Overview of the feature and its benefits (dispute resolution without arbiter fees)
- How parties can propose settlement offers with `propose_settlement()`
- How counterparties can accept offers with `accept_settlement()`
- How proposers can withdraw offers with `withdraw_settlement()`
- Query function `get_settlement_offer()` to check active offers
- Key constraints:
  - Only available for disputed escrows
  - Only buyer or seller can propose/accept
  - Proposer cannot accept their own offer
  - Settlement amount must be within valid range (0 to escrowed amount)
  - Arbiter rulings supersede open settlement offers
  - No fees charged on mutual settlements (sponsorship and collateral refunded)

### Arbiter Recusal (#978)
- Overview of the feature and when arbiters should recuse
- How to recuse using `recuse_arbiter()` with reason hash
- Replacement process from arbiter pool
- Key constraints:
  - Only the currently assigned arbiter can recuse
  - Requires eligible replacement arbiters in the pool
  - Resets dispute timeout clock (not counted as timeout)
  - Works for both disputed and non-disputed escrows

## Use Case
This documentation will help:
- Frontend developers integrate these features into their UIs
- Protocol integrators understand dispute resolution workflows
- Arbiters understand the recusal process and its implications
- End users understand their options when disputes arise

## Implementation Details
Suggested documentation locations:
- Add a new section in `contracts/ahjoor-escrow/README.md` (if it exists) or main `README.md`
- Update any API reference documentation
- Add code examples showing typical usage patterns
- Document the event emissions: `SettlementProposed`, `SettlementAccepted`, `SettlementWithdrawn`, `ArbiterRecused`

## Additional Context
- Feature implementation: #977 (mutual settlement) and #978 (arbiter recusal)
- Related issue: #1095 (test coverage for these features)
- These features are production-ready and already tested in the codebase

## Checklist
- [ ] Document mutual settlement offer workflow
- [ ] Document arbiter recusal process
- [ ] Add code examples for each feature
- [ ] Document error codes and events
- [ ] Update main README if needed
- [ ] Consider adding sequence diagrams for complex flows
