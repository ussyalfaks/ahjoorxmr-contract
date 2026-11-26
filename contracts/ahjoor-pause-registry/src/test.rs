extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, vec, Env, String};

fn setup(env: &Env, threshold: u32) -> (PauseRegistryContractClient<'static>, Vec<Address>) {
    let id = env.register(PauseRegistryContract, ());
    let client = PauseRegistryContractClient::new(env, &id);
    let guardians = vec![
        env,
        Address::generate(env),
        Address::generate(env),
        Address::generate(env),
    ];
    client.initialize(&guardians, &threshold);
    (client, guardians)
}

fn reason(env: &Env) -> String {
    String::from_str(env, "incident")
}

#[test]
fn pause_and_resume_with_quorum() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, g) = setup(&env, 2);
    assert!(!c.is_paused());
    let approvers = vec![&env, g.get(0).unwrap(), g.get(1).unwrap()];
    c.pause_all(&approvers, &reason(&env));
    assert!(c.is_paused());
    assert_eq!(env.auths().len(), 2);
    c.resume_all(&approvers);
    assert!(!c.is_paused());
}

#[test]
fn below_threshold_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, g) = setup(&env, 2);
    let r = c.try_pause_all(&vec![&env, g.get(0).unwrap()], &reason(&env));
    assert_eq!(r, Err(Ok(PauseRegistryError::ThresholdNotMet.into())));
}

#[test]
fn non_guardian_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, g) = setup(&env, 1);
    let r = c.try_pause_all(&vec![&env, g.get(0).unwrap(), Address::generate(&env)], &reason(&env));
    assert_eq!(r, Err(Ok(PauseRegistryError::NotGuardian.into())));
}

#[test]
fn duplicate_approver_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, g) = setup(&env, 2);
    let a = g.get(0).unwrap();
    let r = c.try_pause_all(&vec![&env, a.clone(), a], &reason(&env));
    assert_eq!(r, Err(Ok(PauseRegistryError::DuplicateApprover.into())));
}

#[test]
fn missing_auth_rejected() {
    let env = Env::default();
    let (c, g) = setup(&env, 1);
    assert!(c.try_pause_all(&vec![&env, g.get(0).unwrap()], &reason(&env)).is_err());
    assert!(!c.is_paused());
}

#[test]
fn double_pause_and_resume_when_not_paused_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, g) = setup(&env, 1);
    let a = vec![&env, g.get(0).unwrap()];
    assert_eq!(c.try_resume_all(&a), Err(Ok(PauseRegistryError::NotPaused.into())));
    c.pause_all(&a, &reason(&env));
    assert_eq!(c.try_pause_all(&a, &reason(&env)), Err(Ok(PauseRegistryError::AlreadyPaused.into())));
}

#[test]
fn invalid_init_rejected() {
    let env = Env::default();
    let id = env.register(PauseRegistryContract, ());
    let c = PauseRegistryContractClient::new(&env, &id);
    let g = Address::generate(&env);
    assert_eq!(
        c.try_initialize(&vec![&env, g.clone()], &2),
        Err(Ok(PauseRegistryError::InvalidThreshold.into()))
    );
    assert_eq!(
        c.try_initialize(&vec![&env, g.clone(), g.clone()], &1),
        Err(Ok(PauseRegistryError::InvalidGuardians.into()))
    );
    c.initialize(&vec![&env, g.clone()], &1);
    assert_eq!(
        c.try_initialize(&vec![&env, g], &1),
        Err(Ok(PauseRegistryError::AlreadyInitialized.into()))
    );
}

#[test]
fn rotate_guardians() {
    let env = Env::default();
    env.mock_all_auths();
    let (c, g) = setup(&env, 1);
    let new_g = Address::generate(&env);
    c.set_guardians(&vec![&env, g.get(0).unwrap()], &vec![&env, new_g.clone()], &1);
    assert_eq!(c.get_guardian_config().guardians, vec![&env, new_g.clone()]);
    let r = c.try_pause_all(&vec![&env, g.get(0).unwrap()], &reason(&env));
    assert_eq!(r, Err(Ok(PauseRegistryError::NotGuardian.into())));
    c.pause_all(&vec![&env, new_g], &reason(&env));
    assert!(c.is_paused());
}
