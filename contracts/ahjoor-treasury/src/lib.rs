#![no_std]
//! Protocol fee treasury.
//!
//! Every Ahjoor contract can point its fee recipient at this contract, so
//! fees from all of them land in one balance instead of accruing separately.
//! The admin configures a distribution (recipients + basis points summing to
//! 10 000) and anyone can call `distribute(token)` to pay out the treasury's
//! full balance of `token` according to it.
//!
//! Rounding: each share is `balance * bps / 10_000` (floored); the dust left
//! over goes to the last recipient so the treasury is fully drained.

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error, token,
    Address, Env, Vec,
};

pub const BPS_DENOMINATOR: u32 = 10_000;
/// Upper bound on recipients so `distribute` has a bounded cost.
pub const MAX_RECIPIENTS: u32 = 20;

const INSTANCE_TTL_THRESHOLD: u32 = 17_280 * 30;
const INSTANCE_TTL_BUMP: u32 = 17_280 * 60;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TreasuryError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    /// Distribution is empty or exceeds `MAX_RECIPIENTS`.
    InvalidRecipientCount = 4,
    /// A share has 0 bps.
    ZeroShare = 5,
    /// Shares do not sum to exactly 10 000 bps.
    InvalidBpsTotal = 6,
    /// The same recipient appears twice.
    DuplicateRecipient = 7,
    /// `distribute` called before a distribution was configured.
    DistributionNotSet = 8,
    /// The treasury holds none of the token.
    NothingToDistribute = 9,
    /// The treasury itself cannot be a recipient.
    TreasuryAsRecipient = 10,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Share {
    pub recipient: Address,
    pub bps: u32,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    PendingAdmin,
    Distribution,
}

#[contractevent(topics = ["distribution_set"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DistributionSet {
    pub admin: Address,
    pub shares: Vec<Share>,
}

#[contractevent(topics = ["fees_distributed"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeesDistributed {
    #[topic]
    pub token: Address,
    pub total: i128,
}

#[contractevent(topics = ["fee_payout"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FeePayout {
    #[topic]
    pub token: Address,
    #[topic]
    pub recipient: Address,
    pub amount: i128,
}

#[contract]
pub struct TreasuryContract;

#[contractimpl]
impl TreasuryContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic_with_error!(&env, TreasuryError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        Self::bump(&env);
    }

    /// Replaces the distribution. Shares must be non-empty, distinct,
    /// non-zero and sum to exactly 10 000 bps.
    pub fn set_distribution(env: Env, admin: Address, shares: Vec<Share>) {
        Self::require_admin(&env, &admin);
        Self::validate_shares(&env, &shares);
        env.storage().instance().set(&DataKey::Distribution, &shares);
        Self::bump(&env);
        DistributionSet { admin, shares }.publish(&env);
    }

    pub fn get_distribution(env: Env) -> Vec<Share> {
        env.storage()
            .instance()
            .get(&DataKey::Distribution)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Permissionless: pays out the treasury's entire balance of `token`
    /// according to the configured distribution. Returns the amount paid out.
    /// No auth needed: funds can only ever move to the admin-configured
    /// recipients.
    pub fn distribute(env: Env, token: Address) -> i128 {
        let shares: Vec<Share> = env
            .storage()
            .instance()
            .get(&DataKey::Distribution)
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::DistributionNotSet));

        let this = env.current_contract_address();
        let client = token::Client::new(&env, &token);
        let total = client.balance(&this);
        if total <= 0 {
            panic_with_error!(&env, TreasuryError::NothingToDistribute);
        }

        let last = shares.len() - 1;
        let mut paid: i128 = 0;
        for (i, share) in shares.iter().enumerate() {
            let amount = if i as u32 == last {
                total - paid
            } else {
                total * share.bps as i128 / BPS_DENOMINATOR as i128
            };
            if amount > 0 {
                client.transfer(&this, &share.recipient, &amount);
                FeePayout { token: token.clone(), recipient: share.recipient.clone(), amount }
                    .publish(&env);
            }
            paid += amount;
        }

        Self::bump(&env);
        FeesDistributed { token, total }.publish(&env);
        total
    }

    /// Preview of what `distribute(token)` would pay each recipient now.
    pub fn preview_distribution(env: Env, token: Address) -> Vec<(Address, i128)> {
        let shares = Self::get_distribution(env.clone());
        let mut out = Vec::new(&env);
        if shares.is_empty() {
            return out;
        }
        let total = token::Client::new(&env, &token).balance(&env.current_contract_address());
        let last = shares.len() - 1;
        let mut paid: i128 = 0;
        for (i, share) in shares.iter().enumerate() {
            let amount = if i as u32 == last {
                total - paid
            } else {
                total * share.bps as i128 / BPS_DENOMINATOR as i128
            };
            paid += amount;
            out.push_back((share.recipient, amount));
        }
        out
    }

    // ── admin rotation (two-step) ───────────────────────────────────────────

    pub fn propose_admin(env: Env, admin: Address, new_admin: Address) {
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::PendingAdmin, &new_admin);
        Self::bump(&env);
    }

    pub fn accept_admin(env: Env, new_admin: Address) {
        new_admin.require_auth();
        let pending: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        if pending != Some(new_admin.clone()) {
            panic_with_error!(&env, TreasuryError::Unauthorized);
        }
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        Self::bump(&env);
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, TreasuryError::NotInitialized))
    }

    // ── internals ────────────────────────────────────────────────────────────

    fn require_admin(env: &Env, caller: &Address) {
        caller.require_auth();
        if *caller != Self::get_admin(env.clone()) {
            panic_with_error!(env, TreasuryError::Unauthorized);
        }
    }

    fn validate_shares(env: &Env, shares: &Vec<Share>) {
        let n = shares.len();
        if n == 0 || n > MAX_RECIPIENTS {
            panic_with_error!(env, TreasuryError::InvalidRecipientCount);
        }
        let this = env.current_contract_address();
        let mut total: u32 = 0;
        for i in 0..n {
            let s = shares.get_unchecked(i);
            if s.bps == 0 {
                panic_with_error!(env, TreasuryError::ZeroShare);
            }
            if s.recipient == this {
                panic_with_error!(env, TreasuryError::TreasuryAsRecipient);
            }
            for j in (i + 1)..n {
                if shares.get_unchecked(j).recipient == s.recipient {
                    panic_with_error!(env, TreasuryError::DuplicateRecipient);
                }
            }
            total = total.saturating_add(s.bps);
        }
        if total != BPS_DENOMINATOR {
            panic_with_error!(env, TreasuryError::InvalidBpsTotal);
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
