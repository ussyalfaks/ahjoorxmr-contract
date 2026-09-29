//! Contribution streak bonus: a positive incentive paid from a funded pool to
//! members who contribute on time for every round of a cycle.
//!
//! A member's streak for a cycle breaks on any late contribution, skip
//! request, missed round or emergency exit request in that cycle. Members who
//! join mid-cycle are not on the cycle roster and cannot qualify for it. When a
//! cycle completes, `streak_bonus_bps` of the pool is allocated per qualifying
//! member (capped at the whole pool) and split evenly; each member claims
//! their share once via `claim_streak_bonus`.

use crate::errors::ExtError3;
use crate::{
    events, DataKey, DataKey5, StreakCycleAllocation, StreakStatus, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{panic_with_error, token, Address, Env, Symbol, Vec};

fn bump(env: &Env, key: &DataKey5) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}

fn cycle_len(env: &Env) -> u32 {
    let order: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PayoutOrder)
        .unwrap_or(Vec::new(env));
    order.len()
}

/// Cycle (0-based) that `round` belongs to.
pub(crate) fn cycle_of_round(env: &Env, round: u32) -> u32 {
    let len = cycle_len(env);
    if len == 0 {
        0
    } else {
        round / len
    }
}

pub(crate) fn current_cycle(env: &Env) -> u32 {
    let round: u32 = env
        .storage()
        .instance()
        .get(&DataKey::CurrentRound)
        .unwrap_or(0);
    cycle_of_round(env, round)
}

pub(crate) fn pool_balance(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey5::StreakBonusPool)
        .unwrap_or(0)
}

fn allocated_balance(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey5::StreakBonusAllocated)
        .unwrap_or(0)
}

/// Base-token balance held for streak bonuses (funded + allocated but
/// unclaimed). Must be excluded from round pots.
pub(crate) fn reserved_balance(env: &Env) -> i128 {
    pool_balance(env) + allocated_balance(env)
}

pub(crate) fn bonus_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey5::StreakBonusBps)
        .unwrap_or(0)
}

pub(crate) fn set_bonus_bps(env: &Env, bps: u32) {
    if bps > 10_000 {
        panic_with_error!(env, ExtError3::InvalidStreakBonusBps);
    }
    env.storage().instance().set(&DataKey5::StreakBonusBps, &bps);
}

pub(crate) fn fund_pool(env: &Env, funder: &Address, amount: i128) {
    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    token::Client::new(env, &token_addr).transfer(
        funder,
        &env.current_contract_address(),
        &amount,
    );
    let pool = pool_balance(env) + amount;
    env.storage().instance().set(&DataKey5::StreakBonusPool, &pool);
    events::emit_streak_bonus_pool_funded(env, funder.clone(), amount, pool);
}

/// Records the members who start `cycle`; only they can qualify for it.
pub(crate) fn record_roster(env: &Env, cycle: u32, members: &Vec<Address>) {
    let key = DataKey5::StreakRoster(cycle);
    env.storage().persistent().set(&key, members);
    bump(env, &key);
}

fn broken_members(env: &Env, cycle: u32) -> Vec<Address> {
    env.storage()
        .persistent()
        .get(&DataKey5::StreakBroken(cycle))
        .unwrap_or(Vec::new(env))
}

/// Breaks `member`'s streak for `cycle`. Idempotent: only the first reason
/// is recorded and emitted.
pub(crate) fn mark_broken(env: &Env, member: &Address, cycle: u32, reason: &str) {
    let mut broken = broken_members(env, cycle);
    if broken.contains(member) {
        return;
    }
    broken.push_back(member.clone());
    let key = DataKey5::StreakBroken(cycle);
    env.storage().persistent().set(&key, &broken);
    bump(env, &key);
    events::emit_streak_broken(env, member.clone(), cycle, Symbol::new(env, reason));
}

pub(crate) fn mark_broken_current(env: &Env, member: &Address, reason: &str) {
    mark_broken(env, member, current_cycle(env), reason);
}

fn is_on_track(env: &Env, member: &Address, cycle: u32) -> bool {
    let roster: Option<Vec<Address>> =
        env.storage().persistent().get(&DataKey5::StreakRoster(cycle));
    let on_roster = match roster {
        Some(r) => r.contains(member),
        // Groups created before this feature have no roster: fall back to
        // current membership.
        None => {
            let members: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::Members)
                .unwrap_or(Vec::new(env));
            members.contains(member)
        }
    };
    if !on_roster || broken_members(env, cycle).contains(member) {
        return false;
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
    members.contains(member) && !exited.contains(member)
}

/// Called once when `cycle` completes: snapshots the eligible members and
/// moves their bonus from the pool into the allocated balance.
pub(crate) fn on_cycle_completed(env: &Env, cycle: u32) {
    let key = DataKey5::StreakAllocation(cycle);
    if env.storage().persistent().has(&key) {
        return;
    }
    let members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(Vec::new(env));
    let mut eligible: Vec<Address> = Vec::new(env);
    for member in members.iter() {
        if is_on_track(env, &member, cycle) {
            eligible.push_back(member);
        }
    }

    let n = eligible.len() as i128;
    let pool = pool_balance(env);
    let bps = bonus_bps(env) as i128;
    let per_member_amount = if n > 0 && pool > 0 && bps > 0 {
        let desired_total = pool * bps * n / 10_000;
        desired_total.min(pool) / n
    } else {
        0
    };
    let allocated_total = per_member_amount * n;
    if allocated_total > 0 {
        env.storage()
            .instance()
            .set(&DataKey5::StreakBonusPool, &(pool - allocated_total));
        env.storage().instance().set(
            &DataKey5::StreakBonusAllocated,
            &(allocated_balance(env) + allocated_total),
        );
    }

    let allocation = StreakCycleAllocation {
        cycle,
        eligible: eligible.clone(),
        per_member_amount,
        claimed_count: 0,
    };
    env.storage().persistent().set(&key, &allocation);
    bump(env, &key);
    events::emit_streak_bonus_allocated(env, cycle, eligible.len(), per_member_amount);
}

pub(crate) fn get_allocation(env: &Env, cycle: u32) -> Option<StreakCycleAllocation> {
    env.storage().persistent().get(&DataKey5::StreakAllocation(cycle))
}

fn has_claimed(env: &Env, member: &Address, cycle: u32) -> bool {
    env.storage()
        .persistent()
        .get(&DataKey5::StreakClaimed(cycle, member.clone()))
        .unwrap_or(false)
}

/// Pays `member` their share of `cycle`'s allocation. Returns the amount paid.
pub(crate) fn claim(env: &Env, member: &Address, cycle: u32) -> i128 {
    let key = DataKey5::StreakAllocation(cycle);
    let mut allocation: StreakCycleAllocation = match env.storage().persistent().get(&key) {
        Some(a) => a,
        None => panic_with_error!(env, ExtError3::StreakCycleNotCompleted),
    };
    if !allocation.eligible.contains(member) {
        panic_with_error!(env, ExtError3::StreakNotEligible);
    }
    if has_claimed(env, member, cycle) {
        panic_with_error!(env, ExtError3::StreakBonusAlreadyClaimed);
    }

    let claimed_key = DataKey5::StreakClaimed(cycle, member.clone());
    env.storage().persistent().set(&claimed_key, &true);
    bump(env, &claimed_key);
    allocation.claimed_count += 1;
    env.storage().persistent().set(&key, &allocation);
    bump(env, &key);

    let amount = allocation.per_member_amount;
    if amount > 0 {
        env.storage().instance().set(
            &DataKey5::StreakBonusAllocated,
            &(allocated_balance(env) - amount),
        );
        let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
        token::Client::new(env, &token_addr).transfer(
            &env.current_contract_address(),
            member,
            &amount,
        );
    }
    events::emit_streak_bonus_claimed(env, member.clone(), cycle, amount);
    amount
}

pub(crate) fn status(env: &Env, member: &Address) -> StreakStatus {
    let cycle = current_cycle(env);
    let mut claimable_cycle = None;
    let mut claimable_amount = 0;
    if cycle > 0 {
        let last = cycle - 1;
        if let Some(allocation) = get_allocation(env, last) {
            if allocation.eligible.contains(member) && !has_claimed(env, member, last) {
                claimable_cycle = Some(last);
                claimable_amount = allocation.per_member_amount;
            }
        }
    }
    StreakStatus {
        current_cycle: cycle,
        on_track: is_on_track(env, member, cycle),
        claimable_cycle,
        claimable_amount,
    }
}
