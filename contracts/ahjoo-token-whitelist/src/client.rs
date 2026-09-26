use soroban_sdk::{contractclient, Address, BytesN, Env};

use crate::Error;
use crate::TokenQuota;

/// Client interface for the token whitelist contract
/// This allows other contracts to call the whitelist contract
#[contractclient(name = "TokenWhitelistClient")]
pub trait TokenWhitelistInterface {
    /// Check if a token is allowed
    fn is_token_allowed(env: Env, token: Address) -> bool;
    
    // Add a token to the whitelist (admin only)
    fn add_token(env: Env, admin: Address, token: Address);
    
    // Remove a token from the whitelist (admin only)
    fn remove_token(env: Env, admin: Address, token: Address);
    
    /// Get all whitelisted tokens
    fn get_whitelisted_tokens(env: Env) -> soroban_sdk::Vec<Address>;
    
    /// Get the current admin
    fn get_admin(env: Env) -> Address;

    /// Propose new admin
    fn propose_admin(env: Env, current_admin: Address, new_admin: Address);

    /// Accept admin transfer
    fn accept_admin(env: Env, new_admin: Address);

    /// Set token quota (admin only)
    fn set_token_quota(
        env: Env,
        admin: Address,
        token: Address,
        max_volume_per_period: i128,
        period_ledgers: u32,
    );

    /// Update token quota (admin only)
    fn update_token_quota(
        env: Env,
        admin: Address,
        token: Address,
        max_volume_per_period: i128,
        period_ledgers: u32,
    );

    /// Remove token quota (admin only)
    fn remove_token_quota(env: Env, admin: Address, token: Address);

    /// Get token quota
    fn get_token_quota(env: Env, token: Address) -> Option<TokenQuota>;

    /// Record token volume
    fn record_token_volume(env: Env, token: Address, amount: i128) -> Result<(), Error>;

    /// Get token volume
    fn get_token_volume(env: Env, token: Address, from_ledger: u32, to_ledger: u32) -> i128;

    /// Suspend token timed
    fn suspend_token_timed(
        env: Env,
        admin: Address,
        token: Address,
        suspend_duration_ledgers: u32,
        reason_hash: BytesN<32>,
    );

    /// Lift token suspension
    fn lift_token_suspension(env: Env, admin: Address, token: Address);

    /// Extend token suspension
    fn extend_token_suspension(env: Env, admin: Address, token: Address, additional_ledgers: u32);

    /// Get token suspension
    fn get_token_suspension(env: Env, token: Address) -> Option<crate::TokenSuspension>;

    /// Get suspension history
    fn get_suspension_history(
        env: Env,
        token: Address,
        offset: u32,
        limit: u32,
    ) -> soroban_sdk::Vec<crate::SuspensionRecord>;
}
