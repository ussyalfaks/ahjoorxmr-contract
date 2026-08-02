# Bounty Board (ahjoor-escrow)

This document describes the bounty-board flows in `contracts/ahjoor-escrow`, with emphasis on milestone-based payout behavior.

## Milestone Payouts

Milestone payouts let a bounty release funds in smaller tranches instead of paying the full amount at the end.

### Milestone status flow

The bounty milestone flow is stateful and the contract moves each milestone through a small set of statuses:

- `Pending` while the milestone is waiting to be submitted or verified
- `Submitted` after the solver uploads the deliverable hash
- `Verified` during verifier sign-off
- `Paid` after the tranche has been transferred to the solver

For the verifier-gated bounty flow, later milestones remain blocked until every earlier milestone is already `Paid`. For the proportional BPS flow, each milestone must also be `Submitted` before approval, and rejected milestones can be returned to `Pending` for resubmission.

### How milestones are defined and ordered

Each milestone is defined when the bounty is created. A milestone includes:

- a description hash for the sub-deliverable
- the verifier address responsible for sign-off
- the token amount or release share tied to that milestone

Milestones are stored on-chain in the same order they are provided at creation time, and the contract enforces that order during submission and verification.

For verifier-based bounty milestones, milestone `N + 1` cannot be submitted until milestone `N` has already been verified and paid. That prevents the solver from skipping ahead or claiming later work before earlier work is accepted.

### How the basis-point split determines each payout

For proportional milestone escrows, each milestone defines a `release_bps` value. The contract requires all milestone basis-point values to sum to exactly `10,000`, which represents 100% of the escrow amount.

- `3,000 bps` pays 30% of the escrow amount
- `5,000 bps` pays 50% of the escrow amount
- `2,000 bps` pays the remaining 20%

When a submitted milestone is approved, the contract releases that milestone's proportional share to the seller. The final milestone releases any rounding remainder so the full escrow balance is settled exactly.

### What happens if a milestone is disputed or skipped

The bounty milestone payout flow does not introduce a separate on-chain dispute state for milestones. Instead, a verifier-gated milestone can be rejected before verification and returned to `Pending`, which lets the solver revise and resubmit the work.

Skipped milestones are not allowed. The contract requires strict ordering, so later milestones cannot be submitted until all earlier milestones have reached the paid state. That means a solver cannot bypass an unfinished milestone to unlock later payouts.

For verifier-gated bounty milestones, each milestone remains independently verifiable by its designated verifier, and any unresolved earlier milestone blocks progress to later payouts.

## References

- `contracts/ahjoor-escrow/src/test_bounty_milestone.rs`
- `contracts/ahjoor-escrow/src/test_milestone_bps.rs`