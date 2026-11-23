//! Optional reporting of members' credit scores to the cross-contract
//! reputation registry (`ahjoor-common::reputation`). When a registry is
//! configured, every credit score change is reported so other Ahjoor
//! contracts can see the member's ROSCA history.
//!
//! Reporting is best-effort: if the registry rejects the call (for example
//! this group is not allowlisted), the ROSCA operation still succeeds.

use crate::DataKey6;
use soroban_sdk::{contractclient, Address, Env};

/// Normalized score (0..=10_000, higher is better) of a member with a credit
/// score of 0.
const NEUTRAL_SCORE: i128 = 5_000;
/// Normalized points per credit score point (an on-time contribution is
/// +10 credit points by default, i.e. +100 normalized).
const SCORE_PER_CREDIT_POINT: i128 = 10;
const MAX_SCORE: i128 = 10_000;

/// Minimal interface of the reputation registry.
#[contractclient(name = "ReputationRegistryClient")]
pub trait ReputationRegistryInterface {
    fn report_signal(env: Env, source: Address, user: Address, raw_score: i128, score: u32);
}

pub(crate) fn registry(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey6::ReputationRegistry)
}

pub(crate) fn set_registry(env: &Env, registry: &Option<Address>) {
    match registry {
        Some(addr) => env
            .storage()
            .instance()
            .set(&DataKey6::ReputationRegistry, addr),
        None => env
            .storage()
            .instance()
            .remove(&DataKey6::ReputationRegistry),
    }
}

/// Maps a ROSCA credit score onto the registry's 0..=10_000 scale.
pub(crate) fn normalize(credit_score: i128) -> u32 {
    NEUTRAL_SCORE
        .saturating_add(credit_score.saturating_mul(SCORE_PER_CREDIT_POINT))
        .clamp(0, MAX_SCORE) as u32
}

/// Reports `member`'s credit score to the configured registry, if any.
pub(crate) fn report(env: &Env, member: &Address, credit_score: i128) {
    if let Some(addr) = registry(env) {
        let client = ReputationRegistryClient::new(env, &addr);
        let _ = client.try_report_signal(
            &env.current_contract_address(),
            member,
            &credit_score,
            &normalize(credit_score),
        );
    }
}
