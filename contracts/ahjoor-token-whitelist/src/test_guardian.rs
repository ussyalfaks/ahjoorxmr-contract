#![cfg(test)]
use soroban_sdk::{testutils::{Address as _, Ledger}, Address, BytesN, Env};

use crate::{SuspensionActor, TokenWhitelistContract, TokenWhitelistContractClient};

const CAP: u32 = 500;

struct Setup {
    env: Env,
    client: TokenWhitelistContractClient<'static>,
    admin: Address,
    guardian: Address,
    token: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let guardian = Address::generate(&env);
    client.set_guardian(&admin, &Some(guardian.clone()));
    client.set_guardian_suspension_cap(&admin, &CAP);
    let token = Address::generate(&env);
    client.add_token(&admin, &token);
    Setup { env, client, admin, guardian, token }
}

fn reason(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[7u8; 32])
}

#[test]
fn test_guardian_config_views() {
    let s = setup();
    assert_eq!(s.client.get_guardian(), Some(s.guardian.clone()));
    assert_eq!(s.client.get_guardian_suspension_cap(), CAP);
}

#[test]
fn test_guardian_can_suspend() {
    let s = setup();
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    assert!(!s.client.is_token_allowed(&s.token));
}

#[test]
fn test_guardian_suspension_duration_is_capped() {
    let s = setup();
    let now = s.env.ledger().sequence();
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    assert_eq!(s.client.get_token_suspension(&s.token).unwrap().expiry_ledger, now + CAP);

    s.env.ledger().with_mut(|l| l.sequence_number += CAP - 1);
    assert!(!s.client.is_token_allowed(&s.token));
    s.env.ledger().with_mut(|l| l.sequence_number += 1);
    assert!(s.client.is_token_allowed(&s.token));
}

#[test]
#[should_panic(expected = "Guardian suspension cap not set")]
fn test_guardian_cannot_suspend_without_cap() {
    let s = setup();
    s.client.set_guardian_suspension_cap(&s.admin, &0);
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
}

#[test]
#[should_panic(expected = "Token already suspended")]
fn test_guardian_cannot_extend_by_resuspending() {
    let s = setup();
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
}

#[test]
fn test_suspension_history_marks_actor() {
    let s = setup();
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    s.client.lift_token_suspension(&s.admin, &s.token);
    s.client.suspend_token_timed(&s.admin, &s.token, &10u32, &reason(&s.env));

    let history = s.client.get_suspension_history(&s.token);
    assert_eq!(history.len(), 2);
    let by_guardian = history.get(0).unwrap();
    assert_eq!(by_guardian.actor, SuspensionActor::Guardian);
    assert_eq!(by_guardian.suspended_by, s.guardian);
    let by_admin = history.get(1).unwrap();
    assert_eq!(by_admin.actor, SuspensionActor::Admin);
    assert_eq!(by_admin.suspended_by, s.admin);
}

#[test]
fn test_admin_can_lift_or_extend_guardian_suspension() {
    let s = setup();
    let now = s.env.ledger().sequence();
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    s.client.extend_token_suspension(&s.admin, &s.token, &100u32);
    assert_eq!(
        s.client.get_token_suspension(&s.token).unwrap().expiry_ledger,
        now + CAP + 100
    );
    s.client.lift_token_suspension(&s.admin, &s.token);
    assert!(s.client.is_token_allowed(&s.token));
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not guardian")]
fn test_non_guardian_cannot_suspend() {
    let s = setup();
    let outsider = Address::generate(&s.env);
    s.client.guardian_suspend_token(&outsider, &s.token, &reason(&s.env));
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not guardian")]
fn test_admin_is_not_implicitly_guardian() {
    let s = setup();
    s.client.guardian_suspend_token(&s.admin, &s.token, &reason(&s.env));
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_guardian_cannot_lift() {
    let s = setup();
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    s.client.lift_token_suspension(&s.guardian, &s.token);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_guardian_cannot_extend() {
    let s = setup();
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    s.client.extend_token_suspension(&s.guardian, &s.token, &100u32);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_guardian_cannot_suspend_with_custom_duration() {
    let s = setup();
    s.client.suspend_token_timed(&s.guardian, &s.token, &1_000_000u32, &reason(&s.env));
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_guardian_cannot_list() {
    let s = setup();
    let token = Address::generate(&s.env);
    s.client.add_token(&s.guardian, &token);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_guardian_cannot_remove() {
    let s = setup();
    s.client.remove_token(&s.guardian, &s.token);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_guardian_cannot_change_guardian_config() {
    let s = setup();
    s.client.set_guardian_suspension_cap(&s.guardian, &1_000_000u32);
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_guardian_cannot_appoint_guardian() {
    let s = setup();
    let other = Address::generate(&s.env);
    s.client.set_guardian(&s.guardian, &Some(other));
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not guardian")]
fn test_removed_guardian_cannot_suspend() {
    let s = setup();
    s.client.set_guardian(&s.admin, &None);
    assert_eq!(s.client.get_guardian(), None);
    s.client.guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
}

#[test]
fn test_rotated_guardian_replaces_old_one() {
    let s = setup();
    let new_guardian = Address::generate(&s.env);
    s.client.set_guardian(&s.admin, &Some(new_guardian.clone()));

    let res = s.client.try_guardian_suspend_token(&s.guardian, &s.token, &reason(&s.env));
    assert!(res.is_err());
    s.client.guardian_suspend_token(&new_guardian, &s.token, &reason(&s.env));
    assert!(!s.client.is_token_allowed(&s.token));
}
