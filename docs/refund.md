# Refund Contract (ahjoor-refund)

This document describes the refund contract (`contracts/ahjoor-refund`) behavior, who can trigger refunds, and the typical flow for requesting/approving/claiming refunds.

## When refunds apply
- Cancelled escrow: when an escrow is cancelled before funds are released.
- Failed round: if a ROSCA round fails to execute (e.g., not enough contributions).
- Overpayment: participant deposited more than required or duplicate payments.

Refund issuance is typically created when an upstream contract (e.g., `ahjoor-escrow` or `ahjoor-rosca`) determines funds must be returned. The refund record may be created automatically by the originating contract or by an explicit call to the refund contract.

## Who can trigger a refund
- Admin: can create or approve refunds in exceptional cases or to resolve disputes.
- Participant: a participant can request a refund for their own payment (see `request_refund`).
- Automatic: originating contracts may create refund records automatically on cancellation or failure.

## Key functions

- `request_refund(refund_id)` — participant requests a refund for a specific payment or escrow. Creates a refund record in `Pending` status when caller is the beneficiary.

- `approve_refund(refund_id)` — admin approves a pending refund. Moves refund to `Approved` state and records timestamp and approved amount. Only callable by admin or an authorized arbiter.

- `claim_refund(refund_id)` — participant claims funds for an approved refund. Transfers funds to claimant and marks refund `Claimed`.

- `create_refund(refund_id, owner, amount, metadata)` — (internal / called by originating contracts) create a refund record (used for automatic creation on cancellation).

- `get_refund(refund_id) -> Refund` — view refund record and status (`Pending`, `Approved`, `Claimed`, `Cancelled`).

## Typical flow

1. Escrow is cancelled (or round fails / overpayment detected).
2. Originating contract calls `create_refund(...)` on `ahjoor-refund` (automatic) OR participant calls `request_refund(refund_id)`.
3. Admin (or arbiter) reviews and calls `approve_refund(refund_id)` to approve the refund.
4. Participant calls `claim_refund(refund_id)` to withdraw the approved amount.

Alternate shorter flow (automatic approval): some flows can be configured so that refund creation includes an initial `Approved` status, allowing `claim_refund` directly after creation.

## Time limits and expirations

- Approval-to-claim window: the contract may enforce a time window (e.g., 30 days) within which the claimant must call `claim_refund` after approval. After the window expires the refund may be moved to `Expired` and require admin re-approval.
- Claim timelock: some refunds may include an optional timelock preventing claims until a given epoch (useful for dispute cooling-off).

Check the contract configuration constants for the exact timeouts used in the deployed instance.

## Escalation

Refund escalation is part of the refund dispute flow and is used when the initial review window expires without a final decision.

- **What triggers escalation:** a refund must first be in an escalatable state (`Requested`, `EvidenceSubmitted`, or `EvidencePeriodExpired`), and the primary review deadline must have passed. The contract rejects escalation before that deadline with `PrimaryDeadlineNotPassed`.
- **Who handles escalated refunds:** the configured senior arbiter handles escalated refunds. The arbiter address and the senior review window are set by the admin with `set_senior_arbiter` and `set_senior_review_window`.
- **What the senior arbiter can do:** the senior arbiter resolves the dispute with `resolve_escalated_refund(refund_id, approved, resolution_hash)`. If `approved` is `true`, the refund is processed and the customer receives the refund amount, minus any configured fee. If `approved` is `false`, the refund is marked rejected and the customer is returned the escrowed funds.
- **How the final outcome is enforced on-chain:** escalation moves the refund into `EscalatedToSenior` and stores the senior review deadline on-chain. Resolution can only be submitted by the configured senior arbiter, and the contract enforces the outcome by updating refund status and transferring tokens from the contract to the customer and, if configured, the fee recipient.
- **Missed senior deadline:** if `auto_approve_on_senior_miss` is enabled, anyone can call `trigger_senior_auto_approve(refund_id)` after the senior deadline passes. This finalizes the refund on-chain, marks it `Processed`, and records the auto-approval source as `senior_miss`.

## Events and error handling

- Events: `RefundRequested`, `RefundCreated`, `RefundApproved`, `RefundClaimed`, `RefundExpired`, `RefundCancelled`.
- Common errors: `NotAuthorized`, `InvalidRefundState`, `RefundNotFound`, `ClaimWindowExpired`, `InsufficientBalance`.

## Example (CLI)

Request a refund (participant):

```bash
stellar contract invoke --id <REFUND_CONTRACT_ID> --network testnet -- request_refund --refund-id <ID>
```

Approve (admin):

```bash
stellar contract invoke --id <REFUND_CONTRACT_ID> --network testnet -- approve_refund --refund-id <ID>
```

Claim (participant):

```bash
stellar contract invoke --id <REFUND_CONTRACT_ID> --network testnet -- claim_refund --refund-id <ID>
```

## Abuse Score

The refund contract tracks an **Abuse Score** per customer to prevent spam and dispute abuse.

### What actions increase the score
- **Refund Rejection:** When an admin rejects a refund request (`reject_refund`), the customer's score increases by `+10`.
- **Rapid Submission:** Submitting multiple refund requests within a very short timeframe (configured by `rapid_submission_window`) adds an immediate penalty of `+5` to the score.
- **Flagged Abuse:** If an admin explicitly flags a refund as abusive (`flag_refund_abuse`), it adds an elevated penalty (an additional `+10` on top of the standard rejection penalty).

### Thresholds and Restrictions
- The contract maintains an `abuse_block_threshold` (e.g., typically `30`).
- If a customer's score reaches or exceeds this threshold, they are temporarily blocked from submitting new refund requests (returning a `CustomerBlockedForAbuse` error).
- The block duration is determined by `block_duration_ledgers`. Once this period elapses (or the score decays below the threshold), the customer can request refunds again.

### Score Decay and Resets
- **Decay over time:** The abuse score decays automatically as ledgers advance. By default, the score halves (`5000 bps` factor) every `10,000` ledgers. Both the decay period and the decay factor are configurable by the admin (`set_abuse_score_decay_params`).
- **Manual Reset:** An admin can manually reset a customer's abuse score to zero using `reset_customer_abuse_score`.

## Counter-Offer Negotiation

When a customer requests a refund (`Requested` status), the merchant may respond with a counter-offer — a partial refund amount — instead of approving or rejecting the full request. This negotiation flow is implemented via the counter-offer system.

### How a buyer requests a refund and a merchant counters

1. **Customer requests refund:** The customer calls `request_refund()` with the full refund amount. The refund enters `Requested` status.
2. **Merchant counter-offers:** The merchant calls `counter_offer_refund(refund_id, amount)` to propose a lower amount. The refund moves to `CounterOffered` status and a `CounterOffer` record is stored with an expiry timestamp.
   - Only the refund's merchant can counter-offer.
   - The counter-offer amount must be positive and cannot exceed the original refund amount.
   - Only one counter-offer is permitted per refund (a second attempt panics with `Refund is not in Requested state`).

### How many rounds of negotiation are allowed

The negotiation is **single-round**: the merchant submits exactly one counter-offer. If the customer rejects or the offer expires, the refund escalates to admin review (`UnderAppeal`). There is no multi-round back-and-forth.

### How the flow resolves

The counter-offer resolves in one of four ways:

#### 1. Customer acceptance
The customer calls `accept_counter_offer(refund_id)`. The counter-offer amount is transferred immediately and the refund is marked `Processed`. If the offer has already expired when acceptance is attempted, it auto-escalates to admin instead.

#### 2. Customer rejection
The customer calls `reject_counter_offer(refund_id)`. The counter-offer record is removed and the refund is escalated to `UnderAppeal` for admin review.

#### 3. Expiry escalation
Anyone can call `check_counter_offer_expiry(refund_id)` after the offer's expiry timestamp passes. If expired, the refund escalates to `UnderAppeal` for admin review. The admin also has the option to call `settle_expired_counter_offer(refund_id)` which applies the contract's default resolution on expiry:
- **Accept original** (default): the original refund amount is paid out and the refund is `Processed`.
- **Reject**: the escrowed funds are returned to the customer and the refund is `Rejected`.

The admin can toggle the default resolution by setting the `CounterOfferDefaultResolution` configuration flag.

#### 4. Admin override via settle
The admin can configure the expiry window with `set_counter_offer_expiry_seconds(admin, seconds)` (default: 48 hours). The admin also controls the `CounterOfferDefaultResolution` flag (default: `true` = accept original on expiry).

### Key configuration constants

| Constant | Default | Description |
|---|---|---|
| `counter_offer_refund` expiry | 48 hours | Window for customer to respond to a counter-offer |
| `CounterOfferDefaultResolution` | `true` (accept original) | What happens on expiry — pay original amount or reject |

### Events

- `RefundCounterOffered` — emitted when a merchant submits a counter-offer.
- `RefundCounterAccepted` — emitted when the customer accepts the counter-offer.
- `RefundCounterRejected` — emitted when the customer rejects the counter-offer.
- `CounterOfferExpired` — emitted when a counter-offer expires and is settled.

## Notes for integrators

- Originating contracts should set refund `owner` and `amount` precisely to avoid disputes.
- Admin approvals should be auditable — consider storing `approver` and `approved_at` on the refund record.
- If automatic approvals are enabled, ensure checks are in place to prevent double refunds.

---

See `contracts/ahjoor-refund` for on-chain implementation details and exact function signatures.
