# Ahjoor Token Whitelist

The `ahjoor-token-whitelist` contract maintains the canonical set of tokens accepted across the Ahjoor platform. Only tokens that appear in the whitelist can be used as payment or savings denominations in the escrow, payments, and ROSCA contracts.

---

## Overview

The whitelist is managed by a designated admin address. The admin can directly add or remove tokens and configure per-token quotas, risk tiers, and metadata. In addition, **any governance-token holder** can propose a new token listing through the on-chain governance process described below.

---

## Admin Operations

| Function | Who can call | Effect |
|---|---|---|
| `initialize(admin)` | deployer (once) | Sets the admin and creates an empty whitelist. |
| `add_token(admin, token)` | admin | Immediately adds `token` to the whitelist. |
| `remove_token(admin, token)` | admin | Removes `token` from the whitelist. |
| `set_token_metadata(admin, token, …)` | admin | Stores decimals, symbol, logo hash and canonical oracle for `token`. |
| `set_risk_tier(admin, tier_id, …)` | admin | Defines or updates a named risk tier with transaction-amount limits. |
| `assign_token_tier(admin, token, tier_id)` | admin | Assigns a risk tier to `token`. |
| `set_token_quota(admin, token, …)` | admin | Sets a maximum volume-per-period quota on `token`. |
| `suspend_token(admin, token, …)` | admin | Temporarily suspends `token` until a given ledger. |
| `lift_token_suspension(admin, token)` | admin | Lifts an active suspension before its expiry. |

---

## Governance

### Purpose

The governance system lets the broader token-holder community propose and vote on new token listings without requiring direct admin intervention. It is intentionally additive: an approved proposal adds a token to the whitelist, but the admin retains a veto right during the enactment window.

---

### Governance Configuration

Before any proposal can be submitted the admin must configure the governance parameters. All parameters are stored in instance storage and take effect immediately for future proposals.

| Setter | Parameter | Default (if unset) | Notes |
|---|---|---|---|
| `set_governance_token(admin, token)` | The SPL / Stellar asset used as the voting token. | none — **required** | Must be set before any proposal. |
| `set_min_proposal_stake(admin, min_stake)` | Minimum governance-token balance a proposer must hold at proposal time. | `1` | Prevents dust-account spam. |
| `set_voting_window_ledgers(admin, ledgers)` | How many ledgers the voting window stays open after a proposal is created. | `120_960` (~7 days at 5 s/ledger) | Must be > 0. |
| `set_enactment_delay_ledgers(admin, ledgers)` | Minimum number of ledgers that must elapse between finalisation and enactment. | `34_560` (~2 days at 5 s/ledger) | Gives the admin time to veto if needed. |
| `set_quorum_bps(admin, quorum_bps)` | Minimum share of votes that must be **approve** votes, expressed in basis points. | `5_000` (50 %) | Range: 0–10 000. |

---

### Who Can Propose a Whitelist Change

Any address whose governance-token balance is **≥ `min_proposal_stake`** at the time of calling may submit a proposal.

```
propose_token_listing(proposer, token, rationale_hash) → proposal_id
```

- `proposer` must sign the transaction (`require_auth`).
- `token` is the Stellar asset address to be listed.
- `rationale_hash` is a 32-byte hash (e.g. SHA-256 of an off-chain discussion post or IPFS CID) that links the on-chain record to a human-readable justification.
- The contract checks the proposer's live governance-token balance against `min_proposal_stake`. If the balance is below the threshold the call panics with `InsufficientProposerStake`.
- A new `ListingProposal` is created with `status = Active` and a `voting_deadline_ledger` equal to the current ledger sequence plus `voting_window_ledgers`.
- The monotonically increasing `proposal_id` is returned and a `ListingProposed` event is emitted.

A single address can have multiple open proposals simultaneously; there is no per-proposer proposal cap.

---

### Voting

Any governance-token holder may cast a weighted vote on an active proposal.

```
vote_listing(voter, proposal_id, approve, weight)
```

- `voter` must sign the transaction.
- `approve` — `true` for in-favour, `false` for against.
- `weight` — the number of governance-token units to assign to this vote. Must be positive and **must not exceed the voter's governance-token balance at the time of their first vote on this proposal** (the _balance snapshot_).

#### Balance Snapshot (Flash-Loan Protection)

When a voter calls `vote_listing` for the first time on a given proposal, the contract reads their current governance-token balance and stores it in persistent storage (`VoteWeightSnapshot(proposal_id, voter)`). All subsequent votes from the same address on the same proposal are capped against this snapshot balance, not the live on-chain balance. This prevents a voter from acquiring tokens after their first vote in order to inflate their weight on a re-vote.

#### Re-voting (Last-Write Wins)

A voter may change or replace their vote at any time while the voting window is still open. The previous vote's weight is subtracted from the running `approve_weight` or `reject_weight` totals and the new vote's weight is added. Only the most recent vote from each address is counted.

#### Voting Window

Votes are accepted while `current_ledger_sequence ≤ voting_deadline_ledger`. Any attempt to vote after the deadline panics with `VotingWindowClosed`.

---

### Quorum and Approval Rules

After the voting window closes, any caller may invoke:

```
finalise_listing_proposal(proposal_id)
```

This is a permissionless function — anyone can finalise. The contract:

1. Confirms the voting window has closed (`current_ledger_sequence > voting_deadline_ledger`).
2. Computes `total_weight = approve_weight + reject_weight`.
3. Evaluates the quorum condition:

   ```
   approve_weight * 10_000 >= quorum_bps * total_weight
   ```

   In plain terms: the **fraction of total votes that are approve votes must meet or exceed `quorum_bps / 10_000`**. For the default 50 % quorum, more than half of all cast votes (by weight) must be approve votes.

4. If quorum is met the proposal moves to `PendingEnactment` and `enactment_deadline_ledger` is set to `current_ledger_sequence + enactment_delay_ledgers`.
5. If quorum is not met the proposal moves to `Failed` and no further action is possible.

> **Example:** With `quorum_bps = 5_000`, voters cast 600 approve and 400 reject (total 1 000). `600 * 10_000 = 6_000_000 ≥ 5_000 * 1_000 = 5_000_000` — quorum met. If instead 400 approve and 700 reject, `400 * 10_000 = 4_000_000 < 5_000 * 1_100 = 5_500_000` — quorum failed.

---

### Admin Veto

At any point before a proposal reaches `Enacted` status the admin may veto it:

```
veto_listing_proposal(admin, proposal_id, reason_hash)
```

- `admin` must sign the transaction.
- `reason_hash` is a 32-byte hash that records the veto rationale off-chain.
- The proposal moves to `Vetoed` and a `ListingVetoed` event is emitted.
- A vetoed proposal cannot be re-activated; the proposer must submit a new one.
- Proposals already in `Enacted` state cannot be vetoed.

The enactment delay (`enactment_delay_ledgers`, default ~2 days) is the window during which the admin is expected to exercise this right if needed.

---

### How an Approved Change Is Executed On-Chain

Once a proposal is in `PendingEnactment` status and `enactment_delay_ledgers` have elapsed (i.e. `current_ledger_sequence > enactment_deadline_ledger`), **any caller** may execute enactment:

```
enact_listing(proposal_id)
```

1. The contract confirms `status == PendingEnactment` and that the enactment delay has elapsed; otherwise it panics with `EnactmentDelayNotElapsed`.
2. If the token is not already whitelisted (e.g. no admin race-condition add), it is appended to the `WhitelistedTokens` persistent Vec and a `WhitelistMembership(token)` key is set to `true`.
3. The proposal status is updated to `Enacted` and a `ListingEnacted` event is emitted.
4. From this point `is_token_allowed(token)` returns `true` and the token is accepted platform-wide.

Enactment is idempotent with respect to membership: if the token was whitelisted directly by the admin between finalisation and enactment, the membership write is skipped and the proposal still moves to `Enacted`.

---

### Proposal Lifecycle Summary

```
                     ┌──────────────────────────────────────────┐
                     │           propose_token_listing           │
                     │  (proposer balance ≥ min_proposal_stake)  │
                     └────────────────────┬─────────────────────┘
                                          │
                                          ▼
                                      [ Active ]
                                     /          \
                      voting window              veto_listing_proposal
                         closes                         │
                            │                           ▼
                 finalise_listing_proposal           [ Vetoed ]
                            │
               ┌────────────┴──────────────┐
               │ quorum met?               │ quorum not met?
               ▼                           ▼
       [ PendingEnactment ]            [ Failed ]
               │
    enactment delay elapsed
    + enact_listing called
               │
               ▼
           [ Enacted ]
     (token added to whitelist)
```

---

### On-Chain Events

| Event | When emitted | Key fields |
|---|---|---|
| `ListingProposed` | `propose_token_listing` succeeds | `proposal_id`, `token`, `proposer` |
| `ListingVoteCast` | `vote_listing` succeeds | `proposal_id`, `voter`, `approve`, `weight` |
| `ListingEnacted` | `enact_listing` succeeds | `proposal_id`, `token` |
| `ListingVetoed` | `veto_listing_proposal` succeeds | `proposal_id`, `reason_hash` |

---

### Error Reference

| Panic message | Cause |
|---|---|
| `GovernanceTokenNotConfigured` | `set_governance_token` was never called. |
| `InsufficientProposerStake` | Proposer's balance < `min_proposal_stake`. |
| `ProposalNotFound` | `proposal_id` does not exist. |
| `ProposalNotActive` | Vote or finalise called on a non-Active proposal. |
| `VotingWindowClosed` | Vote attempted after `voting_deadline_ledger`. |
| `VotingWindowNotClosed` | Finalise attempted before `voting_deadline_ledger`. |
| `VoteWeightExceedsBalance` | `weight` > snapshot balance at first-vote time. |
| `ProposalNotPendingEnactment` | Enact called on a proposal not in `PendingEnactment`. |
| `EnactmentDelayNotElapsed` | `enact_listing` called before `enactment_deadline_ledger`. |
| `ProposalAlreadyTerminal` | Veto attempted on `Enacted` or already-`Vetoed` proposal. |

---

### Security Considerations

- **Stake check is live at proposal time only.** The proposer can transfer tokens away after submitting; the proposal remains valid. This is intentional — governance is decided by voter weight, not proposer lock-up.
- **Balance snapshot at first vote** prevents flash-loan amplification of voting weight on re-votes.
- **Permissionless finalisation and enactment** mean no single party can block an approved proposal indefinitely (other than the admin veto, which is by design).
- **Admin veto is unlimited in time** up to enactment. Governance participants should factor the enactment delay into their expectations.
