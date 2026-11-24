use soroban_sdk::{contractclient, Address, BytesN, Env};

use crate::AssetIdentity;
use crate::ContractTokenEntry;
use crate::Error;
use crate::TokenQuota;

#[contractclient(name = "TokenWhitelistClient")]
pub trait TokenWhitelistInterface {
    fn is_token_allowed(env: Env, token: Address) -> bool;

    fn is_whitelisted(env: Env, token: Address) -> bool;

    /// #1185: Verified classic-asset identity of a SAC token, if it was
    /// listed with `add_sac_token`.
    fn get_asset_identity(env: Env, token: Address) -> Option<AssetIdentity>;

    /// Create-time check: rejects deprecated tokens. Consuming contracts
    /// should use this when opening new positions and keep using
    /// `is_token_allowed` for existing ones.
    fn is_token_allowed_for_new(env: Env, token: Address) -> bool;

    fn deprecate_token(
        env: Env,
        admin: Address,
        token: Address,
        sunset_ledger: u32,
        replacement: Option<Address>,
    );

    /// Token integrators should migrate to from a deprecated `token`, if set.
    fn get_replacement_token(env: Env, token: Address) -> Option<Address>;

    fn undeprecate_token(env: Env, admin: Address, token: Address);

    fn get_token_deprecation(env: Env, token: Address) -> Option<crate::TokenDeprecation>;

    fn is_token_allowed_for_contract(env: Env, contract_id: Address, token: Address) -> bool;

    fn set_contract_token(
        env: Env,
        admin: Address,
        contract_id: Address,
        token: Address,
        expiry_ledger: Option<u32>,
    );

    fn remove_contract_token(env: Env, admin: Address, contract_id: Address, token: Address);

    fn get_contract_token_entry(
        env: Env,
        contract_id: Address,
        token: Address,
    ) -> ContractTokenEntry;

    fn add_token(env: Env, admin: Address, token: Address);

    fn batch_add_tokens(env: Env, admin: Address, tokens: soroban_sdk::Vec<Address>);

    fn remove_token(env: Env, admin: Address, token: Address);

    fn cleanup_allowlist_entries(
        env: Env,
        entries: soroban_sdk::Vec<(Address, Address)>,
    );

    fn get_whitelisted_tokens(env: Env, offset: u32, limit: u32) -> soroban_sdk::Vec<Address>;

    fn get_admin(env: Env) -> Address;

    fn get_proposed_admin(env: Env) -> Option<Address>;

    fn set_token_quota(
        env: Env,
        admin: Address,
        token: Address,
        max_volume_per_period: i128,
        period_ledgers: u32,
    );

    fn update_token_quota(
        env: Env,
        admin: Address,
        token: Address,
        max_volume_per_period: i128,
        period_ledgers: u32,
    );

    fn remove_token_quota(env: Env, admin: Address, token: Address);

    fn get_token_quota(env: Env, token: Address) -> Option<TokenQuota>;

    fn record_token_volume(env: Env, token: Address, amount: i128) -> Result<(), Error>;

    fn get_token_volume(env: Env, token: Address, from_ledger: u32, to_ledger: u32) -> i128;

    fn suspend_token_timed(
        env: Env,
        admin: Address,
        token: Address,
        suspend_duration_ledgers: u32,
        reason_hash: BytesN<32>,
    );

    fn lift_token_suspension(env: Env, admin: Address, token: Address);

    fn extend_token_suspension(env: Env, admin: Address, token: Address, additional_ledgers: u32) -> Result<(), Error>;

    fn get_token_suspension(env: Env, token: Address) -> Option<crate::SuspensionRecord>;

    fn get_suspension_history(env: Env, token: Address) -> soroban_sdk::Vec<crate::SuspensionHistoryEntry>;

    fn get_vote_record(env: Env, proposal_id: u32, voter: Address) -> Option<bool>;

    /// #1183: Whether swaps between `a` and `b` are allowlisted (order-independent).
    fn is_swap_pair_allowed(env: Env, a: Address, b: Address) -> bool;

    /// #1183: Max slippage for an allowlisted pair, `None` if not allowlisted.
    fn get_swap_pair_max_slippage(env: Env, a: Address, b: Address) -> Option<u32>;
}
