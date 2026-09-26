# Inspector Role in Escrow

> **Status:** Implemented in `contracts/ahjoor-escrow/src/lib.rs`
> (`create_escrow_with_inspector`, `seller_mark_complete`,
> `submit_inspection_result`, `replace_inspector`,
> `set_inspector_score_threshold`, `get_inspector_score`,
> `appeal_inspector_ruling`, `get_inspector_ruling_appealed`)
> and tested in `contracts/ahjoor-escrow/src/test_inspector.rs`.

---

## Overview

`ahjoor-escrow` supports an optional neutral **inspector** who reviews
delivered work before funds are released. When an inspector is assigned at
creation, a successful release requires the inspector to first approve the
delivery — the buyer cannot unilaterally release funds while the escrow is in
`AwaitingInspection` status.

Inspectors accumulate an on-chain **reputation score** derived from the ratio
of their rulings that were upheld vs. successfully appealed. Admins can set a
minimum score threshold above which an inspector must score in order to be
assigned to high-value escrows.

| Fact | Value |
| --- | --- |
| Inspector assignment | Optional at creation via `create_escrow_with_inspector` |
| Inspection trigger | `seller_mark_complete` → status `AwaitingInspection` |
| Inspection decision | `submit_inspection_result` |
| Score query | `get_inspector_score` → `(total_rulings, correct_rulings, accuracy_bps)` |
| Accuracy scale | Basis points (bps): `10_000` = 100 % |
| Initial accuracy (no rulings) | `10_000` bps (neutral) |
| Threshold configuration | `set_inspector_score_threshold(admin, min_bps, min_escrow_amount)` |

---

## Assigning an Inspector at Escrow Creation

```rust
pub fn create_escrow_with_inspector(
    env: Env,
    buyer: Address,
    req: EscrowCreateRequest,
    inspector: Option<Address>,
) -> u32
```

**Authentication:** `buyer.require_auth()`

Pass `Some(inspector_address)` to assign an inspector, or `None` to create a
standard escrow that skips the inspection step entirely.

**Conflict-of-interest guard:** The inspector address must differ from the
buyer and seller. Passing the buyer or seller address as inspector panics.

The inspector address is stored in `escrow.extensions.inspector`.

---

## Inspection Lifecycle

```text
  create_escrow_with_inspector(buyer, req, Some(inspector))
         │ status = Active
         ▼
  seller does work …
         │
         ▼
  seller_mark_complete(seller, escrow_id)
         │ status = AwaitingInspection
         ▼
  submit_inspection_result(inspector, escrow_id, approved, report_hash)
         │
    ┌────┴────────────────────────────────┐
    │                                    │
    ▼ approved = true                    ▼ approved = false
  status = InspectionPassed           status = InspectionFailed
    │                                    │
    ▼                                    ▼
  release_escrow(buyer, …) allowed    seller may call seller_mark_complete
                                      again to re-enter AwaitingInspection
```

### `seller_mark_complete`

```rust
pub fn seller_mark_complete(env: Env, seller: Address, escrow_id: u32)
```

**Authentication:** `seller.require_auth()`

Transitions status to `AwaitingInspection`. If the escrow has no inspector
assigned (`inspector = None`), this call transitions to a directly releasable
state instead.

Can be called again after an `InspectionFailed` verdict to allow the seller to
fix issues and resubmit for review.

### `submit_inspection_result`

```rust
pub fn submit_inspection_result(
    env: Env,
    inspector: Address,
    escrow_id: u32,
    approved: bool,
    report_hash: BytesN<32>,
) 
```

**Authentication:** `inspector.require_auth()` — only the assigned inspector
may submit. A non-inspector caller panics.

**Preconditions:**
- Escrow must be in `AwaitingInspection` status (seller must have called
  `seller_mark_complete` first).
- Cannot be called twice for the same submission: once the status has moved to
  `InspectionPassed` or `InspectionFailed`, a second call panics.

**Effect:**
- `approved = true` → status set to `InspectionPassed`; buyer may now call
  `release_escrow`.
- `approved = false` → status set to `InspectionFailed`; seller may
  resubmit via `seller_mark_complete`.
- `report_hash` is stored on-chain as the inspector's off-chain evidence
  reference.

---

## Replacing an Inspector (`replace_inspector`)

```rust
pub fn replace_inspector(env: Env, caller: Address, escrow_id: u32, new_inspector: Address)
```

**Authentication:** `caller.require_auth()` — either the buyer **or** the
seller may call this. Both parties must independently call `replace_inspector`
with the **same** `new_inspector` address before the replacement takes effect
(dual-approval).

**Effect when only one party has approved:**
- The approval is recorded but `escrow.extensions.inspector` is not yet changed.

**Effect when both parties have approved:**
- `escrow.extensions.inspector` is updated to `new_inspector`.
- The old inspector's reputation score is **penalized**: their `total_rulings`
  counter is incremented (counted as a ruling) but `correct_rulings` is not,
  reducing their accuracy.
- The new inspector starts with whatever score they had before (scores are
  global per inspector address, not per escrow).

---

## Inspector Reputation Score

Every inspector has a global reputation score composed of three counters:

| Field | Description |
| --- | --- |
| `total_rulings` | Total number of rulings recorded against this inspector. |
| `correct_rulings` | Rulings that were **not** subsequently appealed. |
| `accuracy_bps` | `(correct_rulings * 10_000) / total_rulings`. `10_000` bps = 100 %. |

```rust
pub fn get_inspector_score(env: Env, inspector: Address) -> (u32, u32, u32)
//                                                           total, correct, accuracy_bps
```

An inspector with no recorded rulings returns `(0, 0, 10_000)` — neutral.

### Score update triggers

| Action | Effect on score |
| --- | --- |
| Dispute resolved by arbiter on an escrow the inspector was assigned to | `total_rulings += 1`; first ruling initializes `correct_rulings = 1`. Subsequent rulings increment both. |
| `appeal_inspector_ruling(admin, escrow_id)` | `correct_rulings -= 1` (total unchanged). |
| `replace_inspector(…)` completes (both parties approved) | Old inspector: `total_rulings += 1`, `correct_rulings` unchanged → accuracy drops. |

### Appealing a ruling

```rust
pub fn appeal_inspector_ruling(env: Env, admin: Address, escrow_id: u32)
```

**Authentication:** Admin only.

Marks the ruling for `escrow_id` as appealed (`DataKey2::InspectorRulingAppealed(escrow_id) = true`)
and decrements the assigned inspector's `correct_rulings` by 1.

```rust
pub fn get_inspector_ruling_appealed(env: Env, escrow_id: u32) -> bool
```

Returns `true` if the ruling for this escrow has been appealed.

---

## Score Threshold for High-Value Escrows

```rust
pub fn set_inspector_score_threshold(
    env: Env,
    admin: Address,
    min_accuracy_bps: u32,
    min_escrow_amount: i128,
)
```

**Authentication:** Admin only (`OnlyAdminCanSetInspectorScoreThreshold`).

Configures a global rule: any escrow with `amount >= min_escrow_amount` that
specifies an inspector must have an inspector whose `accuracy_bps >=
min_accuracy_bps`. If the inspector's score is below the threshold,
`create_escrow_with_inspector` panics with
`InspectorScoreBelowMinimumThresholdHighValueEscrow`.

Escrows below `min_escrow_amount` are unaffected by the threshold — an
inspector with any score may be assigned.

Setting `min_escrow_amount = 0` applies the threshold to all escrows
regardless of size.

---

## Events

| Event | Emitted By | Payload | Description |
| --- | --- | --- | --- |
| `InspectionSubmitted` | `submit_inspection_result` | `(escrow_id, inspector, approved, report_hash)` | Inspector submitted their verdict. |
| `InspectorReplaced` | `replace_inspector` (on completion) | `(escrow_id, old_inspector, new_inspector)` | Both parties approved a replacement. |
| `InspectorRulingAppealed` | `appeal_inspector_ruling` | `(escrow_id, inspector)` | Admin appealed an inspector's ruling. |

---

## Error Codes

| Code | Name | Description |
| --- | --- | --- |
| 18 | `OnlyAdminCanSetInspectorScoreThreshold` | Non-admin called `set_inspector_score_threshold`. |
| 22 | `InspectorScoreBelowMinimumThresholdHighValueEscrow` | Inspector score too low for this escrow's value. |
| — | `EscrowIsNotAwaitingInspection` | `submit_inspection_result` called when escrow is not in `AwaitingInspection` status. |
| — | `NotAssignedInspector` | Caller is not the assigned inspector. |
| — | `InspectorConflictOfInterest` | Inspector address matches buyer or seller. |

---

## Function Reference

| Function | Caller | Purpose |
| --- | --- | --- |
| `create_escrow_with_inspector(buyer, req, inspector)` | Buyer | Creates an escrow with an optional inspector. |
| `seller_mark_complete(seller, escrow_id)` | Seller | Signals work is done; transitions to `AwaitingInspection`. |
| `submit_inspection_result(inspector, escrow_id, approved, report_hash)` | Inspector | Records the inspection verdict and report hash. |
| `replace_inspector(caller, escrow_id, new_inspector)` | Buyer or Seller | Votes to replace the current inspector (requires both parties). |
| `set_inspector_score_threshold(admin, min_bps, min_amount)` | Admin | Sets the minimum accuracy score required for high-value escrows. |
| `get_inspector_score(inspector)` | Anyone | Returns `(total_rulings, correct_rulings, accuracy_bps)` for an inspector. |
| `appeal_inspector_ruling(admin, escrow_id)` | Admin | Marks a ruling as overturned and decrements the inspector's correct count. |
| `get_inspector_ruling_appealed(escrow_id)` | Anyone | Returns `true` if the ruling for this escrow was appealed. |

---

## Storage Reference

| Key | Storage Type | Data Stored |
| --- | --- | --- |
| `DataKey::Escrow(escrow_id).extensions.inspector` | Persistent | `Option<Address>` assigned inspector. |
| `DataKey2::InspectorScore(inspector)` | Persistent | `(u32, u32)` tuple of `(total_rulings, correct_rulings)`. |
| `DataKey2::MinInspectorScoreBps` | Instance | `u32` minimum accuracy threshold in bps. |
| `DataKey2::MinInspectorScoreAmount` | Instance | `i128` escrow amount above which threshold applies. |
| `DataKey2::InspectorRulingAppealed(escrow_id)` | Persistent | `bool` flag set when a ruling is appealed. |
| `DataKey2::InspectorReplaceApproval(escrow_id, caller)` | Persistent | `Address` proposed replacement inspector per caller. |
