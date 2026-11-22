//! Keeper bounty: a flat reward, funded by anyone into a pool, paid out of
//! the pool to whoever calls `keeper_close_round` / `keeper_finalize_round`
//! after the round deadline has passed. This lets upkeep run permissionlessly
//! instead of depending on the admin to remember to trigger it every round.
//! The original admin-only `close_round` / `finalize_round` are unaffected
//! and never pay a bounty.

use crate::errors::ExtError4;
use crate::{events, DataKey, DataKey6};
use soroban_sdk::{panic_with_error, token, Address, Env};

pub(crate) fn get_bounty_amount(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey6::KeeperBountyAmount)
        .unwrap_or(0)
}

pub(crate) fn set_bounty_amount(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, ExtError4::InvalidBountyAmount);
    }
    env.storage()
        .instance()
        .set(&DataKey6::KeeperBountyAmount, &amount);
    events::emit_keeper_bounty_amount_set(env, amount);
}

pub(crate) fn get_pool(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey6::KeeperBountyPool)
        .unwrap_or(0)
}

/// Anyone can top up the bounty pool so keepers have something to be paid.
pub(crate) fn fund(env: &Env, funder: &Address, amount: i128) {
    if amount <= 0 {
        panic_with_error!(env, ExtError4::InvalidBountyAmount);
    }
    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let client = token::Client::new(env, &token_addr);
    client.transfer(funder, &env.current_contract_address(), &amount);

    let pool_total = get_pool(env) + amount;
    env.storage()
        .instance()
        .set(&DataKey6::KeeperBountyPool, &pool_total);
    events::emit_keeper_bounty_funded(env, funder.clone(), amount, pool_total);
}

/// Pays `caller` whatever the pool can cover of the configured bounty amount
/// (possibly 0), decrementing the pool by the same amount. Returns the
/// amount actually paid.
pub(crate) fn pay(env: &Env, caller: &Address, round: u32) -> i128 {
    let bounty_amount = get_bounty_amount(env);
    let pool = get_pool(env);
    let payable = if bounty_amount < pool { bounty_amount } else { pool };
    if payable <= 0 {
        return 0;
    }

    let token_addr: Address = env.storage().instance().get(&DataKey::Token).unwrap();
    let client = token::Client::new(env, &token_addr);
    client.transfer(&env.current_contract_address(), caller, &payable);

    env.storage()
        .instance()
        .set(&DataKey6::KeeperBountyPool, &(pool - payable));
    events::emit_keeper_bounty_paid(env, caller.clone(), round, payable);
    payable
}
