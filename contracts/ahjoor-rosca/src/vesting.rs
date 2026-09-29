//! Optional payout vesting: instead of paying a round's pot out in one lump
//! sum, lock it in a `VestingRecord` that releases linearly over
//! `payout_vesting_ledgers` ledgers. This lowers the incentive for early
//! recipients to stop contributing: if they default on a later round, the
//! unvested balance covers the missed contribution before any penalty.
//!
//! Only the base token vests; other approved tokens and reinvested payouts
//! keep their existing behaviour.

use crate::charter;
use crate::errors::ExtError3;
use crate::{
    events, DataKey, DataKey2, DataKey5, ProposalType, VestingRecord, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{panic_with_error, token, Address, Env, Map, Vec};

pub(crate) fn vesting_ledgers(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey5::PayoutVestingLedgers)
        .unwrap_or(0)
}

pub(crate) fn apply_vesting_ledgers(env: &Env, ledgers: u32) {
    env.storage()
        .instance()
        .set(&DataKey5::PayoutVestingLedgers, &ledgers);
    events::emit_payout_vesting_set(env, ledgers);
}

/// Opens a `PayoutVestingUpdate` proposal; `ledgers` is applied by
/// `execute_proposal` if the vote passes.
pub(crate) fn propose_vesting_update(env: &Env, proposer: &Address, ledgers: u32) -> u32 {
    let proposal_id = charter::open_governance_proposal(
        env,
        proposer,
        ProposalType::PayoutVestingUpdate,
        "Payout vesting update",
    );
    let key = DataKey5::PendingVestingLedgers(proposal_id);
    env.storage().persistent().set(&key, &ledgers);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    events::emit_payout_vesting_update_proposed(env, proposal_id, proposer.clone(), ledgers);
    proposal_id
}

/// Called from `execute_proposal` for an approved `PayoutVestingUpdate`.
/// A proposal with no pending value attached executes as a no-op.
pub(crate) fn execute_vesting_update(env: &Env, proposal_id: u32) {
    let key = DataKey5::PendingVestingLedgers(proposal_id);
    let pending: Option<u32> = env.storage().persistent().get(&key);
    if let Some(ledgers) = pending {
        env.storage().persistent().remove(&key);
        apply_vesting_ledgers(env, ledgers);
    }
}

/// Base-token balance locked in vesting records. Must be excluded from
/// round pots.
pub(crate) fn locked_balance(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey5::VestingLocked)
        .unwrap_or(0)
}

fn adjust_locked(env: &Env, delta: i128) {
    env.storage()
        .instance()
        .set(&DataKey5::VestingLocked, &(locked_balance(env) + delta));
}

pub(crate) fn get_record(env: &Env, member: &Address) -> Option<VestingRecord> {
    env.storage()
        .persistent()
        .get(&DataKey5::VestingRecord(member.clone()))
}

fn save_record(env: &Env, record: &VestingRecord) {
    let key = DataKey5::VestingRecord(record.member.clone());
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}

/// Amount vested at the current ledger: linear in elapsed ledgers, capped
/// at what remains after default cover (`total - covered`).
pub(crate) fn vested_amount(env: &Env, record: &VestingRecord) -> i128 {
    let cap = record.total - record.covered;
    if record.duration == 0 {
        return cap;
    }
    let elapsed = env
        .ledger()
        .sequence()
        .saturating_sub(record.start)
        .min(record.duration);
    let linear = record.total * elapsed as i128 / record.duration as i128;
    linear.min(cap)
}

pub(crate) fn claimable_amount(env: &Env, record: &VestingRecord) -> i128 {
    vested_amount(env, record) - record.released
}

/// Where released funds go: the member's nominated payout beneficiary, or
/// the member themselves.
fn payout_destination(env: &Env, member: &Address) -> Address {
    env.storage()
        .instance()
        .get(&DataKey5::PayoutBeneficiary(member.clone()))
        .unwrap_or(member.clone())
}

/// Releases everything vested so far. Returns the amount transferred.
fn release(env: &Env, record: &mut VestingRecord) -> i128 {
    let amount = claimable_amount(env, record);
    if amount <= 0 {
        return 0;
    }
    record.released += amount;
    adjust_locked(env, -amount);
    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let recipient = payout_destination(env, &record.member);
    token::Client::new(env, &token_addr).transfer(
        &env.current_contract_address(),
        &recipient,
        &amount,
    );
    events::emit_vested_claimed(env, record.member.clone(), recipient, amount);
    amount
}

/// Locks `amount` (already held by the contract) for `member`. If the member
/// still has an earlier record, whatever has vested is released first and the
/// unvested remainder is rolled into the new record, which vests from now.
pub(crate) fn vest_payout(env: &Env, member: &Address, amount: i128, round: u32) {
    let duration = vesting_ledgers(env);
    let mut carried = 0i128;
    if let Some(mut previous) = get_record(env, member) {
        release(env, &mut previous);
        carried = previous.total - previous.covered - previous.released;
    }
    let start = env.ledger().sequence();
    let record = VestingRecord {
        member: member.clone(),
        total: carried + amount,
        released: 0,
        start,
        duration,
        covered: 0,
    };
    save_record(env, &record);
    adjust_locked(env, amount);
    events::emit_payout_vested(env, round, member.clone(), amount, start, duration);
}

/// Releases `member`'s vested balance to date. Returns the amount paid.
pub(crate) fn claim(env: &Env, member: &Address) -> i128 {
    let mut record = match get_record(env, member) {
        Some(r) => r,
        None => panic_with_error!(env, ExtError3::NoVestingRecord),
    };
    let amount = release(env, &mut record);
    if amount == 0 {
        panic_with_error!(env, ExtError3::NothingVested);
    }
    save_record(env, &record);
    amount
}

/// Uses defaulters' unvested balances to cover their missed contribution for
/// `round`, before any default or penalty is applied. Covered funds are
/// unlocked into the round pot and credited as the member's contribution.
/// Returns the members that remain defaulters (not fully covered).
pub(crate) fn cover_defaults(env: &Env, defaulters: &Vec<Address>, round: u32) -> Vec<Address> {
    let mut remaining: Vec<Address> = Vec::new(env);
    if locked_balance(env) <= 0 {
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

    for member in defaulters.iter() {
        let mut record = match get_record(env, &member) {
            Some(r) => r,
            None => {
                remaining.push_back(member);
                continue;
            }
        };
        let tier_bps = tiers.get(member.clone()).unwrap_or(10_000);
        let required = base_amount * tier_bps as i128 / 10_000;
        let already = contributions.get(member.clone()).unwrap_or(0);
        let owed = required - already;
        let unvested = record.total - record.covered - vested_amount(env, &record);
        let cover = owed.min(unvested);
        if cover <= 0 {
            remaining.push_back(member);
            continue;
        }

        record.covered += cover;
        save_record(env, &record);
        adjust_locked(env, -cover);
        contributions.set(member.clone(), already + cover);

        let fully_covered = cover == owed;
        if fully_covered {
            paid_members.push_back(member.clone());
        } else {
            remaining.push_back(member.clone());
        }
        events::emit_vesting_covered_default(env, member, round, cover, fully_covered);
    }

    env.storage()
        .instance()
        .set(&DataKey::MemberContributions, &contributions);
    env.storage()
        .instance()
        .set(&DataKey::PaidMembers, &paid_members);
    remaining
}
