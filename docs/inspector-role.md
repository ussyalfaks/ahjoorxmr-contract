# Inspector Role and Scoring System

This document describes the optional third-party inspector role in the `ahjoor-escrow` contract, covering how an inspector is assigned, what authority they hold, and how their on-chain reputation score is calculated and enforced.

---

## 1. What is an Inspector?

An inspector is an optional, neutral third party added to an escrow at creation time. They act as a quality gate between the seller completing their work and the buyer releasing funds. When an inspector is present, the seller cannot simply be paid upon self-declaring completion — the inspector must sign off first.

Inspectors are distinct from arbiters. An arbiter resolves disputes after both parties disagree; an inspector performs a structured review pass before any dispute arises.

---

## 2. Assigning an Inspector

An inspector is set at escrow creation using `create_escrow_with_inspector`:

```
create_escrow_with_inspector(buyer, request, inspector: Option<Address>) → escrow_id
```

- `inspector` is optional. Pass `None` to create a standard escrow with no inspection gate.
- The inspector address is stored in `escrow.extensions.inspector`.
- The inspector **cannot be the buyer or seller** of the same escrow. Attempting to assign a party to the escrow as its own inspector will be rejected (conflict-of-interest guard).
- If the contract has a score threshold configured (see section 4), the inspector's accuracy score is checked at creation time. A low-scored inspector will be blocked from high-value escrows before the escrow is created.

---

## 3. Inspector Responsibilities and Powers

### 3.1 Triggering the Inspection Gate

Once an escrow is active and the seller has finished their work, the seller calls:

```
seller_mark_complete(seller, escrow_id)
```

This transitions the escrow from `Active` → `AwaitingInspection`. The buyer **cannot release funds** while the escrow is in `AwaitingInspection` state.

> If no inspector was assigned, `seller_mark_complete` is not available — the seller should not use it and the buyer releases directly via `release_escrow`.

### 3.2 Submitting an Inspection Result

The assigned inspector submits their verdict via:

```
submit_inspection_result(inspector, escrow_id, approved: bool, report_hash: BytesN<32>)
```

- Only the **assigned inspector** may call this. Any other address is rejected with `OnlyAssignedInspectorCanSubmitReport`.
- `approved = true` → status moves to `InspectionPassed`. The buyer may then call `release_escrow` to complete the payment.
- `approved = false` → status moves to `InspectionFailed`. The buyer cannot release. The seller may address the issues and call `seller_mark_complete` again to resubmit for a second inspection.
- `report_hash` is a 32-byte hash of the off-chain inspection report, stored on-chain for auditability.
- Once a result is submitted, it is final for that inspection round. Submitting a second result to the same escrow in the same `AwaitingInspection` cycle is rejected.

### 3.3 Escrow Status Flow with Inspector

```
Active
  └─ seller_mark_complete()
       └─ AwaitingInspection
            ├─ submit_inspection_result(approved=true)  → InspectionPassed → release_escrow() → Released
            └─ submit_inspection_result(approved=false) → InspectionFailed → seller_mark_complete() → AwaitingInspection (repeat)
```

### 3.4 Replacing an Inspector

Both parties (buyer **and** seller) may agree to replace the current inspector via:

```
replace_inspector(caller, escrow_id, new_inspector)
```

- The caller must be either the buyer or the seller.
- Replacement requires **both** parties to call this function with the same `new_inspector`. A single-party call is recorded but does not take effect.
- Once both parties have signed, the old inspector is replaced, the `InspectorReplacement` record is cleared, and if the escrow was stuck in `AwaitingInspection` or `InspectionFailed`, it is reset to `Active` so the new inspector can start fresh.
- Being replaced **penalizes the old inspector's score** (see section 4.3).

---

## 4. Accuracy Score

Every inspector has a persistent on-chain score stored as `InspectorScore { total_rulings: u32, correct_rulings: u32 }`.

### 4.1 Reading the Score

```
get_inspector_score(inspector) → (total_rulings, correct_rulings, accuracy_bps)
```

- `total_rulings` — number of escrows counted against this inspector.
- `correct_rulings` — number of rulings that have not been overturned on appeal.
- `accuracy_bps` — `(correct_rulings * 10_000) / total_rulings`, expressed in basis points (0–10,000).
- An inspector with no recorded rulings returns `(0, 0, 10_000)` — new inspectors start with full neutral trust.

### 4.2 When the Score Updates

| Event | Effect on score |
|---|---|
| Arbiter resolves a dispute on an escrow that had an inspector | `total_rulings += 1`; `correct_rulings += 1` (first ruling initialises to 1/1) |
| Admin calls `appeal_inspector_ruling(admin, escrow_id)` | `correct_rulings -= 1` (floor 0); `total_rulings` unchanged |
| Inspector is replaced via `replace_inspector` before a ruling | `total_rulings += 1`; `correct_rulings` unchanged (counts as a missed/incomplete ruling) |

**First-ruling initialisation:** On the very first ruling for an inspector, `total_rulings` is set to 1 and `correct_rulings` is set to 1 (not incremented from 0). This gives new inspectors a neutral starting point rather than penalising them immediately.

**Accuracy formula:**

```
accuracy_bps = (correct_rulings * 10_000) / total_rulings
```

Examples:

| total | correct | accuracy_bps | % |
|---|---|---|---|
| 0 | 0 | 10,000 (neutral) | 100% |
| 1 | 1 | 10,000 | 100% |
| 3 | 2 | 6,666 | 66.7% |
| 4 | 2 | 5,000 | 50% |
| 1 | 0 | 0 | 0% |

### 4.3 Appeals

The admin may overturn a ruling via `appeal_inspector_ruling`:

```
appeal_inspector_ruling(admin, escrow_id)
```

- Only callable by the contract admin.
- Each escrow can only be appealed once. A second appeal attempt on the same escrow panics with `InspectorRulingAlreadyAppealedEscrow`.
- Appeals decrement `correct_rulings` (floor at 0) without touching `total_rulings`, thereby lowering the accuracy score.

### 4.4 Score Threshold Gating

The admin can configure a minimum accuracy requirement for high-value escrows:

```
set_inspector_score_threshold(admin, min_score_bps: u32, value_threshold: i128)
```

- `min_score_bps` — minimum accuracy an inspector must have (0–10,000 bps). Cannot exceed 10,000.
- `value_threshold` — escrow amount above which the threshold applies. Set to `0` to disable gating entirely.

When `create_escrow_with_inspector` is called:

1. If `value_threshold == 0` or `escrow.amount <= value_threshold` → no check, inspector is accepted.
2. If the inspector has **no prior rulings** (`total_rulings == 0`) → accepted (new inspectors are not pre-emptively blocked).
3. If `accuracy_bps < min_score_bps` → rejected with `InspectorScoreBelowMinimumThresholdHighValueEscrow`.

This means an inspector who has accumulated a poor track record cannot be assigned to high-value escrows until their score recovers (which requires correct rulings on lower-value ones where the gate does not apply).

---

## 5. Error Reference

| Code | Name | Meaning |
|---|---|---|
| 11 | `OnlySellerCanMarkComplete` | `seller_mark_complete` called by someone other than the escrow's seller |
| 13 | `NoInspectorSetUseReleaseEscrowDirectly` | `seller_mark_complete` called on an escrow that has no inspector |
| 14 | `EscrowIsNotAwaitingInspection` | `submit_inspection_result` called when escrow is not in `AwaitingInspection` |
| 15 | `OnlyAssignedInspectorCanSubmitReport` | Caller of `submit_inspection_result` is not the assigned inspector |
| 16 | `OnlyBuyerOrSellerCanProposeInspectorReplacement` | `replace_inspector` called by someone other than buyer or seller |
| 17 | `NoInspectorSetEscrow` | `replace_inspector` called on an escrow with no inspector |
| 18 | `OnlyAdminCanSetInspectorScoreThreshold` | `set_inspector_score_threshold` called by non-admin |
| 19 | `MinScoreBpsExceedsMaximum` | `min_score_bps` > 10,000 passed to `set_inspector_score_threshold` |
| 20 | `OnlyAdminCanAppealInspectorRuling` | `appeal_inspector_ruling` called by non-admin |
| 21 | `InspectorRulingAlreadyAppealedEscrow` | Escrow has already had its ruling appealed |
| 22 | `InspectorScoreBelowMinimumThresholdHighValueEscrow` | Inspector accuracy is below the configured minimum for high-value escrows |
| 34 | `InspectionPending` | Operation attempted while inspection is still pending |
