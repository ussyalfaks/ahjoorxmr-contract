#![no_std]
#![allow(deprecated, unused_imports, unused_variables, dead_code, unused_mut)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, BytesN, Env, Vec,
};

const INSTANCE_LIFETIME_THRESHOLD: u32 = 100_000;
const INSTANCE_BUMP_AMOUNT: u32 = 120_000;

const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

const SUSPENSION_HISTORY_LIMIT: u32 = 10;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    TokenAlreadyWhitelisted = 4,
    TokenNotWhitelisted = 5,
    QuotaExceeded = 6,
    TokenAlreadyHasQuota = 7,
    TokenHasNoQuota = 8,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenQuota {
    pub max_volume_per_period: i128,
    pub period_ledgers: u32,
}

pub type TokenSuspension = SuspensionRecord;

#[contracttype]
#[derive(Clone)]
pub struct SuspensionRecord {
    pub expiry_ledger: u32,
    pub reason_hash: BytesN<32>,
}

#[contracttype]
#[derive(Clone)]
pub struct SuspensionHistoryEntry {
    pub start_ledger: u32,
    pub expiry_ledger: u32,
    pub reason_hash: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TierLimits {
    pub name: soroban_sdk::String,
    pub max_single_tx_amount: i128,
    pub max_daily_volume: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenMetadata {
    pub decimals: u32,
    pub symbol: soroban_sdk::String,
    pub logo_hash: BytesN<32>,
    pub canonical_oracle: Option<Address>,
}

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    ProposedAdmin,
    WhitelistedTokens,
    SuspensionRecord(Address),
    ContractTokenAllowlist(Address, Address),
    SuspensionHistory(Address),
    TokenQuota(Address),
    TokenVolumeBucket(Address, u32),
    RiskTier(u32),
    TokenTier(Address),
    TokenLimitOverride(Address),
    TokenMetadata(Address),
    ProposalCounter,
    GovernanceToken,
    MinProposalStake,
    VotingWindowLedgers,
    EnactmentDelayLedgers,
    QuorumBps,
    ListingProposal(u32),
    VoteRecord(u32, Address),
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProposalStatus {
    Active = 0,
    PendingEnactment = 1,
    Enacted = 2,
    Failed = 3,
    Vetoed = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ListingProposal {
    pub proposal_id: u32,
    pub token: Address,
    pub proposer: Address,
    pub rationale_hash: BytesN<32>,
    pub voting_deadline_ledger: u32,
    pub approve_weight: i128,
    pub reject_weight: i128,
    pub status: ProposalStatus,
    pub enactment_deadline_ledger: u32,
}

const DEFAULT_VOTING_WINDOW_LEDGERS: u32 = 120_960;
const DEFAULT_ENACTMENT_DELAY_LEDGERS: u32 = 34_560;
const DEFAULT_QUORUM_BPS: u32 = 5_000;

mod events;
mod client;

pub use client::TokenWhitelistClient;

#[contract]
pub struct TokenWhitelistContract;

#[contractimpl]
impl TokenWhitelistContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("Already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        let empty_vec: Vec<Address> = Vec::new(&env);
        env.storage().persistent().set(&DataKey::WhitelistedTokens, &empty_vec);
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_contract_initialized(&env, admin);
    }

    pub fn add_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let mut whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        for existing_token in whitelist.iter() {
            if existing_token == token {
                panic!("Token already whitelisted");
            }
        }
        whitelist.push_back(token.clone());
        env.storage().persistent().set(&DataKey::WhitelistedTokens, &whitelist);
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_token_whitelisted(&env, token, admin);
    }

    pub fn remove_token(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        let mut found = false;
        let mut new_whitelist = Vec::new(&env);
        for existing_token in whitelist.iter() {
            if existing_token == token {
                found = true;
            } else {
                new_whitelist.push_back(existing_token);
            }
        }
        if !found {
            panic!("Token not whitelisted");
        }
        env.storage().persistent().set(&DataKey::WhitelistedTokens, &new_whitelist);
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        if env.storage().persistent().has(&DataKey::SuspensionRecord(token.clone())) {
            env.storage().persistent().remove(&DataKey::SuspensionRecord(token.clone()));
        }
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_token_delisted(&env, token, admin);
    }

    pub fn set_risk_tier(env: Env, admin: Address, tier_id: u32, name: soroban_sdk::String, max_single_tx_amount: i128, max_daily_volume: i128) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let limits = TierLimits { name: name.clone(), max_single_tx_amount, max_daily_volume };
        env.storage().instance().set(&DataKey::RiskTier(tier_id), &limits);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_risk_tier_defined(&env, tier_id, name, max_single_tx_amount);
    }

    pub fn assign_token_tier(env: Env, admin: Address, token: Address, tier_id: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage().persistent().set(&DataKey::TokenTier(token.clone()), &tier_id);
        env.storage().persistent().extend_ttl(&DataKey::TokenTier(token.clone()), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_token_tier_assigned(&env, token, tier_id);
    }

    pub fn set_token_limit_override(env: Env, admin: Address, token: Address, max_single_tx_amount: i128, max_daily_volume: i128) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let limits = TierLimits { name: soroban_sdk::String::from_str(&env, "override"), max_single_tx_amount, max_daily_volume };
        env.storage().persistent().set(&DataKey::TokenLimitOverride(token.clone()), &limits);
        env.storage().persistent().extend_ttl(&DataKey::TokenLimitOverride(token.clone()), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_token_limit_override_set(&env, token);
    }

    pub fn get_token_tier_limits(env: Env, token: Address) -> TierLimits {
        if let Some(override_limits) = env.storage().persistent().get::<_, TierLimits>(&DataKey::TokenLimitOverride(token.clone())) {
            return override_limits;
        }
        if let Some(tier_id) = env.storage().persistent().get::<_, u32>(&DataKey::TokenTier(token.clone())) {
            if let Some(limits) = env.storage().instance().get::<_, TierLimits>(&DataKey::RiskTier(tier_id)) {
                return limits;
            }
        }
        if let Some(limits) = env.storage().instance().get::<_, TierLimits>(&DataKey::RiskTier(1u32)) {
            return limits;
        }
        TierLimits { name: soroban_sdk::String::from_str(&env, "tier-default"), max_single_tx_amount: 0, max_daily_volume: 0 }
    }

    pub fn set_token_metadata(env: Env, admin: Address, token: Address, decimals: u32, symbol: soroban_sdk::String, logo_hash: BytesN<32>, canonical_oracle: Option<Address>) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let md = TokenMetadata { decimals, symbol: symbol.clone(), logo_hash, canonical_oracle };
        env.storage().persistent().set(&DataKey::TokenMetadata(token.clone()), &md);
        env.storage().persistent().extend_ttl(&DataKey::TokenMetadata(token.clone()), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_token_metadata_set(&env, token, symbol, decimals);
    }

    pub fn get_token_metadata(env: Env, token: Address) -> TokenMetadata {
        let whitelist: Vec<Address> = env.storage().persistent().get(&DataKey::WhitelistedTokens).unwrap_or_else(|| Vec::new(&env));
        let mut found = false;
        for t in whitelist.iter() {
            if t == token { found = true; break; }
        }
        if !found { panic!("TokenNotWhitelisted"); }
        env.storage().persistent().get(&DataKey::TokenMetadata(token)).expect("Metadata not set")
    }

    pub fn update_token_decimals(env: Env, admin: Address, token: Address, decimals: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let mut md: TokenMetadata = env.storage().persistent().get(&DataKey::TokenMetadata(token.clone())).expect("Metadata not set");
        md.decimals = decimals;
        env.storage().persistent().set(&DataKey::TokenMetadata(token.clone()), &md);
        env.storage().persistent().extend_ttl(&DataKey::TokenMetadata(token.clone()), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_token_metadata_set(&env, token, md.symbol.clone(), md.decimals);
    }

    pub fn get_all_token_metadata(env: Env, offset: u32, limit: u32) -> Vec<TokenMetadata> {
        let whitelist: Vec<Address> = env.storage().persistent().get(&DataKey::WhitelistedTokens).unwrap_or_else(|| Vec::new(&env));
        let wlen = whitelist.len() as usize;
        let start = offset as usize;
        let mut l = limit.min(50) as usize;
        if start >= wlen { return Vec::new(&env); }
        if start + l > wlen { l = wlen - start; }
        let mut res = Vec::new(&env);
        for i in start..start + l {
            let t = whitelist.get(i as u32).unwrap();
            if let Some(md) = env.storage().persistent().get::<_, TokenMetadata>(&DataKey::TokenMetadata(t.clone())) {
                res.push_back(md);
            }
        }
        res
    }

    /// Check if a token is in the global whitelist (ignores suspension)
    pub fn is_whitelisted(env: Env, token: Address) -> bool {
        let whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        for existing_token in whitelist.iter() {
            if existing_token == token {
                return true;
            }
        }
        false
    }

    /// Check if a token is allowed (whitelist + suspension check)
    pub fn is_token_allowed(env: Env, token: Address) -> bool {
        let whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        let mut in_whitelist = false;
        for existing_token in whitelist.iter() {
            if existing_token == token {
                in_whitelist = true;
                break;
            }
        }
        if !in_whitelist {
            return false;
        }
        let maybe_record: Option<SuspensionRecord> = env
            .storage().persistent()
            .get(&DataKey::SuspensionRecord(token.clone()));
        if let Some(record) = maybe_record {
            let current_ledger = env.ledger().sequence();
            if current_ledger < record.expiry_ledger {
                return false;
            }
            env.storage().persistent().remove(&DataKey::SuspensionRecord(token.clone()));
            events::emit_token_auto_reinstated(&env, token, current_ledger);
        }
        true
    }

    pub fn get_whitelisted_tokens(env: Env) -> Vec<Address> {
        let whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        env.storage().persistent().extend_ttl(
            &DataKey::WhitelistedTokens,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        whitelist
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).expect("Contract not initialized")
    }

    pub fn propose_admin(env: Env, current_admin: Address, new_admin: Address) {
        current_admin.require_auth();
        Self::require_admin(&env, &current_admin);
        env.storage().instance().set(&DataKey::ProposedAdmin, &new_admin);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_admin_transfer_proposed(&env, current_admin, new_admin);
    }

    pub fn accept_admin(env: Env, new_admin: Address) {
        new_admin.require_auth();
        let proposed_admin: Address = env
            .storage().instance()
            .get(&DataKey::ProposedAdmin)
            .expect("No admin transfer proposed");
        if new_admin != proposed_admin {
            panic!("Only proposed admin can accept");
        }
        let old_admin: Address = env
            .storage().instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::ProposedAdmin);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_admin_transferred(&env, old_admin, new_admin);
    }

    pub fn suspend_token_timed(
        env: Env,
        admin: Address,
        token: Address,
        suspend_duration_ledgers: u32,
        reason_hash: BytesN<32>,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        let mut in_whitelist = false;
        for t in whitelist.iter() {
            if t == token { in_whitelist = true; break; }
        }
        if !in_whitelist { panic!("Token not whitelisted"); }
        let current_ledger = env.ledger().sequence();
        let maybe_existing: Option<SuspensionRecord> = env
            .storage().persistent()
            .get(&DataKey::SuspensionRecord(token.clone()));
        if let Some(existing) = maybe_existing {
            if current_ledger < existing.expiry_ledger {
                panic!("Token already suspended");
            }
        }
        let expiry_ledger = current_ledger + suspend_duration_ledgers;
        env.storage().persistent().set(
            &DataKey::SuspensionRecord(token.clone()),
            &SuspensionRecord { expiry_ledger, reason_hash: reason_hash.clone() },
        );
        env.storage().persistent().extend_ttl(
            &DataKey::SuspensionRecord(token.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        Self::add_to_suspension_history(&env, &token, current_ledger, expiry_ledger, reason_hash.clone());
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_token_suspended(&env, token, expiry_ledger, reason_hash);
    }

    pub fn lift_token_suspension(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let maybe_record: Option<SuspensionRecord> = env
            .storage().persistent()
            .get(&DataKey::SuspensionRecord(token.clone()));
        let record = match maybe_record {
            Some(r) => r,
            None => panic!("No active suspension"),
        };
        let current_ledger = env.ledger().sequence();
        if current_ledger >= record.expiry_ledger {
            panic!("No active suspension");
        }
        env.storage().persistent().remove(&DataKey::SuspensionRecord(token.clone()));
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_token_suspension_lifted(&env, token, admin, current_ledger);
    }

    pub fn extend_token_suspension(env: Env, admin: Address, token: Address, additional_ledgers: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let maybe_record: Option<SuspensionRecord> = env
            .storage().persistent()
            .get(&DataKey::SuspensionRecord(token.clone()));
        let record = match maybe_record {
            Some(r) => r,
            None => panic!("No active suspension"),
        };
        let current_ledger = env.ledger().sequence();
        if current_ledger >= record.expiry_ledger {
            panic!("No active suspension");
        }
        env.storage().persistent().set(
            &DataKey::SuspensionRecord(token.clone()),
            &SuspensionRecord {
                expiry_ledger: record.expiry_ledger + additional_ledgers,
                reason_hash: record.reason_hash,
            },
        );
        env.storage().persistent().extend_ttl(
            &DataKey::SuspensionRecord(token.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn get_token_suspension(env: Env, token: Address) -> Option<SuspensionRecord> {
        let maybe: Option<SuspensionRecord> = env
            .storage().persistent()
            .get(&DataKey::SuspensionRecord(token.clone()));
        if let Some(ref record) = maybe {
            if env.ledger().sequence() >= record.expiry_ledger {
                return None;
            }
        }
        maybe
    }

    pub fn get_suspension_history(env: Env, token: Address) -> Vec<SuspensionHistoryEntry> {
        env.storage().persistent()
            .get(&DataKey::SuspensionHistory(token))
            .unwrap_or_else(|| Vec::new(&env))
    }

    fn add_to_suspension_history(
        env: &Env,
        token: &Address,
        start_ledger: u32,
        expiry_ledger: u32,
        reason_hash: BytesN<32>,
    ) {
        let mut history: Vec<SuspensionHistoryEntry> = env
            .storage().persistent()
            .get(&DataKey::SuspensionHistory(token.clone()))
            .unwrap_or_else(|| Vec::new(env));
        history.push_back(SuspensionHistoryEntry { start_ledger, expiry_ledger, reason_hash });
        if history.len() > SUSPENSION_HISTORY_LIMIT {
            let start_idx = history.len() - SUSPENSION_HISTORY_LIMIT;
            let mut trimmed: Vec<SuspensionHistoryEntry> = Vec::new(env);
            for i in start_idx..history.len() {
                trimmed.push_back(history.get(i).unwrap());
            }
            history = trimmed;
        }
        env.storage().persistent().set(&DataKey::SuspensionHistory(token.clone()), &history);
        env.storage().persistent().extend_ttl(
            &DataKey::SuspensionHistory(token.clone()),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }

    pub fn set_contract_token(
        env: Env,
        admin: Address,
        contract_id: Address,
        token: Address,
        expiry_ledger: Option<u32>,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let key = DataKey::ContractTokenAllowlist(contract_id.clone(), token.clone());
        env.storage().persistent().set(&key, &expiry_ledger);
        env.storage().persistent().extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_contract_token_allowlist_updated(&env, contract_id, token, true, expiry_ledger);
    }

    pub fn remove_contract_token(env: Env, admin: Address, contract_id: Address, token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let key = DataKey::ContractTokenAllowlist(contract_id.clone(), token.clone());
        env.storage().persistent().remove(&key);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_contract_token_allowlist_updated(&env, contract_id, token, false, None);
    }

    pub fn get_contract_token_entry(env: Env, contract_id: Address, token: Address) -> Option<Option<u32>> {
        let key = DataKey::ContractTokenAllowlist(contract_id, token);
        env.storage().persistent().get::<_, Option<u32>>(&key)
    }

    pub fn is_token_allowed_for_contract(env: Env, contract_id: Address, token: Address) -> bool {
        let key = DataKey::ContractTokenAllowlist(contract_id, token.clone());
        if let Some(expiry) = env.storage().persistent().get::<_, Option<u32>>(&key) {
            match expiry {
                None => return true,
                Some(exp) => {
                    if env.ledger().sequence() < exp {
                        return true;
                    }
                }
            }
        }
        Self::is_token_allowed(env, token)
    }

    pub fn set_token_quota(
        env: Env,
        admin: Address,
        token: Address,
        max_volume_per_period: i128,
        period_ledgers: u32,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        let mut is_wl = false;
        for existing_token in whitelist.iter() {
            if existing_token == token { is_wl = true; break; }
        }
        if !is_wl { panic!("Token not whitelisted"); }
        if env.storage().persistent().has(&DataKey::TokenQuota(token.clone())) {
            panic!("Token already has quota");
        }
        if max_volume_per_period <= 0 { panic!("max_volume_per_period must be positive"); }
        if period_ledgers == 0 { panic!("period_ledgers must be positive"); }
        let quota = TokenQuota { max_volume_per_period, period_ledgers };
        env.storage().persistent().set(&DataKey::TokenQuota(token.clone()), &quota);
        env.storage().persistent().extend_ttl(&DataKey::TokenQuota(token.clone()), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_token_quota_set(&env, token, max_volume_per_period, period_ledgers);
    }

    pub fn update_token_quota(
        env: Env,
        admin: Address,
        token: Address,
        max_volume_per_period: i128,
        period_ledgers: u32,
    ) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if max_volume_per_period <= 0 { panic!("max_volume_per_period must be positive"); }
        if period_ledgers == 0 { panic!("period_ledgers must be positive"); }
        if !env.storage().persistent().has(&DataKey::TokenQuota(token.clone())) {
            panic!("Token has no quota");
        }
        let quota = TokenQuota { max_volume_per_period, period_ledgers };
        env.storage().persistent().set(&DataKey::TokenQuota(token.clone()), &quota);
        env.storage().persistent().extend_ttl(&DataKey::TokenQuota(token.clone()), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_token_quota_set(&env, token, max_volume_per_period, period_ledgers);
    }

    pub fn remove_token_quota(env: Env, admin: Address, token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if !env.storage().persistent().has(&DataKey::TokenQuota(token.clone())) {
            panic!("Token has no quota");
        }
        env.storage().persistent().remove(&DataKey::TokenQuota(token.clone()));
    }

    pub fn get_token_quota(env: Env, token: Address) -> Option<TokenQuota> {
        env.storage().persistent().get(&DataKey::TokenQuota(token))
    }

    pub fn record_token_volume(env: Env, token: Address, amount: i128) -> Result<(), Error> {
        if amount <= 0 { panic!("amount must be positive"); }
        let Some(quota) = env.storage().persistent().get::<_, TokenQuota>(&DataKey::TokenQuota(token.clone())) else {
            return Ok(());
        };
        let current_ledger = env.ledger().sequence();
        let start_ledger = current_ledger.saturating_sub(quota.period_ledgers - 1);
        let mut current_period_volume: i128 = 0;
        for bucket_ledger in start_ledger..=current_ledger {
            let bucket_volume: i128 = env
                .storage().persistent()
                .get(&DataKey::TokenVolumeBucket(token.clone(), bucket_ledger))
                .unwrap_or(0);
            current_period_volume += bucket_volume;
        }
        if current_period_volume + amount > quota.max_volume_per_period {
            events::emit_token_quota_exceeded(&env, token, amount, current_period_volume);
            return Err(Error::QuotaExceeded);
        }
        let bucket_key = DataKey::TokenVolumeBucket(token.clone(), current_ledger);
        let mut bucket_volume: i128 = env.storage().persistent().get(&bucket_key).unwrap_or(0);
        bucket_volume += amount;
        env.storage().persistent().set(&bucket_key, &bucket_volume);
        env.storage().persistent().extend_ttl(&bucket_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        Ok(())
    }

    pub fn get_token_volume(env: Env, token: Address, from_ledger: u32, to_ledger: u32) -> i128 {
        let mut volume: i128 = 0;
        for bucket_ledger in from_ledger..=to_ledger {
            let bucket_volume: i128 = env
                .storage().persistent()
                .get(&DataKey::TokenVolumeBucket(token.clone(), bucket_ledger))
                .unwrap_or(0);
            volume += bucket_volume;
        }
        volume
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage().instance()
            .get(&DataKey::Admin)
            .expect("Contract not initialized");
        if caller != &admin {
            panic!("Unauthorized: caller is not admin");
        }
    }

    pub fn set_governance_token(env: Env, admin: Address, governance_token: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::GovernanceToken, &governance_token);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn set_min_proposal_stake(env: Env, admin: Address, min_stake: i128) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if min_stake <= 0 { panic!("min_stake must be positive"); }
        env.storage().instance().set(&DataKey::MinProposalStake, &min_stake);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn set_voting_window_ledgers(env: Env, admin: Address, ledgers: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if ledgers == 0 { panic!("ledgers must be positive"); }
        env.storage().instance().set(&DataKey::VotingWindowLedgers, &ledgers);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn set_enactment_delay_ledgers(env: Env, admin: Address, ledgers: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if ledgers == 0 { panic!("ledgers must be positive"); }
        env.storage().instance().set(&DataKey::EnactmentDelayLedgers, &ledgers);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn set_quorum_bps(env: Env, admin: Address, quorum_bps: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if quorum_bps > 10_000 { panic!("quorum_bps cannot exceed 10000"); }
        env.storage().instance().set(&DataKey::QuorumBps, &quorum_bps);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn propose_token_listing(env: Env, proposer: Address, token: Address, rationale_hash: BytesN<32>) -> u32 {
        proposer.require_auth();
        let governance_token: Address = env
            .storage().instance()
            .get(&DataKey::GovernanceToken)
            .expect("GovernanceTokenNotConfigured");
        let min_stake: i128 = env
            .storage().instance()
            .get(&DataKey::MinProposalStake)
            .unwrap_or(1);
        let proposer_balance = token::Client::new(&env, &governance_token).balance(&proposer);
        if proposer_balance < min_stake {
            panic!("InsufficientProposerStake");
        }
        let voting_window: u32 = env
            .storage().instance()
            .get(&DataKey::VotingWindowLedgers)
            .unwrap_or(DEFAULT_VOTING_WINDOW_LEDGERS);
        let proposal_id: u32 = env
            .storage().instance()
            .get(&DataKey::ProposalCounter)
            .unwrap_or(0);
        let proposal = ListingProposal {
            proposal_id,
            token: token.clone(),
            proposer: proposer.clone(),
            rationale_hash,
            voting_deadline_ledger: env.ledger().sequence() + voting_window,
            approve_weight: 0,
            reject_weight: 0,
            status: ProposalStatus::Active,
            enactment_deadline_ledger: 0,
        };
        env.storage().persistent().set(&DataKey::ListingProposal(proposal_id), &proposal);
        env.storage().persistent().extend_ttl(&DataKey::ListingProposal(proposal_id), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        env.storage().instance().set(&DataKey::ProposalCounter, &(proposal_id + 1));
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        events::emit_listing_proposed(&env, proposal_id, token, proposer);
        proposal_id
    }

    pub fn vote_listing(env: Env, voter: Address, proposal_id: u32, approve: bool, weight: i128) {
        voter.require_auth();
        if weight <= 0 { panic!("weight must be positive"); }
        let governance_token: Address = env
            .storage().instance()
            .get(&DataKey::GovernanceToken)
            .expect("GovernanceTokenNotConfigured");
        let voter_balance = token::Client::new(&env, &governance_token).balance(&voter);
        if weight > voter_balance { panic!("VoteWeightExceedsBalance"); }
        let mut proposal: ListingProposal = env
            .storage().persistent()
            .get(&DataKey::ListingProposal(proposal_id))
            .expect("ProposalNotFound");
        if proposal.status != ProposalStatus::Active { panic!("ProposalNotActive"); }
        if env.ledger().sequence() > proposal.voting_deadline_ledger { panic!("VotingWindowClosed"); }
        let vote_key = DataKey::VoteRecord(proposal_id, voter.clone());
        if let Some((prev_approve, prev_weight)) = env
            .storage().persistent()
            .get::<DataKey, (bool, i128)>(&vote_key)
        {
            if prev_approve {
                proposal.approve_weight = proposal.approve_weight.saturating_sub(prev_weight);
            } else {
                proposal.reject_weight = proposal.reject_weight.saturating_sub(prev_weight);
            }
        }
        if approve {
            proposal.approve_weight = proposal.approve_weight.saturating_add(weight);
        } else {
            proposal.reject_weight = proposal.reject_weight.saturating_add(weight);
        }
        env.storage().persistent().set(&vote_key, &(approve, weight));
        env.storage().persistent().extend_ttl(&vote_key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        env.storage().persistent().set(&DataKey::ListingProposal(proposal_id), &proposal);
        env.storage().persistent().extend_ttl(&DataKey::ListingProposal(proposal_id), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_listing_vote_cast(&env, proposal_id, voter, approve, weight);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn finalise_listing_proposal(env: Env, proposal_id: u32) {
        let mut proposal: ListingProposal = env
            .storage().persistent()
            .get(&DataKey::ListingProposal(proposal_id))
            .expect("ProposalNotFound");
        if proposal.status != ProposalStatus::Active { panic!("ProposalNotActive"); }
        if env.ledger().sequence() <= proposal.voting_deadline_ledger { panic!("VotingWindowNotClosed"); }
        let total_weight = proposal.approve_weight + proposal.reject_weight;
        let quorum_bps: u32 = env
            .storage().instance()
            .get(&DataKey::QuorumBps)
            .unwrap_or(DEFAULT_QUORUM_BPS);
        let quorum_met = if total_weight > 0 {
            proposal.approve_weight * 10_000 >= quorum_bps as i128 * total_weight
        } else {
            false
        };
        if quorum_met {
            let enactment_delay: u32 = env
                .storage().instance()
                .get(&DataKey::EnactmentDelayLedgers)
                .unwrap_or(DEFAULT_ENACTMENT_DELAY_LEDGERS);
            proposal.status = ProposalStatus::PendingEnactment;
            proposal.enactment_deadline_ledger = env.ledger().sequence() + enactment_delay;
        } else {
            proposal.status = ProposalStatus::Failed;
        }
        env.storage().persistent().set(&DataKey::ListingProposal(proposal_id), &proposal);
        env.storage().persistent().extend_ttl(&DataKey::ListingProposal(proposal_id), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn enact_listing(env: Env, proposal_id: u32) {
        let mut proposal: ListingProposal = env
            .storage().persistent()
            .get(&DataKey::ListingProposal(proposal_id))
            .expect("ProposalNotFound");
        if proposal.status != ProposalStatus::PendingEnactment { panic!("ProposalNotPendingEnactment"); }
        if env.ledger().sequence() <= proposal.enactment_deadline_ledger { panic!("EnactmentDelayNotElapsed"); }
        let mut whitelist: Vec<Address> = env
            .storage().persistent()
            .get(&DataKey::WhitelistedTokens)
            .unwrap_or_else(|| Vec::new(&env));
        let mut already_listed = false;
        for t in whitelist.iter() {
            if t == proposal.token { already_listed = true; break; }
        }
        if !already_listed {
            whitelist.push_back(proposal.token.clone());
            env.storage().persistent().set(&DataKey::WhitelistedTokens, &whitelist);
            env.storage().persistent().extend_ttl(&DataKey::WhitelistedTokens, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        }
        proposal.status = ProposalStatus::Enacted;
        env.storage().persistent().set(&DataKey::ListingProposal(proposal_id), &proposal);
        env.storage().persistent().extend_ttl(&DataKey::ListingProposal(proposal_id), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_listing_enacted(&env, proposal_id, proposal.token);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn veto_listing_proposal(env: Env, admin: Address, proposal_id: u32, reason_hash: BytesN<32>) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let mut proposal: ListingProposal = env
            .storage().persistent()
            .get(&DataKey::ListingProposal(proposal_id))
            .expect("ProposalNotFound");
        if proposal.status == ProposalStatus::Enacted || proposal.status == ProposalStatus::Vetoed {
            panic!("ProposalAlreadyTerminal");
        }
        proposal.status = ProposalStatus::Vetoed;
        env.storage().persistent().set(&DataKey::ListingProposal(proposal_id), &proposal);
        env.storage().persistent().extend_ttl(&DataKey::ListingProposal(proposal_id), PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        events::emit_listing_vetoed(&env, proposal_id, reason_hash);
        env.storage().instance().extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    pub fn get_listing_proposal(env: Env, proposal_id: u32) -> ListingProposal {
        env.storage().persistent()
            .get(&DataKey::ListingProposal(proposal_id))
            .expect("ProposalNotFound")
    }

    pub fn get_proposal_counter(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::ProposalCounter).unwrap_or(0)
    }
}
