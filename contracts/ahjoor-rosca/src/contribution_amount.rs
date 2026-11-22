//! Governance-driven contribution amount changes. A passed
//! `ChangeContributionAmount` proposal is scheduled for the next cycle and
//! applied at that cycle's first round; the current cycle is never altered.

use crate::errors::{Error, ExtError3};
use crate::{events, DataKey, DataKey3, DataKey5, PendingContributionChange};
use soroban_sdk::{panic_with_error, Address, Env, Map, Vec};

pub(crate) fn get_pending(env: &Env) -> Option<PendingContributionChange> {
    env.storage()
        .instance()
        .get(&DataKey5::PendingContributionChange)
}

pub(crate) fn current_round(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::CurrentRound)
        .unwrap_or(0)
}

pub(crate) fn cycle_len(env: &Env) -> u32 {
    let payout_order: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PayoutOrder)
        .unwrap_or(Vec::new(env));
    payout_order.len() as u32
}

pub(crate) fn current_cycle(env: &Env) -> u32 {
    let len = cycle_len(env);
    if len == 0 {
        0
    } else {
        current_round(env) / len
    }
}

pub(crate) fn validate(env: &Env, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, ExtError3::InvalidContributionAmount);
    }

    let members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(Vec::new(env));
    if amount.checked_mul(members.len() as i128).is_none() {
        panic_with_error!(env, ExtError3::ContributionAmountOverflow);
    }

    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .expect("Not initialized");
    let limits: Map<Address, i128> = env
        .storage()
        .instance()
        .get(&DataKey::TokenLimits)
        .unwrap_or(Map::new(env));
    if let Some(limit) = limits.get(token) {
        if amount > limit {
            panic_with_error!(env, Error::ExceedsTokenLimit);
        }
    }
}

/// Schedules `amount` for the next cycle. Returns
/// `(effective_cycle, effective_round)`.
pub(crate) fn schedule(env: &Env, proposal_id: u32, amount: i128) -> (u32, u32) {
    validate(env, amount);

    let len = cycle_len(env);
    if len == 0 {
        panic_with_error!(env, ExtError3::InvalidContributionAmount);
    }
    let next_cycle = current_cycle(env) + 1;
    let effective_round = next_cycle * len;

    if let Some(pending) = get_pending(env) {
        if pending.effective_cycle >= next_cycle {
            panic_with_error!(env, ExtError3::ContributionAmountChangePending);
        }
    }

    env.storage().instance().set(
        &DataKey5::PendingContributionChange,
        &PendingContributionChange {
            amount,
            effective_cycle: next_cycle,
            proposal_id,
        },
    );
    events::emit_contribution_amount_change_scheduled(
        env,
        proposal_id,
        amount,
        next_cycle,
        effective_round,
    );

    (next_cycle, effective_round)
}

/// Applies the pending change when the cycle boundary is reached. Returns the
/// new amount when a change was applied.
pub(crate) fn apply_pending(env: &Env, cycle_number: u32, new_round: u32) -> Option<i128> {
    let pending = get_pending(env)?;
    if pending.effective_cycle != cycle_number {
        return None;
    }

    env.storage()
        .instance()
        .set(&DataKey::ContributionAmt, &pending.amount);

    // Keep the rebalancing target consistent with the new per-member amount.
    let members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(Vec::new(env));
    if let Some(target) = pending.amount.checked_mul(members.len() as i128) {
        env.storage()
            .instance()
            .set(&DataKey3::BasePoolTarget, &target);
    }

    env.storage()
        .instance()
        .remove(&DataKey5::PendingContributionChange);

    events::emit_contribution_amount_changed(env, pending.amount, cycle_number, new_round);
    Some(pending.amount)
}
