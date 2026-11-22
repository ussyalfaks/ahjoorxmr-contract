//! Optional payout holdback: withhold `payout_holdback_bps` of each round
//! payout until the recipient has made every remaining contribution for the
//! cycle they were paid in. Members paid early have the weakest incentive to
//! keep contributing; the holdback protects the members paid after them.
//!
//! If the member misses one of those contributions, the holdback covers the
//! missed amount (before vesting, insurance or any reserve is touched) and
//! the rest of it is forfeited to the group's insurance pool. The same
//! forfeiture applies if the member exits or is removed before the cycle ends.
//!
//! Only the base token is held back.

use crate::errors::ExtError3;
use crate::{
    charter, events, payout_split, DataKey, DataKey2, DataKey5, HoldbackRecord, HoldbackStatus,
    PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{panic_with_error, token, Address, Env, Map, Vec};

pub(crate) const MAX_HOLDBACK_BPS: u32 = 5_000;

pub(crate) fn holdback_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey5::PayoutHoldbackBps)
        .unwrap_or(0)
}

/// Sets the holdback share. Only allowed before the group is activated, so
/// every payout in the group's life is held back at the same rate.
pub(crate) fn set_holdback_bps(env: &Env, bps: u32) {
    if bps > MAX_HOLDBACK_BPS {
        panic_with_error!(env, ExtError3::InvalidHoldbackBps);
    }
    if charter::is_group_activated(env) {
        panic_with_error!(env, ExtError3::ConfigLockedAfterActivation);
    }
    env.storage()
        .instance()
        .set(&DataKey5::PayoutHoldbackBps, &bps);
    events::emit_payout_holdback_set(env, bps);
}

/// Base-token balance withheld in unsettled holdbacks. Must be excluded from
/// round pots.
pub(crate) fn locked_balance(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey5::HoldbackLocked)
        .unwrap_or(0)
}

fn adjust_locked(env: &Env, delta: i128) {
    env.storage()
        .instance()
        .set(&DataKey5::HoldbackLocked, &(locked_balance(env) + delta));
}

pub(crate) fn get_record(env: &Env, member: &Address, cycle: u32) -> Option<HoldbackRecord> {
    env.storage()
        .persistent()
        .get(&DataKey5::Holdback(member.clone(), cycle))
}

fn save_record(env: &Env, record: &HoldbackRecord) {
    let key = DataKey5::Holdback(record.member.clone(), record.cycle);
    env.storage().persistent().set(&key, record);
    env.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

fn open_holdbacks(env: &Env) -> Map<Address, u32> {
    env.storage()
        .instance()
        .get(&DataKey5::HoldbackOpen)
        .unwrap_or(Map::new(env))
}

fn save_open_holdbacks(env: &Env, open: &Map<Address, u32>) {
    env.storage().instance().set(&DataKey5::HoldbackOpen, open);
}

/// Withholds the configured share of `payout_amount` (already held by the
/// contract) from `member`'s payout for `round`. Returns the amount withheld;
/// 0 when holdback is disabled or `round` is the last round of its cycle (no
/// contributions remain to protect).
pub(crate) fn hold_back(
    env: &Env,
    member: &Address,
    payout_amount: i128,
    round: u32,
    cycle_len: u32,
) -> i128 {
    let bps = holdback_bps(env);
    if bps == 0 || cycle_len == 0 || payout_amount <= 0 {
        return 0;
    }
    let cycle = round / cycle_len;
    let last_round = (cycle + 1) * cycle_len - 1;
    if round >= last_round {
        return 0;
    }
    let amount = payout_amount * bps as i128 / 10_000;
    if amount <= 0 {
        return 0;
    }

    // A member paid twice in one cycle (e.g. after a skip) accumulates into
    // the same record.
    let record = match get_record(env, member, cycle) {
        Some(mut existing) if existing.status == HoldbackStatus::Held => {
            existing.amount += amount;
            existing.payout_round = round;
            existing
        }
        _ => HoldbackRecord {
            member: member.clone(),
            cycle,
            amount,
            payout_round: round,
            last_round,
            covered: 0,
            forfeited: 0,
            status: HoldbackStatus::Held,
        },
    };
    save_record(env, &record);
    adjust_locked(env, amount);

    let mut open = open_holdbacks(env);
    open.set(member.clone(), cycle);
    save_open_holdbacks(env, &open);

    events::emit_payout_held_back(env, round, member.clone(), cycle, amount, last_round);
    amount
}

/// Closes `record`: `covered` is unlocked into the round pot, the remainder
/// is moved to the insurance pool.
fn forfeit(env: &Env, record: &mut HoldbackRecord, covered: i128, round: u32) {
    let forfeited = record.amount - covered;
    record.covered = covered;
    record.forfeited = forfeited;
    record.status = HoldbackStatus::Forfeited;
    save_record(env, record);
    adjust_locked(env, -record.amount);

    if forfeited > 0 {
        let insurance_pool: i128 = env
            .storage()
            .instance()
            .get(&DataKey2::InsurancePool)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey2::InsurancePool, &(insurance_pool + forfeited));
    }
    events::emit_holdback_forfeited(
        env,
        record.member.clone(),
        record.cycle,
        round,
        covered,
        forfeited,
    );
}

/// The member's unsettled holdback whose cycle still expects a contribution
/// for `round`, if any.
fn holdback_due_in(
    env: &Env,
    open: &Map<Address, u32>,
    member: &Address,
    round: u32,
) -> Option<HoldbackRecord> {
    let cycle = open.get(member.clone())?;
    let record = get_record(env, member, cycle)?;
    let in_window = round > record.payout_round && round <= record.last_round;
    if record.status == HoldbackStatus::Held && in_window {
        Some(record)
    } else {
        None
    }
}

/// Uses defaulters' holdbacks to cover their missed contribution for
/// `round`, before vesting, insurance or any reserve is used. The rest of
/// each holdback used is forfeited to the group. Covered funds are credited as
/// the member's contribution. Returns the members that remain defaulters
/// (not fully covered).
pub(crate) fn cover_defaults(env: &Env, defaulters: &Vec<Address>, round: u32) -> Vec<Address> {
    let mut open = open_holdbacks(env);
    if open.is_empty() {
        return defaulters.clone();
    }
    let base_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::ContributionAmt)
        .unwrap_or(0);
    let tiers: Map<Address, u32> = env
        .storage()
        .instance()
        .get(&DataKey2::MemberTiers)
        .unwrap_or(Map::new(env));
    let mut contributions: Map<Address, i128> = env
        .storage()
        .instance()
        .get(&DataKey::MemberContributions)
        .unwrap_or(Map::new(env));
    let mut paid_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PaidMembers)
        .unwrap_or(Vec::new(env));

    let mut remaining: Vec<Address> = Vec::new(env);
    for member in defaulters.iter() {
        let mut record = match holdback_due_in(env, &open, &member, round) {
            Some(r) => r,
            None => {
                remaining.push_back(member);
                continue;
            }
        };
        let tier_bps = tiers.get(member.clone()).unwrap_or(10_000);
        let required = base_amount * tier_bps as i128 / 10_000;
        let already = contributions.get(member.clone()).unwrap_or(0);
        let owed = (required - already).max(0);
        let cover = owed.min(record.amount);

        forfeit(env, &mut record, cover, round);
        open.remove(member.clone());

        if cover > 0 {
            contributions.set(member.clone(), already + cover);
        }
        if cover == owed {
            paid_members.push_back(member.clone());
        } else {
            remaining.push_back(member.clone());
        }
    }

    save_open_holdbacks(env, &open);
    env.storage()
        .instance()
        .set(&DataKey::MemberContributions, &contributions);
    env.storage()
        .instance()
        .set(&DataKey::PaidMembers, &paid_members);
    remaining
}

/// Forfeits, in full, the holdbacks of members who left the group (exited or
/// removed) without making their contribution for `round`. Called when a
/// round is reset, whichever path ended it.
pub(crate) fn forfeit_departed(env: &Env, round: u32) {
    let mut open = open_holdbacks(env);
    if open.is_empty() {
        return;
    }
    let members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(Vec::new(env));
    let exited: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::ExitedMembers)
        .unwrap_or(Vec::new(env));
    let paid_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PaidMembers)
        .unwrap_or(Vec::new(env));

    let mut changed = false;
    for member in open.keys().iter() {
        let departed = !members.contains(&member) || exited.contains(&member);
        if !departed || paid_members.contains(&member) {
            continue;
        }
        if let Some(mut record) = holdback_due_in(env, &open, &member, round) {
            forfeit(env, &mut record, 0, round);
            open.remove(member);
            changed = true;
        }
    }
    if changed {
        save_open_holdbacks(env, &open);
    }
}

/// Pays `member`'s holdback for `cycle` to their payout beneficiary (or
/// themselves) once every contribution after their payout round in that
/// cycle has been made. Returns the amount paid.
pub(crate) fn release(env: &Env, member: &Address, cycle: u32) -> i128 {
    let mut record = match get_record(env, member, cycle) {
        Some(r) => r,
        None => panic_with_error!(env, ExtError3::NoHoldbackRecord),
    };
    if record.status != HoldbackStatus::Held {
        panic_with_error!(env, ExtError3::HoldbackSettled);
    }

    let current_round: u32 = env
        .storage()
        .instance()
        .get(&DataKey::CurrentRound)
        .unwrap_or(0);
    let paid_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PaidMembers)
        .unwrap_or(Vec::new(env));
    // Every round through `last_round` is settled, or the member has already
    // paid the last one in full.
    let releasable = current_round > record.last_round
        || (current_round == record.last_round && paid_members.contains(member));
    if !releasable {
        panic_with_error!(env, ExtError3::HoldbackNotReleasable);
    }

    record.status = HoldbackStatus::Released;
    save_record(env, &record);
    adjust_locked(env, -record.amount);

    let mut open = open_holdbacks(env);
    if open.get(member.clone()) == Some(cycle) {
        open.remove(member.clone());
        save_open_holdbacks(env, &open);
    }

    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let destinations = payout_split::resolve(env, member, record.amount);
    let client = token::Client::new(env, &token_addr);
    for (recipient, share) in destinations.iter() {
        if share > 0 {
            client.transfer(
                &env.current_contract_address(),
                &recipient,
                &share,
            );
            events::emit_holdback_released(env, member.clone(), cycle, recipient, share);
        }
    }
    record.amount
}
