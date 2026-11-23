#![cfg(test)]
//! #1185: Classic-asset identity verification for SAC tokens.
use soroban_sdk::{
    testutils::Address as _,
    xdr::{self, Limits, WriteXdr},
    Address, Bytes, Env, String, TryIntoVal, Vec,
};

use crate::{AssetIdentity, TokenWhitelistContract, TokenWhitelistContractClient};

struct Setup {
    env: Env,
    client: TokenWhitelistContractClient<'static>,
    admin: Address,
    /// SAC registered by the test env for asset `aaa` (AlphaNum4).
    sac: Address,
    issuer: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let sac_contract = env.register_stellar_asset_contract_v2(admin.clone());
    let sac = sac_contract.address();
    let issuer = sac_contract.issuer().address();
    Setup { env, client, admin, sac, issuer }
}

fn code(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

/// Classic account address for a fixed ed25519 key, plus its `AccountId`.
fn account(env: &Env, key: u8) -> (Address, xdr::AccountId) {
    let account_id = xdr::AccountId(xdr::PublicKey::PublicKeyTypeEd25519(xdr::Uint256([key; 32])));
    let address: Address = xdr::ScAddress::Account(account_id.clone())
        .try_into_val(env)
        .unwrap();
    (address, account_id)
}

/// SAC address for `asset`, computed from the SDK's own XDR encoding.
fn sac_address(env: &Env, asset: xdr::Asset) -> Address {
    let bytes = asset.to_xdr(Limits::none()).unwrap();
    env.deployer()
        .with_stellar_asset(Bytes::from_slice(env, &bytes))
        .deployed_address()
}

// ── Happy path ───────────────────────────────────────────────────────────────

#[test]
fn test_add_sac_token_alphanum4_stores_identity() {
    let s = setup();
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aaa"), &s.issuer);

    assert!(s.client.is_whitelisted(&s.sac));
    assert_eq!(
        s.client.get_asset_identity(&s.sac),
        Some(AssetIdentity { asset_code: code(&s.env, "aaa"), issuer: s.issuer.clone() })
    );
}

#[test]
fn test_add_sac_token_alphanum12_stores_identity() {
    let s = setup();
    let (issuer, account_id) = account(&s.env, 7);
    let token = sac_address(
        &s.env,
        xdr::Asset::CreditAlphanum12(xdr::AlphaNum12 {
            asset_code: xdr::AssetCode12(*b"LONGCODE\0\0\0\0"),
            issuer: account_id,
        }),
    );

    s.client.add_sac_token(&s.admin, &token, &code(&s.env, "LONGCODE"), &issuer);

    assert!(s.client.is_whitelisted(&token));
    let identity = s.client.get_asset_identity(&token).unwrap();
    assert_eq!(identity.asset_code, code(&s.env, "LONGCODE"));
    assert_eq!(identity.issuer, issuer);
}

#[test]
fn test_add_sac_token_max_length_codes() {
    let s = setup();
    let (issuer, account_id) = account(&s.env, 9);

    let token4 = sac_address(
        &s.env,
        xdr::Asset::CreditAlphanum4(xdr::AlphaNum4 {
            asset_code: xdr::AssetCode4(*b"USDC"),
            issuer: account_id.clone(),
        }),
    );
    let token12 = sac_address(
        &s.env,
        xdr::Asset::CreditAlphanum12(xdr::AlphaNum12 {
            asset_code: xdr::AssetCode12(*b"ABCDEFGHIJ12"),
            issuer: account_id,
        }),
    );

    s.client.add_sac_token(&s.admin, &token4, &code(&s.env, "USDC"), &issuer);
    s.client.add_sac_token(&s.admin, &token12, &code(&s.env, "ABCDEFGHIJ12"), &issuer);

    assert!(s.client.is_whitelisted(&token4));
    assert!(s.client.is_whitelisted(&token12));
}

#[test]
fn test_get_asset_identity_none_for_address_only_listing() {
    let s = setup();
    let token = Address::generate(&s.env);
    s.client.add_token(&s.admin, &token);

    assert_eq!(s.client.get_asset_identity(&token), None);
    assert_eq!(s.client.get_asset_identity(&Address::generate(&s.env)), None);
}

#[test]
fn test_remove_token_clears_asset_identity() {
    let s = setup();
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aaa"), &s.issuer);
    s.client.remove_token(&s.admin, &s.sac);

    assert!(!s.client.is_whitelisted(&s.sac));
    assert_eq!(s.client.get_asset_identity(&s.sac), None);
}

// ── Error branches ───────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #20)")]
fn test_add_sac_token_rejects_wrong_code() {
    let s = setup();
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aab"), &s.issuer);
}

#[test]
#[should_panic(expected = "Error(Contract, #20)")]
fn test_add_sac_token_rejects_wrong_issuer() {
    let s = setup();
    let (other_issuer, _) = account(&s.env, 3);
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aaa"), &other_issuer);
}

#[test]
#[should_panic(expected = "Error(Contract, #20)")]
fn test_add_sac_token_rejects_look_alike_contract() {
    let s = setup();
    let look_alike = Address::generate(&s.env);
    s.client.add_sac_token(&s.admin, &look_alike, &code(&s.env, "aaa"), &s.issuer);
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_add_sac_token_rejects_empty_code() {
    let s = setup();
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, ""), &s.issuer);
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_add_sac_token_rejects_code_longer_than_12() {
    let s = setup();
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "ABCDEFGHIJKLM"), &s.issuer);
}

#[test]
#[should_panic(expected = "Error(Contract, #18)")]
fn test_add_sac_token_rejects_non_alphanumeric_code() {
    let s = setup();
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "US-D"), &s.issuer);
}

#[test]
#[should_panic(expected = "Error(Contract, #19)")]
fn test_add_sac_token_rejects_contract_issuer() {
    let s = setup();
    // `Address::generate` yields a contract (C...) address, not an account.
    let contract_issuer = Address::generate(&s.env);
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aaa"), &contract_issuer);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_add_sac_token_rejects_non_admin() {
    let s = setup();
    let stranger = Address::generate(&s.env);
    s.client.add_sac_token(&stranger, &s.sac, &code(&s.env, "aaa"), &s.issuer);
}

#[test]
#[should_panic(expected = "Token already whitelisted")]
fn test_add_sac_token_rejects_duplicate() {
    let s = setup();
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aaa"), &s.issuer);
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aaa"), &s.issuer);
}

#[test]
#[should_panic(expected = "MultisigRequired")]
fn test_add_sac_token_rejects_when_multisig_enabled() {
    let s = setup();
    let mut signers = Vec::new(&s.env);
    signers.push_back(Address::generate(&s.env));
    signers.push_back(Address::generate(&s.env));
    s.client.init_multisig(&s.admin, &signers, &2);
    s.client.add_sac_token(&s.admin, &s.sac, &code(&s.env, "aaa"), &s.issuer);
}
