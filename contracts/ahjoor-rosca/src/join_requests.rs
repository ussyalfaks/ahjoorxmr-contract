//! Private groups: when `IsPrivate` is set, an address cannot be added by the
//! admin out of the blue through the normal invite paths alone — it must
//! first submit a join request via `request_to_join`, which the admin then
//! approves (adding the member through the same path as `add_member`) or
//! rejects. Open (non-private) groups don't use this flow; the admin's
//! existing `add_member` keeps working as before regardless of this setting.

use crate::errors::ExtError4;
use crate::{bans, events, DataKey6};
use soroban_sdk::{panic_with_error, Address, Env, Map, Symbol};

pub(crate) fn is_private(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey6::IsPrivate)
        .unwrap_or(false)
}

pub(crate) fn set_private(env: &Env, is_private: bool) {
    env.storage().instance().set(&DataKey6::IsPrivate, &is_private);
    events::emit_group_privacy_set(env, is_private);
}

fn requests(env: &Env) -> Map<Address, u64> {
    env.storage()
        .instance()
        .get(&DataKey6::JoinRequests)
        .unwrap_or(Map::new(env))
}

pub(crate) fn is_pending(env: &Env, requester: &Address) -> bool {
    requests(env).get(requester.clone()).is_some()
}

pub(crate) fn pending_requests(env: &Env) -> Map<Address, u64> {
    requests(env)
}

pub(crate) fn request(env: &Env, requester: &Address) {
    if !is_private(env) {
        panic_with_error!(env, ExtError4::GroupNotPrivate);
    }
    bans::require_not_banned(env, requester);
    if is_pending(env, requester) {
        panic_with_error!(env, ExtError4::JoinRequestAlreadyPending);
    }

    let requested_at = env.ledger().timestamp();
    let mut all = requests(env);
    all.set(requester.clone(), requested_at);
    env.storage().instance().set(&DataKey6::JoinRequests, &all);
    events::emit_join_requested(env, requester.clone(), requested_at);
}

fn remove(env: &Env, requester: &Address) {
    let mut all = requests(env);
    if all.get(requester.clone()).is_none() {
        panic_with_error!(env, ExtError4::JoinRequestNotFound);
    }
    all.remove(requester.clone());
    env.storage().instance().set(&DataKey6::JoinRequests, &all);
}

pub(crate) fn cancel(env: &Env, requester: &Address) {
    remove(env, requester);
    events::emit_join_request_resolved(env, requester.clone(), Symbol::new(env, "cancelled"));
}

/// Removes the pending request, leaving the caller to actually add the
/// member (through the same path as `add_member`).
pub(crate) fn approve(env: &Env, requester: &Address) {
    remove(env, requester);
    events::emit_join_request_resolved(env, requester.clone(), Symbol::new(env, "approved"));
}

pub(crate) fn reject(env: &Env, requester: &Address) {
    remove(env, requester);
    events::emit_join_request_resolved(env, requester.clone(), Symbol::new(env, "rejected"));
}
