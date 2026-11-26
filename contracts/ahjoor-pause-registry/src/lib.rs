#![no_std]
//! Global pause registry.
//!
//! One switch for the whole protocol: a guardian multisig (M-of-N signer set)
//! calls `pause_all` / `resume_all`, and every Ahjoor contract consults
//! `is_paused` in addition to its own local pause flag. An incident therefore
//! needs one transaction instead of one per contract.
//!
//! Multisig model: the approving guardians are passed in `approvers`; each one
//! must authorize the call and at least `threshold` distinct registered
//! guardians must be present. No proposal state is kept, so a pause lands in a
//! single transaction.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, Address,
    Env, String, Vec,
};

const INSTANCE_TTL_THRESHOLD: u32 = 17_280 * 30;
const INSTANCE_TTL_BUMP: u32 = 17_280 * 60;
/// Upper bound on the guardian set so approval checks stay cheap.
pub const MAX_GUARDIANS: u32 = 20;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PauseRegistryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    /// Threshold is 0 or larger than the guardian set.
    InvalidThreshold = 3,
    /// Guardian set is empty, too large, or contains duplicates.
    InvalidGuardians = 4,
    /// An approver is not a registered guardian.
    NotGuardian = 5,
    /// The same approver was listed twice.
    DuplicateApprover = 6,
    /// Fewer distinct guardian approvals than the threshold.
    ThresholdNotMet = 7,
    AlreadyPaused = 8,
    NotPaused = 9,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Guardians,
    Threshold,
    Paused,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardianConfig {
    pub guardians: Vec<Address>,
    pub threshold: u32,
}

#[contractevent(topics = ["pause_all"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalPaused {
    pub approvers: Vec<Address>,
    pub reason: String,
}

#[contractevent(topics = ["resume_all"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalResumed {
    pub approvers: Vec<Address>,
}

#[contractevent(topics = ["guardians_set"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardiansUpdated {
    pub guardians: Vec<Address>,
    pub threshold: u32,
}

#[contract]
pub struct PauseRegistryContract;

#[contractimpl]
impl PauseRegistryContract {
    /// One-time setup of the guardian multisig.
    pub fn initialize(env: Env, guardians: Vec<Address>, threshold: u32) {
        if env.storage().instance().has(&DataKey::Threshold) {
            panic_with_error!(&env, PauseRegistryError::AlreadyInitialized);
        }
        Self::store_guardians(&env, &guardians, threshold);
        env.storage().instance().set(&DataKey::Paused, &false);
        Self::bump(&env);
    }

    /// Pauses every contract that consults this registry.
    pub fn pause_all(env: Env, approvers: Vec<Address>, reason: String) {
        Self::require_quorum(&env, &approvers);
        if Self::is_paused(env.clone()) {
            panic_with_error!(&env, PauseRegistryError::AlreadyPaused);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        Self::bump(&env);
        GlobalPaused { approvers, reason }.publish(&env);
    }

    /// Lifts the global pause. Contracts with their own local pause flag set
    /// stay paused until that flag is cleared too.
    pub fn resume_all(env: Env, approvers: Vec<Address>) {
        Self::require_quorum(&env, &approvers);
        if !Self::is_paused(env.clone()) {
            panic_with_error!(&env, PauseRegistryError::NotPaused);
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        Self::bump(&env);
        GlobalResumed { approvers }.publish(&env);
    }

    /// Rotates the guardian set; requires a quorum of the *current* set.
    pub fn set_guardians(env: Env, approvers: Vec<Address>, guardians: Vec<Address>, threshold: u32) {
        Self::require_quorum(&env, &approvers);
        Self::store_guardians(&env, &guardians, threshold);
        Self::bump(&env);
        GuardiansUpdated { guardians, threshold }.publish(&env);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
    }

    pub fn get_guardian_config(env: Env) -> GuardianConfig {
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&DataKey::Threshold)
            .unwrap_or_else(|| panic_with_error!(&env, PauseRegistryError::NotInitialized));
        let guardians: Vec<Address> = env.storage().instance().get(&DataKey::Guardians).unwrap();
        GuardianConfig { guardians, threshold }
    }

    // ── internals ────────────────────────────────────────────────────────────

    fn store_guardians(env: &Env, guardians: &Vec<Address>, threshold: u32) {
        let n = guardians.len();
        if n == 0 || n > MAX_GUARDIANS {
            panic_with_error!(env, PauseRegistryError::InvalidGuardians);
        }
        for i in 0..n {
            let g = guardians.get_unchecked(i);
            for j in (i + 1)..n {
                if guardians.get_unchecked(j) == g {
                    panic_with_error!(env, PauseRegistryError::InvalidGuardians);
                }
            }
        }
        if threshold == 0 || threshold > n {
            panic_with_error!(env, PauseRegistryError::InvalidThreshold);
        }
        env.storage().instance().set(&DataKey::Guardians, guardians);
        env.storage().instance().set(&DataKey::Threshold, &threshold);
    }

    /// Every approver must be a distinct registered guardian that authorized
    /// this call, and there must be at least `threshold` of them.
    fn require_quorum(env: &Env, approvers: &Vec<Address>) {
        let cfg = Self::get_guardian_config(env.clone());
        let n = approvers.len();
        for i in 0..n {
            let a = approvers.get_unchecked(i);
            if !cfg.guardians.contains(&a) {
                panic_with_error!(env, PauseRegistryError::NotGuardian);
            }
            for j in (i + 1)..n {
                if approvers.get_unchecked(j) == a {
                    panic_with_error!(env, PauseRegistryError::DuplicateApprover);
                }
            }
        }
        if n < cfg.threshold {
            panic_with_error!(env, PauseRegistryError::ThresholdNotMet);
        }
        for a in approvers.iter() {
            a.require_auth();
        }
    }

    fn bump(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_BUMP);
    }
}

#[cfg(test)]
mod test;
