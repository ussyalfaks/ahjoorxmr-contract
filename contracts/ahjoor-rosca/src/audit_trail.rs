use crate::{events, ContributionEntry, CycleRecord, DataKey, DataKey2, DataKey5};
use soroban_sdk::{Address, Env, Vec};

const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

/// Default retention window: keep 100 cycles in persistent storage
const DEFAULT_RETENTION_WINDOW: u32 = 100;

/// #544: maximum number of cycles `get_member_contribution_history` will scan
/// in a single call, so cost stays bounded regardless of total group history.
pub(crate) const MAX_CONTRIBUTION_HISTORY_RANGE: u32 = 100;
/// Default cycles scanned when history bounds are omitted.
pub(crate) const DEFAULT_CONTRIBUTION_HISTORY_WINDOW: u32 = 25;

/// Records a complete cycle audit trail atomically at round closure.
/// This captures all significant events: contributions, payouts, defaults, skips, and penalties.
pub(crate) fn record_cycle_audit(
    env: &Env,
    cycle_number: u32,
    total_pool_amount: i128,
    payout_recipient: Address,
    payout_amount: i128,
    contributions: Vec<ContributionEntry>,
    defaulters: Vec<Address>,
    skippers: Vec<Address>,
    penalties_collected: i128,
    fee_collected: i128,
    insurance_drawn: i128,
    cycle_start_timestamp: u64,
    cycle_end_timestamp: u64,
) {
    let record = CycleRecord {
        cycle_number,
        total_pool_amount,
        payout_recipient: payout_recipient.clone(),
        payout_amount,
        contributions,
        defaulters,
        skippers,
        penalties_collected,
        fee_collected,
        insurance_drawn,
        cycle_start_timestamp,
        cycle_end_timestamp,
    };

    // #544: store this cycle under its own key instead of a single blob Map
    // shared by every cycle, so reads/writes cost O(1) regardless of how
    // many cycles the group has completed.
    let entry_key = DataKey5::CycleRecordEntry(cycle_number);
    env.storage().persistent().set(&entry_key, &record);
    env.storage()
        .persistent()
        .extend_ttl(&entry_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

    if !env.storage().persistent().has(&DataKey5::OldestPersistentCycle) {
        env.storage()
            .persistent()
            .set(&DataKey5::OldestPersistentCycle, &cycle_number);
    }

    events::emit_cycle_record_created(env, cycle_number, total_pool_amount, payout_recipient);

    // Check if archival is needed
    archive_old_records(env, cycle_number);
}

/// Archives old cycle records to temporary storage based on retention window.
/// Records older than the retention window are moved from persistent to temporary storage.
///
/// Walks forward from the last known oldest persistent cycle rather than
/// scanning every persisted cycle, so cost stays bounded (amortized O(1) per
/// call in the common case of the current cycle advancing by one each round).
fn archive_old_records(env: &Env, current_cycle: u32) {
    let retention_window: u32 = env
        .storage()
        .persistent()
        .get(&DataKey2::CycleRecordRetentionWindow)
        .unwrap_or(DEFAULT_RETENTION_WINDOW);

    if current_cycle <= retention_window {
        return; // Not enough cycles to archive yet
    }

    let archive_threshold = current_cycle - retention_window;

    let oldest: u32 = env
        .storage()
        .persistent()
        .get(&DataKey5::OldestPersistentCycle)
        .unwrap_or(0);

    if oldest >= archive_threshold {
        return;
    }

    for cycle_num in oldest..archive_threshold {
        let entry_key = DataKey5::CycleRecordEntry(cycle_num);
        if let Some(record) = env.storage().persistent().get::<_, CycleRecord>(&entry_key) {
            // #543: archived records live in persistent storage (not temporary) so
            // their TTL is governed by the same long-lived bump strategy as the
            // rest of the audit trail, and does not silently expire and get
            // deleted just because no new cycle was archived for a while.
            let archived_key = DataKey5::ArchivedCycleRecordEntry(cycle_num);
            env.storage().persistent().set(&archived_key, &record);
            env.storage().persistent().extend_ttl(
                &archived_key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            env.storage().persistent().remove(&entry_key);
            events::emit_cycle_record_archived(env, cycle_num);
        }
    }

    env.storage()
        .persistent()
        .set(&DataKey5::OldestPersistentCycle, &archive_threshold);
}

/// Retrieves a cycle record from either persistent or archived storage.
pub(crate) fn get_cycle_record(env: &Env, cycle_number: u32) -> Option<CycleRecord> {
    if let Some(record) = env
        .storage()
        .persistent()
        .get::<_, CycleRecord>(&DataKey5::CycleRecordEntry(cycle_number))
    {
        return Some(record);
    }

    env.storage()
        .persistent()
        .get(&DataKey5::ArchivedCycleRecordEntry(cycle_number))
}

/// Returns the contribution entries for `member` within cycles
/// `from_cycle..=to_cycle` (inclusive), checking persistent then archived
/// storage for each cycle in the range.
///
/// #544: bounded by `MAX_CONTRIBUTION_HISTORY_RANGE` so a single call's cost
/// is capped by the requested window rather than the group's total history —
/// callers page through older history with successive calls.
pub(crate) fn get_member_contribution_history(
    env: &Env,
    member: Address,
    from_cycle: Option<u32>,
    to_cycle: Option<u32>,
) -> Vec<ContributionEntry> {
    let current_round: u32 = env
        .storage()
        .instance()
        .get(&DataKey::CurrentRound)
        .unwrap_or(0);
    let latest_cycle = current_round.saturating_sub(1);
    let default_span = DEFAULT_CONTRIBUTION_HISTORY_WINDOW.saturating_sub(1);

    let (from_cycle, to_cycle) = match (from_cycle, to_cycle) {
        (Some(from), Some(to)) => (from, to),
        (Some(from), None) => (from, from.saturating_add(default_span)),
        (None, Some(to)) => (to.saturating_sub(default_span), to),
        (None, None) => (latest_cycle.saturating_sub(default_span), latest_cycle),
    };

    if from_cycle > to_cycle {
        panic!("from_cycle must not exceed to_cycle");
    }
    if to_cycle - from_cycle >= MAX_CONTRIBUTION_HISTORY_RANGE {
        panic!("cycle range exceeds maximum allowed");
    }

    let mut history = Vec::new(env);
    for cycle_num in from_cycle..=to_cycle {
        if let Some(record) = get_cycle_record(env, cycle_num) {
            for contribution in record.contributions.iter() {
                if contribution.member == member {
                    history.push_back(contribution);
                }
            }
        }
    }

    history
}

/// Updates the retention window for cycle records. Admin only.
pub(crate) fn set_retention_window(env: &Env, new_window: u32) {
    let old_window: u32 = env
        .storage()
        .persistent()
        .get(&DataKey2::CycleRecordRetentionWindow)
        .unwrap_or(DEFAULT_RETENTION_WINDOW);

    env.storage()
        .persistent()
        .set(&DataKey2::CycleRecordRetentionWindow, &new_window);
    env.storage().persistent().extend_ttl(
        &DataKey2::CycleRecordRetentionWindow,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );

    events::emit_retention_window_updated(env, old_window, new_window);
}

/// Gets the current retention window setting.
pub(crate) fn get_retention_window(env: &Env) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey2::CycleRecordRetentionWindow)
        .unwrap_or(DEFAULT_RETENTION_WINDOW)
}

/// Records the start timestamp for a cycle.
pub(crate) fn record_cycle_start(env: &Env, cycle_number: u32, timestamp: u64) {
    let mut timestamps: soroban_sdk::Map<u32, u64> = env
        .storage()
        .instance()
        .get(&DataKey2::CycleStartTimestamps)
        .unwrap_or(soroban_sdk::Map::new(env));

    timestamps.set(cycle_number, timestamp);
    env.storage()
        .instance()
        .set(&DataKey2::CycleStartTimestamps, &timestamps);
}

/// Gets the start timestamp for a cycle.
pub(crate) fn get_cycle_start_timestamp(env: &Env, cycle_number: u32) -> u64 {
    let timestamps: soroban_sdk::Map<u32, u64> = env
        .storage()
        .instance()
        .get(&DataKey2::CycleStartTimestamps)
        .unwrap_or(soroban_sdk::Map::new(env));

    timestamps.get(cycle_number).unwrap_or(0)
}
