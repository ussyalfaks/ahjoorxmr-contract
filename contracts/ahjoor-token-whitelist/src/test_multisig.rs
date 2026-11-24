#![cfg(test)]
//! #1182: M-of-N multisig admin for whitelist changes.
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

use crate::{MultisigAction, TokenWhitelistContract, TokenWhitelistContractClient};

struct Setup {
    env: Env,
    client: TokenWhitelistContractClient<'static>,
    admin: Address,
    guardian: Address,
    s1: Address,
    s2: Address,
    s3: Address,
}

fn base() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(TokenWhitelistContract, ());
    let client = TokenWhitelistContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    client.initialize(&admin);
    let guardian = Address::generate(&env);
    client.set_guardian(&admin, &Some(guardian.clone()));
    client.set_guardian_suspension_cap(&admin, &100);
    let (s1, s2, s3) = (Address::generate(&env), Address::generate(&env), Address::generate(&env));
    Setup { env, client, admin, guardian, s1, s2, s3 }
}

fn signers(s: &Setup) -> Vec<Address> {
    let mut v = Vec::new(&s.env);
    v.push_back(s.s1.clone());
    v.push_back(s.s2.clone());
    v.push_back(s.s3.clone());
    v
}

/// 2-of-3 multisig enabled.
fn setup() -> Setup {
    let s = base();
    s.client.init_multisig(&s.admin, &signers(&s), &2);
    s
}

// ---------- init_multisig ----------

#[test]
fn test_init_multisig_stores_config() {
    let s = setup();
    let cfg = s.client.get_multisig_config().unwrap();
    assert_eq!(cfg.threshold, 2);
    assert_eq!(cfg.signers, signers(&s));
}

#[test]
fn test_multisig_config_none_before_init() {
    let s = base();
    assert!(s.client.get_multisig_config().is_none());
}

#[test]
#[should_panic(expected = "Unauthorized: caller is not admin")]
fn test_init_multisig_non_admin_rejected() {
    let s = base();
    s.client.init_multisig(&s.s1, &signers(&s), &2);
}

#[test]
#[should_panic(expected = "MultisigAlreadyInitialized")]
fn test_init_multisig_twice_rejected() {
    let s = setup();
    s.client.init_multisig(&s.admin, &signers(&s), &2);
}

#[test]
#[should_panic(expected = "MultisigSignersEmpty")]
fn test_init_multisig_empty_signers_rejected() {
    let s = base();
    s.client.init_multisig(&s.admin, &Vec::new(&s.env), &1);
}

#[test]
#[should_panic(expected = "TooManyMultisigSigners")]
fn test_init_multisig_too_many_signers_rejected() {
    let s = base();
    let mut v = Vec::new(&s.env);
    for _ in 0..21 {
        v.push_back(Address::generate(&s.env));
    }
    s.client.init_multisig(&s.admin, &v, &2);
}

#[test]
#[should_panic(expected = "InvalidMultisigThreshold")]
fn test_init_multisig_zero_threshold_rejected() {
    let s = base();
    s.client.init_multisig(&s.admin, &signers(&s), &0);
}

#[test]
#[should_panic(expected = "InvalidMultisigThreshold")]
fn test_init_multisig_threshold_above_signers_rejected() {
    let s = base();
    s.client.init_multisig(&s.admin, &signers(&s), &4);
}

#[test]
#[should_panic(expected = "DuplicateMultisigSigner")]
fn test_init_multisig_duplicate_signer_rejected() {
    let s = base();
    let mut v = signers(&s);
    v.push_back(s.s1.clone());
    s.client.init_multisig(&s.admin, &v, &2);
}

#[test]
#[should_panic(expected = "GuardianCannotBeMultisigSigner")]
fn test_init_multisig_guardian_as_signer_rejected() {
    let s = base();
    let mut v = signers(&s);
    v.push_back(s.guardian.clone());
    s.client.init_multisig(&s.admin, &v, &2);
}

// ---------- single-admin paths disabled ----------

#[test]
#[should_panic(expected = "MultisigRequired")]
fn test_admin_add_token_blocked_after_multisig() {
    let s = setup();
    s.client.add_token(&s.admin, &Address::generate(&s.env));
}

#[test]
#[should_panic(expected = "MultisigRequired")]
fn test_admin_batch_add_blocked_after_multisig() {
    let s = setup();
    let mut v = Vec::new(&s.env);
    v.push_back(Address::generate(&s.env));
    s.client.batch_add_tokens(&s.admin, &v);
}

#[test]
#[should_panic(expected = "MultisigRequired")]
fn test_admin_remove_token_blocked_after_multisig() {
    let s = base();
    let token = Address::generate(&s.env);
    s.client.add_token(&s.admin, &token);
    s.client.init_multisig(&s.admin, &signers(&s), &2);
    s.client.remove_token(&s.admin, &token);
}

#[test]
#[should_panic(expected = "MultisigRequired")]
fn test_admin_schedule_removal_blocked_after_multisig() {
    let s = base();
    let token = Address::generate(&s.env);
    s.client.add_token(&s.admin, &token);
    s.client.init_multisig(&s.admin, &signers(&s), &2);
    s.client.schedule_token_removal(&s.admin, &token, &10, &BytesN::from_array(&s.env, &[1u8; 32]));
}

#[test]
#[should_panic(expected = "MultisigRequired")]
fn test_admin_assign_tier_blocked_after_multisig() {
    let s = setup();
    s.client.set_risk_tier(&s.admin, &1, &String::from_str(&s.env, "t1"), &100, &1000);
    s.client.assign_token_tier(&s.admin, &Address::generate(&s.env), &1);
}

// ---------- proposals ----------

#[test]
fn test_add_token_requires_threshold_approvals() {
    let s = setup();
    let token = Address::generate(&s.env);
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(token.clone()));
    // 1 of 2: not yet executed.
    assert!(!s.client.is_whitelisted(&token));
    let p = s.client.get_multisig_proposal(&id).unwrap();
    assert!(!p.executed);
    assert_eq!(p.approvals.len(), 1);
    assert_eq!(p.proposer, s.s1);

    s.client.approve_whitelist_change(&s.s2, &id);
    assert!(s.client.is_whitelisted(&token));
    assert!(s.client.get_multisig_proposal(&id).unwrap().executed);
}

#[test]
fn test_remove_token_via_multisig() {
    let s = base();
    let token = Address::generate(&s.env);
    s.client.add_token(&s.admin, &token);
    s.client.init_multisig(&s.admin, &signers(&s), &2);
    let id = s.client.propose_whitelist_change(&s.s2, &MultisigAction::RemoveToken(token.clone()));
    assert!(s.client.is_whitelisted(&token));
    s.client.approve_whitelist_change(&s.s3, &id);
    assert!(!s.client.is_whitelisted(&token));
}

#[test]
fn test_assign_tier_via_multisig() {
    let s = setup();
    s.client.set_risk_tier(&s.admin, &7, &String::from_str(&s.env, "t7"), &100, &1000);
    let token = Address::generate(&s.env);
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AssignTier(token.clone(), 7));
    assert_eq!(s.client.get_token_tier(&token), None);
    s.client.approve_whitelist_change(&s.s3, &id);
    assert_eq!(s.client.get_token_tier(&token), Some(7));
}

#[test]
fn test_threshold_one_executes_on_propose() {
    let s = base();
    s.client.init_multisig(&s.admin, &signers(&s), &1);
    let token = Address::generate(&s.env);
    let id = s.client.propose_whitelist_change(&s.s3, &MultisigAction::AddToken(token.clone()));
    assert!(s.client.is_whitelisted(&token));
    assert!(s.client.get_multisig_proposal(&id).unwrap().executed);
}

#[test]
fn test_proposal_ids_increment() {
    let s = setup();
    let a = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(Address::generate(&s.env)));
    let b = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(Address::generate(&s.env)));
    assert_eq!(b, a + 1);
    assert!(s.client.get_multisig_proposal(&99).is_none());
}

#[test]
#[should_panic(expected = "MultisigNotInitialized")]
fn test_propose_without_multisig_rejected() {
    let s = base();
    s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(Address::generate(&s.env)));
}

#[test]
#[should_panic(expected = "NotMultisigSigner")]
fn test_propose_by_non_signer_rejected() {
    let s = setup();
    s.client.propose_whitelist_change(&s.admin, &MultisigAction::AddToken(Address::generate(&s.env)));
}

#[test]
#[should_panic(expected = "NotMultisigSigner")]
fn test_approve_by_non_signer_rejected() {
    let s = setup();
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(Address::generate(&s.env)));
    s.client.approve_whitelist_change(&Address::generate(&s.env), &id);
}

#[test]
#[should_panic(expected = "MultisigProposalNotFound")]
fn test_approve_missing_proposal_rejected() {
    let s = setup();
    s.client.approve_whitelist_change(&s.s1, &42);
}

#[test]
#[should_panic(expected = "MultisigAlreadyApproved")]
fn test_double_approval_rejected() {
    let s = setup();
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(Address::generate(&s.env)));
    s.client.approve_whitelist_change(&s.s1, &id);
}

#[test]
#[should_panic(expected = "MultisigProposalAlreadyExecuted")]
fn test_approve_executed_proposal_rejected() {
    let s = setup();
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(Address::generate(&s.env)));
    s.client.approve_whitelist_change(&s.s2, &id);
    s.client.approve_whitelist_change(&s.s3, &id);
}

#[test]
#[should_panic(expected = "Token already whitelisted")]
fn test_executing_invalid_add_reverts() {
    let s = setup();
    let token = Address::generate(&s.env);
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(token.clone()));
    s.client.approve_whitelist_change(&s.s2, &id);
    let id2 = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(token));
    s.client.approve_whitelist_change(&s.s2, &id2);
}

#[test]
#[should_panic(expected = "RiskTierNotDefined")]
fn test_executing_assign_undefined_tier_reverts() {
    let s = setup();
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AssignTier(Address::generate(&s.env), 9));
    s.client.approve_whitelist_change(&s.s2, &id);
}

// ---------- guardian stays suspend-only ----------

#[test]
fn test_guardian_can_still_suspend_with_multisig() {
    let s = setup();
    let token = Address::generate(&s.env);
    let id = s.client.propose_whitelist_change(&s.s1, &MultisigAction::AddToken(token.clone()));
    s.client.approve_whitelist_change(&s.s2, &id);
    s.client.guardian_suspend_token(&s.guardian, &token, &BytesN::from_array(&s.env, &[3u8; 32]));
    assert!(!s.client.is_token_allowed(&token));
}

#[test]
#[should_panic(expected = "NotMultisigSigner")]
fn test_guardian_cannot_propose() {
    let s = setup();
    s.client.propose_whitelist_change(&s.guardian, &MultisigAction::AddToken(Address::generate(&s.env)));
}

#[test]
#[should_panic(expected = "GuardianCannotBeMultisigSigner")]
fn test_signer_cannot_become_guardian() {
    let s = setup();
    s.client.set_guardian(&s.admin, &Some(s.s1.clone()));
}
