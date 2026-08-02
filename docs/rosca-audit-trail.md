# ROSCA Audit Trail

The ROSCA contract records a cycle-level audit trail when a round closes and its final payout state is snapshotted. This gives group members and admins a consistent, queryable history of what happened during each cycle.

## What is recorded

Each closed cycle produces a `CycleRecord` snapshot that contains:

- `cycle_number`: the cycle/round being finalized
- `total_pool_amount`: the total contributions collected for that cycle
- `payout_recipient`: the address that received the payout for the cycle
- `payout_amount`: the amount paid out to the payout recipient
- `contributions`: per-member contributions for that cycle, with:
    - `member`
    - `amount`
    - `timestamp`
- `defaulters`: members who failed to contribute before the round closed
- `skippers`: members who were recorded as skipping the cycle
- `penalties_collected`: total penalties collected during the cycle
- `fee_collected`: protocol/fee amount captured for the cycle
- `insurance_drawn`: amount pulled from the insurance pool for the cycle
- `cycle_start_timestamp`: the cycle start time
- `cycle_end_timestamp`: the cycle end time

In practice, this means the audit trail is meant to answer questions such as:

- who contributed in a given cycle
- how much was collected and paid out
- who defaulted or skipped
- how much fee and insurance activity occurred
- when each cycle started and ended

## When the audit trail is written

The audit trail is written at round closure/finalization time, not continuously during member activity. That makes the stored record a complete, immutable snapshot for each completed cycle.

The recording flow is implemented in the ROSCA contract’s audit trail module and is invoked from the round finalization path.

## How to query the audit trail

The public ROSCA contract exposes the following query methods:

### 1. Get a single cycle record

```rust
get_cycle_record(cycle_number)
```

Returns the full `CycleRecord` for the requested cycle number if it is still available in persistent or archived temporary storage.

### 2. Get a member’s contribution history

```rust
get_member_contribution_history(member)
```

Returns all `ContributionEntry` records for that member across all recorded cycles.

### 3. Get the configured retention window

```rust
get_cycle_retention_window()
```

Returns the number of recent cycles the contract keeps in persistent storage.

### 4. Update the retention window

```rust
set_cycle_retention_window(new_window)
```

This is an admin-only operation.

## Retention and archival behavior

The ROCSA audit trail uses a two-tier retention model:

1. Recent cycle records remain in persistent storage.
2. Older cycle records are moved to temporary archived storage once the configured retention window is exceeded.

The default retention window is `100` cycles.

### What this means in practice

- The latest cycles stay readily queryable in persistent storage.
- Older records are not deleted outright by the application logic; they are archived to temporary storage so they can still be accessed through the same query methods.
- The contract can still return a historical cycle record by cycle number even after it has aged out of the active persistent set.

### Important distinction: application retention vs. chain archival

This ROSCA feature is separate from Stellar/Soroban state archival. The contract’s own `CycleRecord` retention window controls how many recent records remain in persistent contract storage. Stellar state archival is a network-level storage lifecycle concern for inactive contract data.

For the chain-level behavior, see [state-archival.md](state-archival.md).

## Does the audit trail get pruned?

Not permanently, at least not by the ROSCA application code. The contract implements a retention/archival flow:

- keep the most recent `N` cycles in persistent storage
- move older cycle records into temporary storage

That means the audit trail is designed to be historical and queryable, but it is intentionally bounded by the configured retention window for active storage.

## Operational note

If a ROSCA contract is affected by Stellar/Soroban state archival, the state can be restored without redeploying the contract. For that workflow, refer to [state-archival.md](state-archival.md).
