# Ahjoor Token Whitelist

The `ahjoor-token-whitelist` contract is the single source of truth for which tokens are
accepted across the entire Ahjoor platform. Any token that is **not** present in this
whitelist is automatically rejected by the escrow, payments, and ROSCA contracts — no
per-contract configuration is needed.

This design means a bad token can be blocked once here instead of in every downstream
contract, and a new token can be unlocked platform-wide through a single governance vote.

---

## Overview

The whitelist has two management paths:

| Path | Who controls it | Speed |
|---|---|---|
| **Admin operations** | A single designated admin address | Instant — takes effect in the same transaction |
| **Governance proposals** | Any governance-token holder with enough stake | Slower — requires a voting window and an enactment delay |

The admin path is intended for routine maintenance (adding well-known stablecoins, removing
a deprecated token). The governance path exists so that the community can propose new
listings without depending on admin availability, and so that the process is transparent
and auditable on-chain.

---

## Admin Operations

The admin is set once at deployment and cannot be changed without redeploying the contract.
All admin functions require the admin address to have signed the transaction.

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

Admin calls bypass the governance process entirely — they are designed for emergency
responses (e.g. suspending a token whose oracle has gone stale) and for uncontroversial
housekeeping.

---

## Governance

### Purpose

The governance system lets the broader token-holder community propose and vote on new
token listings **without requiring direct admin intervention**. The flow is:

1. A holder with sufficient stake submits a proposal naming the token they want listed.
2. Other holders vote approve or reject using their governance-token balance as weight.
3. After the voting window closes, anyone can finalise the result.
4. If approved, anyone can enact the change after a mandatory delay (giving the admin
   time to veto if something went wrong).
5. Once enacted, the token is live on the whitelist — no further admin action required.

The process is intentionally **additive**: governance can only add tokens; removing a
token still requires the admin. This limits the blast radius of a bad governance vote.

---

### Governance Configuration

The admin must configure the governance parameters before any proposals can be submitted.
These settings live in instance storage and take effect immediately for any proposal
created after they are changed.

| Setter | What it controls | Default (if never set) | Notes |
|---|---|---|---|
| `set_governance_token(admin, token)` | Which Stellar asset is used as the voting token | none — **required before any proposal** | This is the token voters must hold to participate. |
| `set_min_proposal_stake(admin, min_stake)` | Minimum governance-token balance required to submit a proposal | `1` | Set this high enough to deter spam proposals from dust accounts. |
| `set_voting_window_ledgers(admin, ledgers)` | How many ledgers the voting period lasts after a proposal is created | `120_960` ≈ 7 days at 5 s/ledger | Voters need enough time to notice and participate; longer windows reduce the risk of a rushed vote. |
| `set_enactment_delay_ledgers(admin, ledgers)` | How many ledgers must pass between a successful finalisation and enactment | `34_560` ≈ 2 days at 5 s/ledger | This is the admin's veto window — time to review the result and block it if needed. |
| `set_quorum_bps(admin, quorum_bps)` | What fraction of cast votes must be approve votes, in basis points (1 bp = 0.01 %) | `5_000` = 50 % | A value of `5_000` means more than half of all cast weight must be approve. Range: 0–10 000. |

> **Ledger time reference:** At a nominal 5-second ledger close time, 1 day ≈ 17 280
> ledgers. Actual close times vary; treat the defaults as approximations.

---

### Who Can Propose a Whitelist Change

Any address that holds **at least `min_proposal_stake` governance tokens** at the moment
they call the function may submit a proposal. There is no allowlist of approved
proposers — the stake requirement is the only gate.

```
propose_token_listing(proposer, token, rationale_hash) → proposal_id
```

**Parameters:**

- `proposer` — the address submitting the proposal. Must have signed the transaction
  (`require_auth` is enforced). This is the account whose balance is checked.
- `token` — the Stellar asset contract address of the token to be listed.
- `rationale_hash` — a 32-byte hash (e.g. SHA-256 of an IPFS CID or a forum post URL)
  that links this on-chain record to a human-readable justification. The contract does
  not validate the contents; it just stores the hash so the community can verify the
  proposer's stated reasoning has not been altered after the vote.

**What happens on-chain:**

1. The contract reads the proposer's live governance-token balance. If it is below
   `min_proposal_stake` the call panics with `InsufficientProposerStake`.
2. A new `ListingProposal` record is created in persistent storage with:
   - `status = Active`
   - `approve_weight = 0`, `reject_weight = 0`
   - `voting_deadline_ledger = current_ledger + voting_window_ledgers`
3. The next available `proposal_id` (a monotonically increasing integer) is returned
   to the caller and a `ListingProposed` event is emitted.

**Things to know:**

- The stake check happens **only at proposal time**. The proposer can transfer their
  tokens away immediately after; the proposal stays open. Governance is decided by
  voter weight, not proposer lock-up.
- A single address can have multiple active proposals at the same time. There is no
  per-proposer cap.
- There is no proposal fee. The only cost is the network transaction fee.

---

### Voting

Any governance-token holder may cast a weighted vote on a proposal while it is in
`Active` status and the voting window is still open.

```
vote_listing(voter, proposal_id, approve, weight)
```

**Parameters:**

- `voter` — must have signed the transaction.
- `proposal_id` — the ID returned by `propose_token_listing`.
- `approve` — `true` to vote in favour; `false` to vote against.
- `weight` — the number of governance-token units to assign to this vote. Must be
  positive and must not exceed the voter's balance at first-vote time (see snapshot
  rule below).

#### Balance Snapshot (Flash-Loan Protection)

The first time a voter calls `vote_listing` on a given proposal, the contract reads
their governance-token balance and stores it permanently in persistent storage under
the key `VoteWeightSnapshot(proposal_id, voter)`. From that point forward, **all votes
from this address on this proposal are capped against that snapshot**, not the live
on-chain balance.

Why does this matter? Without the snapshot, an attacker could:

1. Vote with a small balance.
2. Borrow a large number of governance tokens (flash loan).
3. Re-vote with the inflated balance to swing the result.
4. Return the borrowed tokens in the same transaction.

The snapshot prevents step 3: even if the attacker's balance grows after their first
vote, the cap stays at what they held when they first participated.

#### Re-voting (Last-Write Wins)

Voters are not locked into their initial choice. While the voting window is open, a
voter can call `vote_listing` again on the same proposal to change their vote or
change their weight. The contract:

1. Subtracts the previous vote's weight from whichever running total it was in
   (`approve_weight` or `reject_weight`).
2. Adds the new vote's weight to the appropriate total.

Only the most recent vote from each address counts. Earlier votes from the same address
are fully replaced.

#### Voting Window

Votes are accepted while `current_ledger_sequence ≤ voting_deadline_ledger`. Once this
deadline is passed, any further `vote_listing` call panics with `VotingWindowClosed`.
The deadline is fixed when the proposal is created and cannot be extended.

---

### Quorum and Approval Rules

After the voting window closes, **anyone** can trigger finalisation — there is no
requirement for the proposer or admin to do it. This permissionless design ensures
a proposal cannot be held hostage by a single party's inaction.

```
finalise_listing_proposal(proposal_id)
```

The contract performs these steps in order:

1. **Checks the window is closed.** If `current_ledger_sequence ≤ voting_deadline_ledger`
   it panics with `VotingWindowNotClosed` — you cannot finalise early.

2. **Tallies the votes.** `total_weight = approve_weight + reject_weight`.

3. **Evaluates quorum.** The condition is:

   ```
   approve_weight × 10_000  ≥  quorum_bps × total_weight
   ```

   Rearranged into plain English: **the fraction of total voted weight that is in
   favour must be at least `quorum_bps / 10_000`.**

   With the default `quorum_bps = 5_000` (50 %), the approve side must account for at
   least half of all cast weight. A bare majority of 50.0001 % is enough; abstaining
   (not voting at all) does not count against the proposal.

4. **Updates the proposal status:**
   - If quorum is met → `PendingEnactment`. The `enactment_deadline_ledger` is set to
     `current_ledger + enactment_delay_ledgers`.
   - If quorum is not met → `Failed`. The proposal is permanently closed; no further
     action is possible on this proposal ID.

> **Worked example:**
>
> | Scenario | approve | reject | total | approve % | quorum (50 %) |
> |---|---|---|---|---|---|
> | Clear win | 600 | 400 | 1 000 | 60 % | ✅ met |
> | Narrow win | 501 | 499 | 1 000 | 50.1 % | ✅ met |
> | Exact tie | 500 | 500 | 1 000 | 50 % | ✅ met (≥, not >) |
> | Narrow loss | 499 | 501 | 1 000 | 49.9 % | ❌ failed |
> | Heavy rejection | 400 | 700 | 1 100 | 36 % | ❌ failed |

---

### Admin Veto

Even after a proposal passes the quorum check, the admin retains the right to block
enactment. This is a safety valve: if a governance vote was manipulated, if the token
turns out to be fraudulent, or if the proposal conflicts with a regulatory requirement,
the admin can stop it before it becomes active.

```
veto_listing_proposal(admin, proposal_id, reason_hash)
```

- `admin` must have signed the transaction.
- `reason_hash` — a 32-byte hash recording the veto rationale off-chain (same
  convention as `rationale_hash` on proposals).
- The proposal moves to `Vetoed` and a `ListingVetoed` event is emitted.
- **A vetoed proposal cannot be reactivated.** If the underlying concern is resolved,
  the proposer must start a fresh proposal.
- Proposals already in `Enacted` state cannot be vetoed — once a token is live it
  must be removed via `remove_token` instead.

The veto right is available from the moment a proposal is created (`Active` status) all
the way through `PendingEnactment`. The `enactment_delay_ledgers` setting (default ~2
days) is specifically designed to give the admin time to review the result of every
successful vote before it takes permanent effect.

---

### How an Approved Change Is Executed On-Chain

Once a proposal is in `PendingEnactment` and the enactment delay has fully elapsed,
**any caller** (not just the admin or proposer) can execute the change. Like finalisation,
enactment is permissionless so neither party can stall it.

```
enact_listing(proposal_id)
```

**What the contract does, step by step:**

1. **Validates state.** Checks `status == PendingEnactment`. If not, panics with
   `ProposalNotPendingEnactment`.

2. **Validates timing.** Checks `current_ledger_sequence > enactment_deadline_ledger`.
   If the delay has not yet elapsed, panics with `EnactmentDelayNotElapsed`. This is the
   hard guarantee that the admin always has the full delay window to veto.

3. **Writes whitelist membership (idempotent).** If `WhitelistMembership(token)` is not
   already `true`, the token is appended to the `WhitelistedTokens` persistent Vec and
   the membership key is set to `true`. If the admin happened to whitelist the same token
   directly between finalisation and enactment (a race condition), the write is skipped —
   the token is already live and no duplicate entry is created.

4. **Finalises the proposal.** Sets `status = Enacted` and emits a `ListingEnacted`
   event.

5. **Effect on the rest of the platform.** From this ledger onward, `is_token_allowed(token)`
   returns `true`. All escrow, payments, and ROSCA contracts that call this function will
   immediately accept the newly listed token.

---

### End-to-End Walkthrough

Here is a concrete example tracing a proposal from submission to enactment, using the
default governance parameters (7-day voting window, 2-day enactment delay, 50 % quorum).

```
Ledger 1 000 000
  Alice holds 500 GOV tokens (min_proposal_stake = 100).
  Alice calls: propose_token_listing(alice, USDC_XLM, sha256("ipfs://QmXyz..."))
  → proposal_id = 42, voting_deadline_ledger = 1 120 960

Ledger 1 010 000
  Bob (holds 600 GOV) calls: vote_listing(bob, 42, true, 600)
  → snapshot stored: Bob → 600. approve_weight = 600.

Ledger 1 020 000
  Carol (holds 400 GOV) calls: vote_listing(carol, 42, false, 400)
  → reject_weight = 400.

Ledger 1 030 000
  Bob changes his mind and reduces his support:
  vote_listing(bob, 42, true, 200)
  → approve_weight updated: 600 - 600 + 200 = 200.
  → Bob's snapshot cap is still 600; he used only 200 this time.

Ledger 1 120 961  (window just closed)
  Anyone calls: finalise_listing_proposal(42)
  total_weight = 200 + 400 = 600
  200 * 10_000 = 2_000_000  vs  5_000 * 600 = 3_000_000  → FAILED (33 % < 50 %)

  → The proposal fails. Alice must resubmit with a stronger case to recruit more
    approve votes.
```

A passing scenario with the same setup but different voters:

```
Ledger 1 010 000  Bob votes approve 600.   approve = 600
Ledger 1 020 000  Carol votes approve 350. approve = 950, reject = 0
Ledger 1 120 961  finalise → 950/950 = 100 % ≥ 50 % → PendingEnactment
                  enactment_deadline_ledger = 1 120 961 + 34 560 = 1 155 521

Ledger 1 155 522  (delay elapsed, no veto issued)
  Anyone calls: enact_listing(42)
  → USDC_XLM is added to WhitelistedTokens.
  → is_token_allowed(USDC_XLM) now returns true platform-wide.
```

---

### Proposal Lifecycle Summary

The diagram below shows every state a proposal can reach and the function calls that
drive each transition.

```
  ┌──────────────────────────────────────────────────┐
  │              propose_token_listing               │
  │   (proposer balance ≥ min_proposal_stake)        │
  └──────────────────────┬───────────────────────────┘
                         │  proposal created
                         ▼
                     [ Active ]  ◄──────────────────────────────────────┐
                    /           \                                        │
   voting window                 veto_listing_proposal           (new proposal
      closes                     (admin only, anytime              required)
         │                        before Enacted)                       │
         │                             │                                │
         ▼                             ▼                                │
  finalise_listing_proposal        [ Vetoed ] ──── permanently ─────────┘
         │
  ┌──────┴──────────────────────┐
  │ approve_weight * 10_000     │
  │   ≥ quorum_bps * total?     │
  └──────┬──────────────────────┘
         │                    │
         │ YES                │ NO
         ▼                    ▼
  [ PendingEnactment ]    [ Failed ]
         │                (terminal)
  enactment delay
  elapses (no veto)
         │
         ▼
    enact_listing
    (permissionless)
         │
         ▼
     [ Enacted ]
  (token is live on
    the whitelist)
```

**State glossary:**

| State | Meaning |
|---|---|
| `Active` | Proposal is open; votes are being collected. |
| `PendingEnactment` | Quorum passed; waiting for the enactment delay before the token goes live. |
| `Enacted` | Token has been added to the whitelist. Terminal. |
| `Failed` | Quorum was not met after the voting window. Terminal. |
| `Vetoed` | Admin blocked enactment before the token went live. Terminal. |

---

### On-Chain Events

Events are emitted on every state change and vote. Off-chain indexers should subscribe
to these to build governance dashboards, notification systems, or audit logs.

| Event | Emitted when | Key fields |
|---|---|---|
| `ListingProposed` | `propose_token_listing` succeeds | `proposal_id`, `token`, `proposer`, `rationale_hash`, `voting_deadline_ledger` |
| `ListingVoteCast` | `vote_listing` succeeds | `proposal_id`, `voter`, `approve`, `weight` |
| `ListingFinalised` | `finalise_listing_proposal` succeeds | `proposal_id`, `status` (PendingEnactment or Failed) |
| `ListingEnacted` | `enact_listing` succeeds | `proposal_id`, `token` |
| `ListingVetoed` | `veto_listing_proposal` succeeds | `proposal_id`, `reason_hash` |

---

### Error Reference

| Panic / error | What caused it | What to do |
|---|---|---|
| `GovernanceTokenNotConfigured` | `set_governance_token` has never been called. | Admin must call `set_governance_token` first. |
| `InsufficientProposerStake` | The proposer's balance is below `min_proposal_stake` at proposal time. | Acquire more governance tokens, or ask the admin to lower the stake requirement. |
| `ProposalNotFound` | The given `proposal_id` does not exist in storage. | Verify the ID; it must have been returned by a prior `propose_token_listing`. |
| `ProposalNotActive` | `vote_listing` or `finalise_listing_proposal` was called on a proposal that is no longer `Active`. | Check the proposal status; it may have already been finalised, vetoed, or enacted. |
| `VotingWindowClosed` | A vote was attempted after `voting_deadline_ledger`. | The window has passed; no more votes are accepted. |
| `VotingWindowNotClosed` | `finalise_listing_proposal` was called before `voting_deadline_ledger`. | Wait until the voting window has fully elapsed. |
| `VoteWeightExceedsBalance` | The requested `weight` exceeds the voter's balance at first-vote time (snapshot). | Use a weight ≤ the snapshot value. Acquiring more tokens after the first vote does not raise the cap. |
| `ProposalNotPendingEnactment` | `enact_listing` was called on a proposal not in `PendingEnactment` status. | Only proposals that passed quorum and have not yet been vetoed or enacted can be enacted. |
| `EnactmentDelayNotElapsed` | `enact_listing` was called before `enactment_deadline_ledger`. | Wait until the full enactment delay has passed. |
| `ProposalAlreadyTerminal` | `veto_listing_proposal` was called on a proposal already in `Enacted` or `Vetoed` state. | A token that is already `Enacted` must be removed via `remove_token` instead. |

---

### Security Considerations

#### Stake check is live only at proposal time

The proposer's balance is checked once when they submit. They are free to transfer
tokens away immediately after. This is intentional: requiring the proposer to lock
tokens would create a denial-of-service vector (someone could grief by always
submitting proposals and locking up a large stake). Governance outcome is determined
by voter weight, not proposer commitment.

#### Balance snapshot prevents flash-loan attacks on re-votes

Without the snapshot, a voter could cast a small initial vote, borrow a large number
of governance tokens, re-vote with the inflated balance, and return the borrowed tokens
— all within a single transaction. The snapshot locks the cap at the voter's balance
at first-vote time, making flash-loan amplification impossible on subsequent votes.

Note that a voter who never re-votes is still vulnerable to acquiring tokens and casting
a large single first-vote. The `min_proposal_stake` and `quorum_bps` parameters should
be tuned with governance-token supply and distribution in mind to keep any single actor
from unilaterally passing proposals.

#### Permissionless finalisation and enactment prevent griefing by inaction

If only the admin or proposer could finalise or enact, they could stall the process
indefinitely by simply not acting. Making both steps permissionless means any
stakeholder can move the proposal forward once the required time has passed.

#### The admin veto is a centralisation trade-off

The admin can veto any proposal up until the moment it is enacted. This is a deliberate
centralisation point: it provides an emergency brake for obviously bad outcomes (scam
tokens, regulatory issues) but also means the admin could theoretically block legitimate
community decisions. Governance participants should factor the enactment delay into their
expectations and understand that the admin's veto power is permanent until the contract
is upgraded.

#### Enactment idempotency prevents double-listing

If the admin adds a token directly between finalisation and enactment, the enactment
call still succeeds but skips the whitelist write. This prevents duplicate entries in
the `WhitelistedTokens` Vec and avoids confusing state.
