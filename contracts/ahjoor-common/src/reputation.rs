//! Cross-contract user reputation registry.
//!
//! Each Ahjoor contract judges users on its own signal: ROSCA has a credit
//! score, refunds have an abuse score, payments have a payment history. The
//! registry collects those signals in one place so any contract can see a
//! user's full picture.
//!
//! - The admin allowlists the contracts ("sources") that may report, each
//!   with a weight used for the combined score.
//! - An allowlisted source calls `report_signal` with the user's raw score in
//!   its own units and the same score normalized to `0..=MAX_SCORE`
//!   (higher is better). Only allowlisted sources can write.
//! - Anyone, including the user, can read a user's per-source signals and
//!   combined score with `get_signals` / `get_reputation`.
//!
//! The combined score is the weighted average of the normalized scores of the
//! sources that are currently allowlisted and have reported for the user.

use soroban_sdk::{contractclient, contracterror, contractevent, contracttype, Address, Env, Vec};

#[cfg(feature = "contract")]
use soroban_sdk::{contract, contractimpl, panic_with_error};

/// Upper bound of a normalized score (basis points; higher is better).
pub const MAX_SCORE: u32 = 10_000;
/// Normalized score of a user with no positive or negative history.
pub const NEUTRAL_SCORE: u32 = 5_000;
/// Maximum number of allowlisted sources.
pub const MAX_SOURCES: u32 = 10;

#[cfg(feature = "contract")]
const INSTANCE_LIFETIME_THRESHOLD: u32 = 100_000;
#[cfg(feature = "contract")]
const INSTANCE_BUMP_AMOUNT: u32 = 120_000;
#[cfg(feature = "contract")]
const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
#[cfg(feature = "contract")]
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReputationError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    /// Caller is not the registry admin.
    Unauthorized = 3,
    /// The reporting contract is not on the source allowlist.
    SourceNotAllowed = 4,
    /// Source weight must be greater than zero.
    InvalidWeight = 5,
    /// Normalized score must be within `0..=MAX_SCORE`.
    InvalidScore = 6,
    /// The allowlist already holds `MAX_SOURCES` sources.
    TooManySources = 7,
}

/// Latest signal a source reported for a user.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationSignal {
    pub source: Address,
    /// Score in the source's own units (e.g. ROSCA credit score).
    pub raw_score: i128,
    /// Score normalized to `0..=MAX_SCORE`, higher is better.
    pub score: u32,
    pub updated_ledger: u32,
}

/// A source's contribution to a user's reputation.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceScore {
    pub source: Address,
    pub raw_score: i128,
    pub score: u32,
    pub weight: u32,
    pub updated_ledger: u32,
}

/// Per-source scores and the weighted combined score for a user.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reputation {
    pub user: Address,
    pub sources: Vec<SourceScore>,
    /// Weighted average of `sources[].score`; `None` when no allowlisted
    /// source has reported for the user.
    pub combined_score: Option<u32>,
}

#[contracttype]
#[derive(Clone)]
pub enum ReputationDataKey {
    Admin,
    /// Vec<Address> — allowlisted sources, in the order they were added.
    Sources,
    /// u32 — weight of an allowlisted source.
    SourceWeight(Address),
    /// Vec<Address> — sources that have reported for a user (persistent).
    UserSources(Address),
    /// ReputationSignal — (user, source) → latest signal (persistent).
    Signal(Address, Address),
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ReputationSourceAllowed {
    pub source: Address,
    pub weight: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ReputationSourceRemoved {
    pub source: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ReputationSignalReported {
    pub source: Address,
    pub user: Address,
    pub raw_score: i128,
    pub score: u32,
}

/// Interface used by other contracts to report to, or read from, the
/// registry without depending on the contract implementation.
#[contractclient(name = "ReputationRegistryClient")]
pub trait ReputationRegistryInterface {
    fn report_signal(env: Env, source: Address, user: Address, raw_score: i128, score: u32);

    fn get_reputation(env: Env, user: Address) -> Reputation;

    fn get_signals(env: Env, user: Address) -> Vec<ReputationSignal>;

    fn is_source_allowed(env: Env, source: Address) -> bool;
}

#[cfg(feature = "contract")]
#[contract]
pub struct ReputationRegistryContract;

#[cfg(feature = "contract")]
#[contractimpl]
impl ReputationRegistryContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&ReputationDataKey::Admin) {
            panic_with_error!(&env, ReputationError::AlreadyInitialized);
        }
        env.storage().instance().set(&ReputationDataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&ReputationDataKey::Sources, &Vec::<Address>::new(&env));
        Self::bump_instance(&env);
    }

    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    /// Admin: allowlist `source` (a contract address) with `weight` in the
    /// combined score, or update the weight of an allowlisted source.
    pub fn allow_source(env: Env, admin: Address, source: Address, weight: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if weight == 0 {
            panic_with_error!(&env, ReputationError::InvalidWeight);
        }
        let mut sources = Self::sources(&env);
        if !sources.contains(&source) {
            if sources.len() >= MAX_SOURCES {
                panic_with_error!(&env, ReputationError::TooManySources);
            }
            sources.push_back(source.clone());
            env.storage().instance().set(&ReputationDataKey::Sources, &sources);
        }
        env.storage()
            .instance()
            .set(&ReputationDataKey::SourceWeight(source.clone()), &weight);
        Self::bump_instance(&env);
        ReputationSourceAllowed { source, weight }.publish(&env);
    }

    /// Admin: remove `source` from the allowlist. It can no longer report,
    /// and its stored signals are left out of `get_reputation`.
    pub fn remove_source(env: Env, admin: Address, source: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let sources = Self::sources(&env);
        if !sources.contains(&source) {
            panic_with_error!(&env, ReputationError::SourceNotAllowed);
        }
        let mut kept = Vec::new(&env);
        for s in sources.iter() {
            if s != source {
                kept.push_back(s);
            }
        }
        env.storage().instance().set(&ReputationDataKey::Sources, &kept);
        env.storage()
            .instance()
            .remove(&ReputationDataKey::SourceWeight(source.clone()));
        Self::bump_instance(&env);
        ReputationSourceRemoved { source }.publish(&env);
    }

    pub fn get_sources(env: Env) -> Vec<Address> {
        Self::sources(&env)
    }

    pub fn get_source_weight(env: Env, source: Address) -> Option<u32> {
        env.storage()
            .instance()
            .get(&ReputationDataKey::SourceWeight(source))
    }

    pub fn is_source_allowed(env: Env, source: Address) -> bool {
        env.storage()
            .instance()
            .has(&ReputationDataKey::SourceWeight(source))
    }

    /// Allowlisted source: record `user`'s latest score from that source.
    /// `raw_score` is in the source's own units; `score` is the same value
    /// normalized to `0..=MAX_SCORE` (higher is better). Replaces the
    /// source's previous signal for the user.
    pub fn report_signal(env: Env, source: Address, user: Address, raw_score: i128, score: u32) {
        source.require_auth();
        Self::admin(&env);
        if !Self::is_source_allowed(env.clone(), source.clone()) {
            panic_with_error!(&env, ReputationError::SourceNotAllowed);
        }
        if score > MAX_SCORE {
            panic_with_error!(&env, ReputationError::InvalidScore);
        }

        let signal = ReputationSignal {
            source: source.clone(),
            raw_score,
            score,
            updated_ledger: env.ledger().sequence(),
        };
        let signal_key = ReputationDataKey::Signal(user.clone(), source.clone());
        env.storage().persistent().set(&signal_key, &signal);
        Self::bump_persistent(&env, &signal_key);

        let user_key = ReputationDataKey::UserSources(user.clone());
        let mut user_sources: Vec<Address> = env
            .storage()
            .persistent()
            .get(&user_key)
            .unwrap_or(Vec::new(&env));
        if !user_sources.contains(&source) {
            user_sources.push_back(source.clone());
            env.storage().persistent().set(&user_key, &user_sources);
        }
        Self::bump_persistent(&env, &user_key);
        Self::bump_instance(&env);

        ReputationSignalReported { source, user, raw_score, score }.publish(&env);
    }

    /// Every signal stored for `user`, including ones from sources that have
    /// since been removed from the allowlist.
    pub fn get_signals(env: Env, user: Address) -> Vec<ReputationSignal> {
        let mut out = Vec::new(&env);
        for source in Self::user_sources(&env, &user).iter() {
            if let Some(signal) = env
                .storage()
                .persistent()
                .get::<_, ReputationSignal>(&ReputationDataKey::Signal(user.clone(), source))
            {
                out.push_back(signal);
            }
        }
        out
    }

    /// Per-source scores from currently allowlisted sources and their
    /// weighted combined score.
    pub fn get_reputation(env: Env, user: Address) -> Reputation {
        let mut sources = Vec::new(&env);
        let mut weighted_sum: u64 = 0;
        let mut total_weight: u64 = 0;
        for signal in Self::get_signals(env.clone(), user.clone()).iter() {
            let weight: Option<u32> = env
                .storage()
                .instance()
                .get(&ReputationDataKey::SourceWeight(signal.source.clone()));
            let weight = match weight {
                Some(w) => w,
                None => continue,
            };
            weighted_sum += signal.score as u64 * weight as u64;
            total_weight += weight as u64;
            sources.push_back(SourceScore {
                source: signal.source,
                raw_score: signal.raw_score,
                score: signal.score,
                weight,
                updated_ledger: signal.updated_ledger,
            });
        }
        let combined_score = if total_weight > 0 {
            Some((weighted_sum / total_weight) as u32)
        } else {
            None
        };
        Reputation { user, sources, combined_score }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn admin(env: &Env) -> Address {
        match env.storage().instance().get(&ReputationDataKey::Admin) {
            Some(a) => a,
            None => panic_with_error!(env, ReputationError::NotInitialized),
        }
    }

    fn require_admin(env: &Env, caller: &Address) {
        if *caller != Self::admin(env) {
            panic_with_error!(env, ReputationError::Unauthorized);
        }
    }

    fn sources(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&ReputationDataKey::Sources)
            .unwrap_or(Vec::new(env))
    }

    fn user_sources(env: &Env, user: &Address) -> Vec<Address> {
        env.storage()
            .persistent()
            .get(&ReputationDataKey::UserSources(user.clone()))
            .unwrap_or(Vec::new(env))
    }

    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn bump_persistent(env: &Env, key: &ReputationDataKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }
}

#[cfg(all(test, feature = "contract"))]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    struct Setup {
        env: Env,
        client: ReputationRegistryContractClient<'static>,
        admin: Address,
        rosca: Address,
        refund: Address,
        user: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register(ReputationRegistryContract, ());
        let client = ReputationRegistryContractClient::new(&env, &id);
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let rosca = Address::generate(&env);
        let refund = Address::generate(&env);
        let user = Address::generate(&env);
        Setup { env, client, admin, rosca, refund, user }
    }

    #[test]
    fn test_no_signals_has_no_combined_score() {
        let s = setup();
        let rep = s.client.get_reputation(&s.user);
        assert_eq!(rep.sources.len(), 0);
        assert_eq!(rep.combined_score, None);
        assert_eq!(s.client.get_signals(&s.user).len(), 0);
    }

    #[test]
    fn test_combined_score_is_weighted_average() {
        let s = setup();
        s.client.allow_source(&s.admin, &s.rosca, &3);
        s.client.allow_source(&s.admin, &s.refund, &1);

        s.client.report_signal(&s.rosca, &s.user, &40, &8_000);
        s.client.report_signal(&s.refund, &s.user, &20, &4_000);

        let rep = s.client.get_reputation(&s.user);
        assert_eq!(rep.sources.len(), 2);
        // (8_000 * 3 + 4_000 * 1) / 4 = 7_000
        assert_eq!(rep.combined_score, Some(7_000));
        let rosca = rep.sources.get(0).unwrap();
        assert_eq!(rosca.source, s.rosca);
        assert_eq!(rosca.raw_score, 40);
        assert_eq!(rosca.score, 8_000);
        assert_eq!(rosca.weight, 3);
    }

    #[test]
    fn test_report_replaces_previous_signal() {
        let s = setup();
        s.client.allow_source(&s.admin, &s.rosca, &1);
        s.client.report_signal(&s.rosca, &s.user, &10, &5_100);
        s.client.report_signal(&s.rosca, &s.user, &-20, &4_800);

        let signals = s.client.get_signals(&s.user);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals.get(0).unwrap().raw_score, -20);
        assert_eq!(s.client.get_reputation(&s.user).combined_score, Some(4_800));
    }

    #[test]
    fn test_removed_source_excluded_from_reputation() {
        let s = setup();
        s.client.allow_source(&s.admin, &s.rosca, &1);
        s.client.allow_source(&s.admin, &s.refund, &1);
        s.client.report_signal(&s.rosca, &s.user, &0, &9_000);
        s.client.report_signal(&s.refund, &s.user, &0, &1_000);

        s.client.remove_source(&s.admin, &s.refund);

        assert!(!s.client.is_source_allowed(&s.refund));
        let rep = s.client.get_reputation(&s.user);
        assert_eq!(rep.sources.len(), 1);
        assert_eq!(rep.combined_score, Some(9_000));
        // The stored signal is still visible to the user.
        assert_eq!(s.client.get_signals(&s.user).len(), 2);
    }

    #[test]
    fn test_allow_source_updates_weight() {
        let s = setup();
        s.client.allow_source(&s.admin, &s.rosca, &1);
        s.client.allow_source(&s.admin, &s.rosca, &5);
        assert_eq!(s.client.get_source_weight(&s.rosca), Some(5));
        assert_eq!(s.client.get_sources().len(), 1);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_non_allowlisted_source_cannot_report() {
        let s = setup();
        s.client.report_signal(&s.rosca, &s.user, &0, &5_000);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_removed_source_cannot_report() {
        let s = setup();
        s.client.allow_source(&s.admin, &s.rosca, &1);
        s.client.remove_source(&s.admin, &s.rosca);
        s.client.report_signal(&s.rosca, &s.user, &0, &5_000);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #6)")]
    fn test_score_above_max_rejected() {
        let s = setup();
        s.client.allow_source(&s.admin, &s.rosca, &1);
        s.client.report_signal(&s.rosca, &s.user, &0, &(MAX_SCORE + 1));
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #3)")]
    fn test_non_admin_cannot_allow_source() {
        let s = setup();
        let stranger = Address::generate(&s.env);
        s.client.allow_source(&stranger, &s.rosca, &1);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #5)")]
    fn test_zero_weight_rejected() {
        let s = setup();
        s.client.allow_source(&s.admin, &s.rosca, &0);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #7)")]
    fn test_source_cap_enforced() {
        let s = setup();
        for _ in 0..=MAX_SOURCES {
            s.client.allow_source(&s.admin, &Address::generate(&s.env), &1);
        }
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_initialize_twice_fails() {
        let s = setup();
        s.client.initialize(&s.admin);
    }
}
