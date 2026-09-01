# ROSCA Audit Trail

The `ahjoor-rosca` contract maintains an on-chain audit trail for every completed ROSCA cycle. The trail is implemented in [`contracts/ahjoor-rosca/src/audit_trail.rs`](../contracts/ahjoor-rosca/src/audit_trail.rs) and exercised by [`contracts/ahjoor-rosca/src/test_audit_trail.rs`](../contracts/ahjoor-rosca/src/test_audit_trail.rs).

## What is recorded

Each time a cycle closes, the contract writes a `CycleRecord` containing:

| Field | Description |
|-------|-------------|
| `cycle_number` | The completed ROSCA round number. |
| `total_pool_amount` | Total assets pooled for the cycle. |
| `payout_recipient` | Member who received the cycle payout. |
| `payout_amount` | Amount disbursed to the payout recipient. |
| `contributions` | List of `ContributionEntry` records for every member who contributed. |
| `defaulters` | Members who failed to contribute in this cycle. |
| `skippers` | Members who formally skipped this cycle. |
| `penalties_collected` | Total penalty assets collected from defaulters. |
| `fee_collected` | Platform or organizer fees collected for the cycle. |
| `insurance_drawn` | Insurance amount used to cover shortfalls. |
| `cycle_start_timestamp` | Ledger timestamp when the cycle started. |
| `cycle_end_timestamp` | Ledger timestamp when the cycle ended. |

This single record captures the full financial and membership state of a closed cycle.

## Storage layout

Records are stored in **persistent storage** under:

- `DataKey5::CycleRecordEntry(cycle_number)` for cycles inside the retention window.
- `DataKey5::ArchivedCycleRecordEntry(cycle_number)` for cycles that have been archived.

The contract also tracks:

- `DataKey5::OldestPersistentCycle` — the oldest cycle number still in the "hot" persistent set.
- `DataKey2::CycleRecordRetentionWindow` — how many recent cycles are kept hot (default `100`).

### Archival

When a new cycle is recorded, `archive_old_records` moves any cycle older than `current_cycle - retention_window` from `CycleRecordEntry` to `ArchivedCycleRecordEntry`. Archived records remain in persistent storage with bumped TTL, so they are not silently deleted.

## How to query the audit log

The audit trail exposes two read functions (used by the contract's public query methods):

### `get_cycle_record(env, cycle_number)`

Returns `Option<CycleRecord>` for the requested cycle, looking first in the hot persistent entry and then in the archived entry.

### `get_member_contribution_history(env, member, from_cycle, to_cycle)`

Returns a `Vec<ContributionEntry>` for a specific member across an inclusive cycle range.

- If both bounds are `None`, the function scans the most recent `DEFAULT_CONTRIBUTION_HISTORY_WINDOW` cycles (default `25`).
- If one bound is `None`, the other is inferred using the default window.
- If both bounds are provided, they are used directly.
- The range is capped at `MAX_CONTRIBUTION_HISTORY_RANGE` (default `100`) to bound execution cost. Callers page through history with successive calls.

## Retention window administration

Two helpers manage how many cycles stay hot:

- `set_retention_window(env, new_window)` — admin-only; updates `CycleRecordRetentionWindow`.
- `get_retention_window(env)` — returns the current window, defaulting to `100`.

Lowering the window causes older cycles to be archived earlier; raising it keeps more cycles hot.

## Events

The audit trail emits the following contract events:

| Event | When emitted | Payload |
|-------|--------------|---------|
| `cycle_record_created` | A cycle record is written at round closure. | `cycle_number`, `total_pool_amount`, `payout_recipient` |
| `cycle_record_archived` | A cycle record is moved from hot to archived storage. | `cycle_number` |
| `retention_window_updated` | The retention window is changed by an admin. | `old_window`, `new_window` |

## Cost and paging notes

- Writing a cycle record is `O(1)` because each cycle gets its own storage key.
- Archival is amortized `O(1)` when cycles advance by one at a time.
- History reads are bounded by the requested range (`<= 100` cycles) and by the number of contributions in those cycles.
