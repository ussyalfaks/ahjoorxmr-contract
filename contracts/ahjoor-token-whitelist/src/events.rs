use soroban_sdk::{contractevent, Address, BytesN, Env, Symbol, Vec};

#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractInitialized {
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenWhitelisted {
    pub token: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenDelisted {
    pub token: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferProposed {
    pub current_admin: Address,
    pub proposed_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AdminTransferred {
    pub old_admin: Address,
    pub new_admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenSuspended {
    pub token: Address,
    pub expiry_ledger: u32,
    pub reason_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenSuspensionLifted {
    pub token: Address,
    pub lifted_by: Address,
    pub ledger: u32,
}

pub fn emit_contract_initialized(e: &Env, admin: Address) {
    ContractInitialized { admin }.publish(e);
}

pub fn emit_token_whitelisted(e: &Env, token: Address, admin: Address) {
    TokenWhitelisted { token, admin }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenMetadataSet {
    pub token: Address,
    pub symbol: soroban_sdk::String,
    pub decimals: u32,
}

pub fn emit_token_metadata_set(e: &Env, token: Address, symbol: soroban_sdk::String, decimals: u32) {
    TokenMetadataSet { token, symbol, decimals }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct RiskTierDefined {
    pub tier_id: u32,
    pub name: soroban_sdk::String,
    pub max_single_tx_amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenTierAssigned {
    pub token: Address,
    pub tier_id: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenLimitOverrideSet {
    pub token: Address,
}

pub fn emit_risk_tier_defined(e: &Env, tier_id: u32, name: soroban_sdk::String, max_single_tx_amount: i128) {
    RiskTierDefined { tier_id, name, max_single_tx_amount }.publish(e);
}

pub fn emit_token_tier_assigned(e: &Env, token: Address, tier_id: u32) {
    TokenTierAssigned { token, tier_id }.publish(e);
}

pub fn emit_token_limit_override_set(e: &Env, token: Address) {
    TokenLimitOverrideSet { token }.publish(e);
}

pub fn emit_token_delisted(e: &Env, token: Address, admin: Address) {
    TokenDelisted { token, admin }.publish(e);
}

pub fn emit_admin_transfer_proposed(e: &Env, current_admin: Address, proposed_admin: Address) {
    AdminTransferProposed { current_admin, proposed_admin }.publish(e);
}

pub fn emit_admin_transferred(e: &Env, old_admin: Address, new_admin: Address) {
    AdminTransferred { old_admin, new_admin }.publish(e);
}

pub fn emit_token_suspended(e: &Env, token: Address, expiry_ledger: u32, reason_hash: BytesN<32>) {
    TokenSuspended { token, expiry_ledger, reason_hash }.publish(e);
}

pub fn emit_token_suspension_lifted(e: &Env, token: Address, lifted_by: Address, ledger: u32) {
    TokenSuspensionLifted { token, lifted_by, ledger }.publish(e);
}

pub fn emit_token_auto_reinstated(e: &Env, token: Address, ledger: u32) {
    e.events().publish(
        (Symbol::new(e, "TokenAutoReinstated"),),
        (token, ledger),
    );
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenQuotaSet {
    pub token: Address,
    pub max_volume_per_period: i128,
    pub period_ledgers: u32,
}

pub fn emit_token_quota_set(e: &Env, token: Address, max_volume_per_period: i128, period_ledgers: u32) {
    TokenQuotaSet { token, max_volume_per_period, period_ledgers }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenQuotaExceeded {
    pub token: Address,
    pub attempted_amount: i128,
    pub period_volume: i128,
}

pub fn emit_token_quota_exceeded(e: &Env, token: Address, attempted_amount: i128, period_volume: i128) {
    TokenQuotaExceeded { token, attempted_amount, period_volume }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractTokenAllowlistUpdated {
    pub contract_id: Address,
    pub token: Address,
    pub action: bool,
    pub expiry: Option<u32>,
}

pub fn emit_contract_token_allowlist_updated(
    e: &Env,
    contract_id: Address,
    token: Address,
    action: bool,
    expiry: Option<u32>,
) {
    ContractTokenAllowlistUpdated { contract_id, token, action, expiry }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ListingProposed {
    pub proposal_id: u32,
    pub token: Address,
    pub proposer: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ListingVoteCast {
    pub proposal_id: u32,
    pub voter: Address,
    pub approve: bool,
    pub weight: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ListingEnacted {
    pub proposal_id: u32,
    pub token: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct ListingVetoed {
    pub proposal_id: u32,
    pub reason_hash: BytesN<32>,
}

pub fn emit_listing_proposed(e: &Env, proposal_id: u32, token: Address, proposer: Address) {
    ListingProposed { proposal_id, token, proposer }.publish(e);
}

pub fn emit_listing_vote_cast(e: &Env, proposal_id: u32, voter: Address, approve: bool, weight: i128) {
    ListingVoteCast { proposal_id, voter, approve, weight }.publish(e);
}

pub fn emit_listing_enacted(e: &Env, proposal_id: u32, token: Address) {
    ListingEnacted { proposal_id, token }.publish(e);
}

pub fn emit_listing_vetoed(e: &Env, proposal_id: u32, reason_hash: BytesN<32>) {
    ListingVetoed { proposal_id, reason_hash }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenDeprecated {
    pub token: Address,
    pub admin: Address,
    pub sunset_ledger: u32,
    pub replacement: Option<Address>,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenUndeprecated {
    pub token: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenSunset {
    pub token: Address,
    pub sunset_ledger: u32,
    pub ledger: u32,
}

pub fn emit_token_deprecated(
    e: &Env,
    token: Address,
    admin: Address,
    sunset_ledger: u32,
    replacement: Option<Address>,
) {
    TokenDeprecated { token, admin, sunset_ledger, replacement }.publish(e);
}

pub fn emit_token_undeprecated(e: &Env, token: Address, admin: Address) {
    TokenUndeprecated { token, admin }.publish(e);
}

pub fn emit_token_sunset(e: &Env, token: Address, sunset_ledger: u32, ledger: u32) {
    TokenSunset { token, sunset_ledger, ledger }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct GuardianSet {
    pub admin: Address,
    pub guardian: Option<Address>,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MaxGuardianSuspensionSet {
    pub ledgers: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenSuspendedByGuardian {
    pub token: Address,
    pub guardian: Address,
    pub expiry_ledger: u32,
}

pub fn emit_guardian_set(e: &Env, admin: Address, guardian: Option<Address>) {
    GuardianSet { admin, guardian }.publish(e);
}

pub fn emit_max_guardian_suspension_set(e: &Env, ledgers: u32) {
    MaxGuardianSuspensionSet { ledgers }.publish(e);
}

pub fn emit_token_suspended_by_guardian(e: &Env, token: Address, guardian: Address, expiry_ledger: u32) {
    TokenSuspendedByGuardian { token, guardian, expiry_ledger }.publish(e);
}

// ==================== TIMELOCKED REMOVAL EVENTS ====================

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenRemovalScheduled {
    pub token: Address,
    pub admin: Address,
    pub removal_ledger: u32,
    pub reason_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenRemovalCancelled {
    pub token: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenRemovedViaTimelock {
    pub token: Address,
    pub requester: Address,
}

pub fn emit_token_removal_scheduled(
    e: &Env,
    token: Address,
    admin: Address,
    removal_ledger: u32,
    reason_hash: BytesN<32>,
) {
    TokenRemovalScheduled { token, admin, removal_ledger, reason_hash }.publish(e);
}

pub fn emit_token_removal_cancelled(e: &Env, token: Address, admin: Address) {
    TokenRemovalCancelled { token, admin }.publish(e);
}

pub fn emit_token_removed_via_timelock(e: &Env, token: Address, requester: Address) {
    TokenRemovedViaTimelock { token, requester }.publish(e);
}

// ==================== TOKEN CATEGORY EVENTS ====================

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenCategoryAssigned {
    pub token: Address,
    pub category: crate::TokenCategory,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenCategoryRemoved {
    pub token: Address,
}

pub fn emit_token_category_assigned(e: &Env, token: Address, category: crate::TokenCategory) {
    TokenCategoryAssigned { token, category }.publish(e);
}

pub fn emit_token_category_removed(e: &Env, token: Address) {
    TokenCategoryRemoved { token }.publish(e);
}

#[allow(dead_code)]
fn _use_symbol(_: Symbol) {}

// ==================== #1182 MULTISIG / #1183 SWAP PAIR EVENTS ====================

#[contractevent]
#[derive(Clone, Debug)]
pub struct MultisigInitialized {
    pub signers: Vec<Address>,
    pub threshold: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MultisigChangeProposed {
    pub proposal_id: u32,
    pub proposer: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MultisigChangeApproved {
    pub proposal_id: u32,
    pub signer: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct MultisigChangeExecuted {
    pub proposal_id: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct SwapPairAllowed {
    pub token_a: Address,
    pub token_b: Address,
    pub max_slippage_bps: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct SwapPairDisallowed {
    pub token_a: Address,
    pub token_b: Address,
}

pub fn emit_multisig_initialized(e: &Env, signers: Vec<Address>, threshold: u32) {
    MultisigInitialized { signers, threshold }.publish(e);
}

pub fn emit_multisig_change_proposed(e: &Env, proposal_id: u32, proposer: Address) {
    MultisigChangeProposed { proposal_id, proposer }.publish(e);
}

pub fn emit_multisig_change_approved(e: &Env, proposal_id: u32, signer: Address) {
    MultisigChangeApproved { proposal_id, signer }.publish(e);
}

pub fn emit_multisig_change_executed(e: &Env, proposal_id: u32) {
    MultisigChangeExecuted { proposal_id }.publish(e);
}

pub fn emit_swap_pair_allowed(e: &Env, token_a: Address, token_b: Address, max_slippage_bps: u32) {
    SwapPairAllowed { token_a, token_b, max_slippage_bps }.publish(e);
}

pub fn emit_swap_pair_disallowed(e: &Env, token_a: Address, token_b: Address) {
    SwapPairDisallowed { token_a, token_b }.publish(e);
}

/// #1185: A SAC token was listed with a verified classic-asset identity.
#[contractevent]
#[derive(Clone, Debug)]
pub struct AssetIdentitySet {
    pub token: Address,
    pub asset_code: soroban_sdk::String,
    pub issuer: Address,
}

pub fn emit_asset_identity_set(e: &Env, token: Address, asset_code: soroban_sdk::String, issuer: Address) {
    AssetIdentitySet { token, asset_code, issuer }.publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CuratorAdded {
    pub curator: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CuratorRemoved {
    pub curator: Address,
    pub admin: Address,
}

pub fn emit_curator_added(e: &Env, curator: Address, admin: Address) {
    CuratorAdded { curator, admin }.publish(e);
}

pub fn emit_curator_removed(e: &Env, curator: Address, admin: Address) {
    CuratorRemoved { curator, admin }.publish(e);
}
