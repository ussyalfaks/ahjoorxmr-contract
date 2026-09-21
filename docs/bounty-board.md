# Bounty Board Feature

The **bounty board** is an open escrow flow that enables anyone to post a bounty,
allowing other participants to claim the work, submit their results, and receive
payment once the bounty creator approves the submission.  
All actions are performed through the `ahjoor-escrow` contract and are fully
covered by unit tests (`test_bounty_board.rs`).

---

## Overview

| Actor | Role |
|-------|------|
| **Bounty Creator** (buyer) | Posts a bounty, funds it, and later approves or rejects the work. |
| **Claimer** (worker) | Claims an unclaimed bounty, submits work, and can be paid or have the bounty cancelled. |
| **Anyone** | Can view the bounty board (read‑only) but cannot modify a bounty unless they are the creator or the claimer. |

The flow is permission‑checked at the contract level:

* Only the **creator** can create and cancel a bounty before it is claimed.
* Only an **unclaimed** bounty can be claimed.
* Only the **claimer** can submit work for the bounty they claimed.
* Only the **creator** can approve or reject a submitted work.
* After a rejection, the creator may either cancel the bounty or allow the
  claimer to re‑submit (depending on the contract’s state).

---

## Bounty Lifecycle

### 1. `create_bounty`

