//! Client-side view of the global `ahjoor-pause-registry` contract.
//!
//! Each Ahjoor contract stores an optional registry address and calls
//! [`is_globally_paused`] from its local `require_not_paused` guard, so a
//! single `pause_all` on the registry halts state changes everywhere.

use soroban_sdk::{contractclient, Address, Env};

#[contractclient(name = "PauseRegistryClient")]
pub trait PauseRegistryInterface {
    /// `true` while the guardian multisig has the whole protocol paused.
    fn is_paused(env: Env) -> bool;
}

/// Returns `true` when `registry` is configured and reports a global pause.
/// An unset registry never pauses anything.
pub fn is_globally_paused(env: &Env, registry: Option<Address>) -> bool {
    match registry {
        Some(addr) => PauseRegistryClient::new(env, &addr).is_paused(),
        None => false,
    }
}
