//! Optional per-member collateral deposit, slashed on default.
//!
//! A group can require each member to deposit a fixed amount of the base
//! token as collateral, held separately from round contributions. Depositing
//! is opt-in and does not gate contributions or round participation — a
//! member who never deposits simply has nothing to slash.
//!
//! Each time a member defaults on a round, `collateral_slash_bps` of their
//! remaining collateral balance is forfeited to the group's insurance pool
//! (the default is 100%: the entire remaining balance). A member who leaves
//! the group, or whose group has completed its final cycle, can withdraw
//! whatever is left of their balance.

use crate::errors::ExtError4;
use crate::{charter, events, DataKey, DataKey2, DataKey6, GroupStatus};
use soroban_sdk::{panic_with_error, token, Address, Env, Map};

pub(crate) const MAX_SLASH_BPS: u32 = 10_000;
pub(crate) const DEFAULT_SLASH_BPS: u32 = 10_000;

pub(crate) fn requirement(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey6::CollateralAmount)
        .unwrap_or(0)
}

pub(crate) fn slash_bps(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey6::CollateralSlashBps)
        .unwrap_or(DEFAULT_SLASH_BPS)
}

/// Sets the per-member collateral amount and slash share. Only allowed
/// before the group is activated, so every member deposits under the same
/// terms. `amount = 0` disables the feature.
pub(crate) fn set_requirement(env: &Env, amount: i128, bps: u32) {
    if amount < 0 {
        panic_with_error!(env, ExtError4::InvalidCollateralAmount);
    }
    if bps > MAX_SLASH_BPS {
        panic_with_error!(env, ExtError4::InvalidCollateralSlashBps);
    }
    if charter::is_group_activated(env) {
        panic_with_error!(env, ExtError4::CollateralLockedAfterActivation);
    }
    env.storage().instance().set(&DataKey6::CollateralAmount, &amount);
    env.storage().instance().set(&DataKey6::CollateralSlashBps, &bps);
    events::emit_collateral_requirement_set(env, amount, bps);
}

fn balances(env: &Env) -> Map<Address, i128> {
    env.storage()
        .instance()
        .get(&DataKey6::CollateralBalances)
        .unwrap_or(Map::new(env))
}

fn save_balances(env: &Env, map: &Map<Address, i128>) {
    env.storage().instance().set(&DataKey6::CollateralBalances, map);
}

pub(crate) fn balance_of(env: &Env, member: &Address) -> i128 {
    balances(env).get(member.clone()).unwrap_or(0)
}

/// Deposits the full required collateral amount from `member`. Panics if
/// collateral is disabled, the member has already deposited, or the member
/// is not a current group member.
pub(crate) fn deposit(env: &Env, member: &Address) {
    let members: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(soroban_sdk::Vec::new(env));
    if !members.contains(member) {
        panic_with_error!(env, crate::errors::Error::NotAMember);
    }

    let required = requirement(env);
    if required <= 0 {
        panic_with_error!(env, ExtError4::CollateralNotRequired);
    }

    let mut map = balances(env);
    if map.get(member.clone()).unwrap_or(0) > 0 {
        panic_with_error!(env, ExtError4::CollateralAlreadyDeposited);
    }

    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    token::Client::new(env, &token_addr).transfer(
        member,
        &env.current_contract_address(),
        &required,
    );

    map.set(member.clone(), required);
    save_balances(env, &map);
    events::emit_collateral_deposited(env, member.clone(), required, required);
}

/// Slashes `slash_bps` of `member`'s remaining collateral balance into the
/// group's insurance pool after a default. No-op (returns 0) if the member
/// has no balance. Returns the amount slashed.
pub(crate) fn slash_on_default(env: &Env, member: &Address, round: u32) -> i128 {
    let mut map = balances(env);
    let balance = map.get(member.clone()).unwrap_or(0);
    if balance <= 0 {
        return 0;
    }
    let bps = slash_bps(env);
    let amount = balance * bps as i128 / 10_000;
    if amount <= 0 {
        return 0;
    }
    let remaining = balance - amount;
    if remaining == 0 {
        map.remove(member.clone());
    } else {
        map.set(member.clone(), remaining);
    }
    save_balances(env, &map);

    let insurance_pool: i128 = env
        .storage()
        .instance()
        .get(&DataKey2::InsurancePool)
        .unwrap_or(0);
    env.storage()
        .instance()
        .set(&DataKey2::InsurancePool, &(insurance_pool + amount));

    events::emit_collateral_slashed(env, member.clone(), round, amount, remaining);
    amount
}

/// Withdraws `member`'s remaining collateral balance. Only allowed once the
/// member is no longer an active member of the group, or the group has
/// completed its final cycle.
pub(crate) fn withdraw(env: &Env, member: &Address) -> i128 {
    let balance = balance_of(env, member);
    if balance <= 0 {
        panic_with_error!(env, ExtError4::NoCollateralBalance);
    }

    let members: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(soroban_sdk::Vec::new(env));
    let group_status: GroupStatus = env
        .storage()
        .instance()
        .get(&DataKey2::GroupStatus)
        .unwrap_or(GroupStatus::Active);
    let eligible = !members.contains(member) || group_status == GroupStatus::Completed;
    if !eligible {
        panic_with_error!(env, ExtError4::CollateralWithdrawalNotAllowed);
    }

    let mut map = balances(env);
    map.remove(member.clone());
    save_balances(env, &map);

    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    token::Client::new(env, &token_addr).transfer(
        &env.current_contract_address(),
        member,
        &balance,
    );
    events::emit_collateral_withdrawn(env, member.clone(), balance);
    balance
}
