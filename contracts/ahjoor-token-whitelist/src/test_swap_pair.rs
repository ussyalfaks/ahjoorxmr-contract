#![cfg(test)]
//! #1183: Swap pair allowlist.
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env};

use crate::{TokenWhitelistContract, TokenWhitelistContractClient};

struct Setup {
    env: Env,
    client: TokenWhitelistContractClient<'static>,
    admin: Address,
    a: Address,
    b: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let (a, b) = (Address::generate(&env), Address::generate(&env));
    client.add_token(&admin, &a);
    client.add_token(&admin, &b);
    Setup { env, client, admin, a, b }
}

#[test]
fn test_pair_not_allowed_by_default() {
    let s = setup();
    assert!(!s.client.is_swap_pair_allowed(&s.a, &s.b));
    assert_eq!(s.client.get_swap_pair_max_slippage(&s.a, &s.b), None);
}

#[test]
fn test_allow_swap_pair_is_order_independent() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.b, &150);
    assert!(s.client.is_swap_pair_allowed(&s.a, &s.b));
    assert!(s.client.is_swap_pair_allowed(&s.b, &s.a));
    assert_eq!(s.client.get_swap_pair_max_slippage(&s.b, &s.a), Some(150));
}

#[test]
fn test_allow_swap_pair_updates_slippage() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.b, &150);
    s.client.allow_swap_pair(&s.admin, &s.b, &s.a, &300);
    assert_eq!(s.client.get_swap_pair_max_slippage(&s.a, &s.b), Some(300));
}

#[test]
fn test_identical_tokens_never_allowed() {
    let s = setup();
    assert!(!s.client.is_swap_pair_allowed(&s.a, &s.a));
}

#[test]
fn test_disallow_swap_pair() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.b, &150);
    s.client.disallow_swap_pair(&s.admin, &s.b, &s.a);
    assert!(!s.client.is_swap_pair_allowed(&s.a, &s.b));
    assert_eq!(s.client.get_swap_pair_max_slippage(&s.a, &s.b), None);
}

#[test]
fn test_pair_blocked_when_token_delisted() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.b, &150);
    s.client.remove_token(&s.admin, &s.b);
    assert!(!s.client.is_swap_pair_allowed(&s.a, &s.b));
}

#[test]
fn test_pair_blocked_while_token_suspended() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.b, &150);
    s.client.suspend_token_timed(&s.admin, &s.a, &10, &BytesN::from_array(&s.env, &[1u8; 32]));
    assert!(!s.client.is_swap_pair_allowed(&s.a, &s.b));
    s.env.ledger().with_mut(|l| l.sequence_number += 11);
    assert!(s.client.is_swap_pair_allowed(&s.a, &s.b));
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_allow_swap_pair_non_admin_rejected() {
    let s = setup();
    s.client.allow_swap_pair(&Address::generate(&s.env), &s.a, &s.b, &150);
}

#[test]
#[should_panic(expected = "SwapPairTokensIdentical")]
fn test_allow_swap_pair_identical_rejected() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.a, &150);
}

#[test]
#[should_panic(expected = "InvalidMaxSlippageBps")]
fn test_allow_swap_pair_slippage_over_100_percent_rejected() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.b, &10_001);
}

#[test]
#[should_panic(expected = "Token not whitelisted")]
fn test_allow_swap_pair_unlisted_token_rejected() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &Address::generate(&s.env), &150);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_disallow_swap_pair_non_admin_rejected() {
    let s = setup();
    s.client.allow_swap_pair(&s.admin, &s.a, &s.b, &150);
    s.client.disallow_swap_pair(&Address::generate(&s.env), &s.a, &s.b);
}

#[test]
#[should_panic(expected = "SwapPairNotAllowed")]
fn test_disallow_unknown_pair_rejected() {
    let s = setup();
    s.client.disallow_swap_pair(&s.admin, &s.a, &s.b);
}
