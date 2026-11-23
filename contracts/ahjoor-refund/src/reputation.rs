//! Optional reporting of customers' refund abuse scores to the
//! cross-contract reputation registry (`ahjoor-common::reputation`). When a
//! registry is configured, abuse score increments and admin resets are
//! reported so other Ahjoor contracts can see the customer's refund history.
//!
//! Reporting is best-effort: if the registry rejects the call (for example
//! this contract is not allowlisted), the refund operation still succeeds.

use crate::DataKey2;
use soroban_sdk::{contractclient, Address, Env};

/// Normalized points (0..=10_000, higher is better) lost per abuse score
/// point. A denied refund adds 10 abuse points by default, i.e. -1_000.
const SCORE_PER_ABUSE_POINT: u32 = 100;
const MAX_SCORE: u32 = 10_000;

/// Minimal interface of the reputation registry.
#[contractclient(name = "ReputationRegistryClient")]
pub trait ReputationRegistryInterface {
    fn report_signal(env: Env, source: Address, user: Address, raw_score: i128, score: u32);
}

pub(crate) fn registry(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey2::ReputationRegistry)
}

pub(crate) fn set_registry(env: &Env, registry: &Option<Address>) {
    match registry {
        Some(addr) => env
            .storage()
            .instance()
            .set(&DataKey2::ReputationRegistry, addr),
        None => env
            .storage()
            .instance()
            .remove(&DataKey2::ReputationRegistry),
    }
}

/// Maps an abuse score (higher is worse) onto the registry's 0..=10_000
/// scale (higher is better).
pub(crate) fn normalize(abuse_score: u32) -> u32 {
    MAX_SCORE - abuse_score.saturating_mul(SCORE_PER_ABUSE_POINT).min(MAX_SCORE)
}

/// Reports `customer`'s abuse score to the configured registry, if any.
pub(crate) fn report(env: &Env, customer: &Address, abuse_score: u32) {
    if let Some(addr) = registry(env) {
        let client = ReputationRegistryClient::new(env, &addr);
        let _ = client.try_report_signal(
            &env.current_contract_address(),
            customer,
            &(abuse_score as i128),
            &normalize(abuse_score),
        );
    }
}
