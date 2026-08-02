# Seller Veto Mechanism

The **Seller Veto** allows a seller in an active escrow dispute to temporarily halt auto-resolution or immediate buyer refund triggers, giving both parties time to negotiate or submit evidence for platform arbitration.

---

## 1. Scope & Blocked Actions

When a seller exercises a veto on a disputed transaction, the contract temporarily blocks:

* **Automated Buyer Refund Execution:** Prevents automatic release of escrowed funds to the buyer upon dispute timer expiration.
* **Immediate Dispute Closure:** Prevents unilateral dispute resolution by the buyer while the veto is active.
* **Fund Release to Buyer:** Holds the disputed escrow balance in contract storage for the duration of the veto window.

> **Note:** A veto **does not** cancel the dispute or transfer funds to the seller; it merely pauses unilateral buyer refund execution.

---

## 2. Duration & Cooldown Rules

To prevent sellers from indefinitely locking escrow funds, vetoes are governed by duration, cooldown, and usage limits:

| Parameter | Type | Description | Default / Limit |
| :--- | :--- | :--- | :--- |
| `veto_duration` | `u64` (seconds / ledgers) | Time window during which buyer refund execution is blocked. | Configurable (e.g., 7 days) |
| `veto_cooldown` | `u64` (seconds / ledgers) | Mandatory waiting period after a veto expires before another veto can be triggered. | Configurable (e.g., 14 days) |
| `max_vetoes_per_dispute` | `u32` | Maximum number of vetoes allowed on a single dispute. | **1 veto per dispute** |

### Boundary & Expiration Behavior

1. **Active Window:** Between `veto_timestamp` and `veto_timestamp + veto_duration`, buyer refund calls will fail with `ContractError::VetoActive`.
2. **Automatic Expiration:** Once `current_timestamp > veto_timestamp + veto_duration`, the veto expires automatically without requiring an explicit transaction.
3. **Cooldown Enforcement:** A seller cannot issue a subsequent veto until `current_timestamp >= last_veto_timestamp + veto_cooldown`.

---

## 3. Resolution Pathways

An active seller veto can be resolved through three pathways:

+-----------------------------------+
              |      Active Seller Veto           |
              +-----------------------------------+
                                |
     +--------------------------+--------------------------+
     |                          |                          |
     v                          v                          v


1. **Veto Expiration (Time-based):**
   * Once `veto_duration` elapses without an admin ruling or settlement, the veto state clears and the standard dispute flow resumes.
2. **Arbitrator / Admin Override:**
   * An authorized platform arbitrator or contract admin can call `force_resolve_dispute` at any time.
   * Admin resolution bypasses active veto locks and immediately disburses funds based on the ruling.
3. **Seller Cancellation:**
   * The seller can manually withdraw their active veto by calling `cancel_veto`.

---

## 4. Code References

* **Entrypoint Implementations:** `contracts/escrow/src/lib.rs`
  * `veto_dispute(env, seller, dispute_id)` — Validates seller identity, cooldown state, and applies the veto lock.
  * `cancel_veto(env, seller, dispute_id)` — Removes active veto lock before duration elapses.
  * `resolve_dispute(env, resolver, dispute_id, outcome)` — Evaluates veto status before permitting resolution.
* **Test Suite:** `contracts/escrow/src/tests/test_seller_veto.rs`
