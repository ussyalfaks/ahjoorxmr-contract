//! Hardship contribution reductions (#1030).
//!
//! A member in temporary hardship can ask to contribute `reduced_amount`
//! instead of their full contribution for a single round. The request opens a
//! `HardshipReduction` governance proposal; once it is approved the shortfall
//! (full contribution minus `reduced_amount`) is moved out of the emergency
//! reserve into an earmark, so the round pot still adds up.
//!
//! When the round ends the earmark is settled: if the member contributed, the
//! shortfall joins the round pot; otherwise it goes back to the reserve.
//!
//! A member gets at most one pending or approved request per cycle. A request
//! whose proposal was rejected does not count, so the member may ask again.

use crate::charter;
use crate::errors::{Error, ExtError3};
use crate::{
    events, streak, DataKey, DataKey2, DataKey3, DataKey5, HardshipRequest, Proposal,
    ProposalStatus, ProposalType, PERSISTENT_BUMP_AMOUNT, PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{panic_with_error, Address, Env, Map, Vec};

fn current_round(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::CurrentRound)
        .unwrap_or(0)
}

/// The member's full (tier-adjusted) contribution for a round.
fn full_contribution(env: &Env, member: &Address) -> i128 {
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
    let tier_bps = tiers.get(member.clone()).unwrap_or(10_000);
    (base_amount * tier_bps as i128) / 10_000
}

/// Emergency reserve balance usable for hardship cover (0 when disabled).
fn available_reserve(env: &Env) -> i128 {
    let enabled: bool = env
        .storage()
        .instance()
        .get(&DataKey3::ReserveEnabled)
        .unwrap_or(false);
    if !enabled {
        return 0;
    }
    reserve_balance(env)
}

fn reserve_balance(env: &Env) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey3::EmergencyReserveBalance)
        .unwrap_or(0)
}

fn set_reserve_balance(env: &Env, balance: i128) {
    env.storage()
        .persistent()
        .set(&DataKey3::EmergencyReserveBalance, &balance);
    env.storage().persistent().extend_ttl(
        &DataKey3::EmergencyReserveBalance,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

/// Reserve funds drawn for approved reductions whose round has not ended.
/// Must be excluded from round pots.
pub(crate) fn earmarked(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey5::HardshipEarmarked)
        .unwrap_or(0)
}

fn set_earmarked(env: &Env, amount: i128) {
    env.storage()
        .instance()
        .set(&DataKey5::HardshipEarmarked, &amount);
}

fn save_persistent<V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>>(
    env: &Env,
    key: &DataKey5,
    value: &V,
) {
    env.storage().persistent().set(key, value);
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}

/// The contribution `member` owes for `round`: the approved reduced amount
/// when a hardship reduction applies to that round, else `default_required`.
pub(crate) fn required_for(
    env: &Env,
    member: &Address,
    round: u32,
    default_required: i128,
) -> i128 {
    let reduction: Option<HardshipRequest> = env
        .storage()
        .persistent()
        .get(&DataKey5::HardshipReduction(member.clone(), round));
    match reduction {
        Some(r) => r.reduced_amount,
        None => default_required,
    }
}

pub(crate) fn get_request(env: &Env, proposal_id: u32) -> Option<HardshipRequest> {
    env.storage()
        .persistent()
        .get(&DataKey5::HardshipRequest(proposal_id))
}

pub(crate) fn get_reduction(env: &Env, member: &Address, round: u32) -> Option<HardshipRequest> {
    env.storage()
        .persistent()
        .get(&DataKey5::HardshipReduction(member.clone(), round))
}

/// Opens a `HardshipReduction` proposal for `member` and returns its id.
pub(crate) fn request(env: &Env, member: &Address, round: u32, reduced_amount: i128) -> u32 {
    let members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .expect("Not initialized");
    if !members.contains(member) {
        panic_with_error!(env, Error::NotAMember);
    }
    let exited: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::ExitedMembers)
        .unwrap_or(Vec::new(env));
    if exited.contains(member) {
        panic_with_error!(env, Error::MemberHasExited);
    }

    if round < current_round(env) {
        panic_with_error!(env, ExtError3::HardshipRoundPassed);
    }

    let full = full_contribution(env, member);
    if reduced_amount <= 0 || reduced_amount >= full {
        panic_with_error!(env, ExtError3::InvalidHardshipAmount);
    }
    let shortfall = full - reduced_amount;

    let cycle = streak::cycle_of_round(env, round);
    let cycle_key = DataKey5::HardshipCycleRequest(member.clone(), cycle);
    let previous: Option<u32> = env.storage().persistent().get(&cycle_key);
    if let Some(previous_id) = previous {
        let proposals: Map<u32, Proposal> = env
            .storage()
            .instance()
            .get(&DataKey::Proposals)
            .unwrap_or(Map::new(env));
        let still_counts = proposals
            .get(previous_id)
            .map(|p| p.status != ProposalStatus::Rejected)
            .unwrap_or(false);
        if still_counts {
            panic_with_error!(env, ExtError3::HardshipAlreadyUsedThisCycle);
        }
    }

    if available_reserve(env) < shortfall {
        panic_with_error!(env, ExtError3::InsufficientReserveForHardship);
    }

    let proposal_id = charter::open_governance_proposal(
        env,
        member,
        ProposalType::HardshipReduction,
        "Hardship contribution reduction",
    );
    let request = HardshipRequest {
        proposal_id,
        member: member.clone(),
        round,
        cycle,
        reduced_amount,
        shortfall,
        approved: false,
    };
    save_persistent(env, &DataKey5::HardshipRequest(proposal_id), &request);
    save_persistent(env, &cycle_key, &proposal_id);

    events::emit_hardship_reduction_requested(
        env,
        proposal_id,
        member.clone(),
        round,
        reduced_amount,
        shortfall,
    );
    proposal_id
}

/// Called from `execute_proposal` for an approved `HardshipReduction`.
/// Draws the shortfall from the emergency reserve and applies the reduction
/// to the requested round. A proposal with no request attached is a no-op.
pub(crate) fn execute(env: &Env, proposal_id: u32) {
    let mut request = match get_request(env, proposal_id) {
        Some(r) => r,
        None => return,
    };
    if request.approved {
        return;
    }

    let round_now = current_round(env);
    if request.round < round_now {
        panic_with_error!(env, ExtError3::HardshipRoundPassed);
    }
    if request.round == round_now {
        // A member who already paid the reduced amount (or more) this round
        // cannot have the reduction applied retroactively.
        let paid_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::PaidMembers)
            .unwrap_or(Vec::new(env));
        let contributions: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&DataKey::MemberContributions)
            .unwrap_or(Map::new(env));
        let already_paid = contributions.get(request.member.clone()).unwrap_or(0);
        if paid_members.contains(&request.member) || already_paid >= request.reduced_amount {
            panic_with_error!(env, Error::AlreadyContributed);
        }
    }

    if available_reserve(env) < request.shortfall {
        panic_with_error!(env, ExtError3::InsufficientReserveForHardship);
    }
    set_reserve_balance(env, reserve_balance(env) - request.shortfall);
    set_earmarked(env, earmarked(env) + request.shortfall);

    request.approved = true;
    save_persistent(env, &DataKey5::HardshipRequest(proposal_id), &request);
    save_persistent(
        env,
        &DataKey5::HardshipReduction(request.member.clone(), request.round),
        &request,
    );

    let round_key = DataKey5::HardshipRoundMembers(request.round);
    let mut round_members: Vec<Address> = env
        .storage()
        .persistent()
        .get(&round_key)
        .unwrap_or(Vec::new(env));
    round_members.push_back(request.member.clone());
    save_persistent(env, &round_key, &round_members);

    events::emit_hardship_reduction_approved(
        env,
        proposal_id,
        request.member,
        request.round,
        request.reduced_amount,
        request.shortfall,
    );
}

/// Releases the earmarks of `round`'s approved reductions: into the round pot
/// for members who contributed, back to the emergency reserve otherwise.
/// Idempotent — runs once per round from whichever path ends it first.
pub(crate) fn settle_round(env: &Env, round: u32) {
    let round_key = DataKey5::HardshipRoundMembers(round);
    let round_members: Option<Vec<Address>> = env.storage().persistent().get(&round_key);
    let round_members = match round_members {
        Some(m) => m,
        None => return,
    };
    env.storage().persistent().remove(&round_key);

    let paid_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PaidMembers)
        .unwrap_or(Vec::new(env));

    let mut released: i128 = 0;
    let mut returned: i128 = 0;
    for member in round_members.iter() {
        let reduction = match get_reduction(env, &member, round) {
            Some(r) => r,
            None => continue,
        };
        let contributed = paid_members.contains(&member);
        if contributed {
            released += reduction.shortfall;
        } else {
            returned += reduction.shortfall;
        }
        events::emit_hardship_shortfall_settled(env, member, round, reduction.shortfall, contributed);
    }

    set_earmarked(env, earmarked(env) - released - returned);
    if returned > 0 {
        set_reserve_balance(env, reserve_balance(env) + returned);
    }
}
