# Sealed-Bid Slot Auctions

## Overview

`ahjoor-rosca` lets a group decide the order in which members receive their
payout slots using a **commit-reveal sealed-bid auction**. Instead of a public
first-come-first-served race, members privately commit to the amount they are
willing to pay for a slot, then reveal their bids once the commit phase closes.
Because bids are hidden during the commit phase, no member can see (and
out-bid) another member's offer before the deadline.

The auction is driven by the following entry points in
`contracts/ahjoor-rosca/src/lib.rs`:

| Function | Purpose |
| --- | --- |
| `configure_sealed_slot_auction` | Set up an auction for a round: reserve price, commit/reveal windows, and slot count. |
| `open_sealed_slot_auction` | Open the commit phase so members can start submitting sealed bids. |
| `settle_sealed_slot_auction` | Close the auction and assign slots to the winning bids. |
| `get_sealed_auction` | Read the full auction configuration and state. |
| `get_auction_status` | Read the current phase (commit, reveal, settled, etc.). |

## Lifecycle: commit -> reveal -> settle

A sealed-bid slot auction moves through three phases.

### 1. Commit

After the auction is configured and opened, members enter the **commit phase**.
Each member submits a sealed bid by committing to a hash of their bid amount
(and any required nonce) rather than the amount itself. The contract stores the
commitment without revealing the underlying value, so the bid stays private for
the duration of the commit window.

### 2. Reveal

Once the commit window closes, the **reveal phase** begins. Members who wish to
be considered must reveal their bid by submitting the original amount and nonce.
The contract hashes the revealed values and checks them against the stored
commitment. A reveal that does not match its commitment is rejected, so a member
cannot change their bid after seeing others.

### 3. Settle

When the reveal window closes, the auction is **settled**. The contract ranks the
valid revealed bids and assigns the available payout slots to the winners. The
auction state and status are then readable via `get_sealed_auction` and
`get_auction_status`.

## Minimum reserve mechanic

Each auction is configured with a **minimum reserve** price. A revealed bid only
qualifies for a slot if it meets or exceeds the reserve. Bids below the reserve
are not eligible to win, which protects the group from awarding slots for less
than the agreed floor. The reserve is set when the auction is configured with
`configure_sealed_slot_auction` and is enforced at settlement time.

## Unrevealed commits

A commitment that is never revealed during the reveal phase cannot be counted as
a valid bid. Because the contract only has the hash and not the underlying
amount, an unrevealed commit is treated as a non-participating bid: it is not
ranked and cannot win a slot. Members who commit but fail to reveal simply forgo
their chance at a slot for that auction; the reserve and the remaining valid bids
determine the outcome.

## See also

- Tests: `contracts/ahjoor-rosca/src/test_sealed_slot_auction.rs`
- Implementation: `contracts/ahjoor-rosca/src/lib.rs`
