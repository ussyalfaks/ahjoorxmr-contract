extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, token::StellarAssetClient, vec, Env};

struct Setup {
    env: Env,
    treasury: TreasuryContractClient<'static>,
    admin: Address,
    token: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(TreasuryContract, ());
    let treasury = TreasuryContractClient::new(&env, &id);
    treasury.initialize(&admin);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    Setup { env, treasury, admin, token }
}

fn fund(s: &Setup, amount: i128) {
    // Simulates a contract paying its fee to the treasury address.
    StellarAssetClient::new(&s.env, &s.token).mint(&s.treasury.address, &amount);
}

fn share(recipient: &Address, bps: u32) -> Share {
    Share { recipient: recipient.clone(), bps }
}

#[test]
fn distribute_splits_by_bps_and_drains() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    let c = Address::generate(&s.env);
    s.treasury.set_distribution(
        &s.admin,
        &vec![&s.env, share(&a, 5_000), share(&b, 3_000), share(&c, 2_000)],
    );
    fund(&s, 1_001);

    let paid = s.treasury.distribute(&s.token);
    assert_eq!(paid, 1_001);

    let tok = token::Client::new(&s.env, &s.token);
    assert_eq!(tok.balance(&a), 500);
    assert_eq!(tok.balance(&b), 300);
    // Rounding dust goes to the last recipient.
    assert_eq!(tok.balance(&c), 201);
    assert_eq!(tok.balance(&s.treasury.address), 0);
}

#[test]
fn preview_matches_distribution() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    s.treasury
        .set_distribution(&s.admin, &vec![&s.env, share(&a, 7_000), share(&b, 3_000)]);
    fund(&s, 10);
    let p = s.treasury.preview_distribution(&s.token);
    assert_eq!(p, vec![&s.env, (a, 7), (b, 3)]);
}

#[test]
fn distribute_without_distribution_fails() {
    let s = setup();
    fund(&s, 100);
    let r = s.treasury.try_distribute(&s.token);
    assert_eq!(r, Err(Ok(TreasuryError::DistributionNotSet.into())));
}

#[test]
fn distribute_empty_balance_fails() {
    let s = setup();
    let a = Address::generate(&s.env);
    s.treasury.set_distribution(&s.admin, &vec![&s.env, share(&a, 10_000)]);
    let r = s.treasury.try_distribute(&s.token);
    assert_eq!(r, Err(Ok(TreasuryError::NothingToDistribute.into())));
}

#[test]
fn set_distribution_validation() {
    let s = setup();
    let a = Address::generate(&s.env);
    let b = Address::generate(&s.env);
    let e = &s.env;
    let t = &s.treasury;
    assert_eq!(
        t.try_set_distribution(&s.admin, &vec![e]),
        Err(Ok(TreasuryError::InvalidRecipientCount.into()))
    );
    assert_eq!(
        t.try_set_distribution(&s.admin, &vec![e, share(&a, 9_999)]),
        Err(Ok(TreasuryError::InvalidBpsTotal.into()))
    );
    assert_eq!(
        t.try_set_distribution(&s.admin, &vec![e, share(&a, 10_000), share(&b, 0)]),
        Err(Ok(TreasuryError::ZeroShare.into()))
    );
    assert_eq!(
        t.try_set_distribution(&s.admin, &vec![e, share(&a, 5_000), share(&a, 5_000)]),
        Err(Ok(TreasuryError::DuplicateRecipient.into()))
    );
    assert_eq!(
        t.try_set_distribution(&s.admin, &vec![e, share(&t.address, 10_000)]),
        Err(Ok(TreasuryError::TreasuryAsRecipient.into()))
    );
}

#[test]
fn non_admin_cannot_set_distribution() {
    let s = setup();
    let a = Address::generate(&s.env);
    let r = s
        .treasury
        .try_set_distribution(&Address::generate(&s.env), &vec![&s.env, share(&a, 10_000)]);
    assert_eq!(r, Err(Ok(TreasuryError::Unauthorized.into())));
}

#[test]
fn admin_rotation() {
    let s = setup();
    let new_admin = Address::generate(&s.env);
    assert_eq!(
        s.treasury.try_accept_admin(&new_admin),
        Err(Ok(TreasuryError::Unauthorized.into()))
    );
    s.treasury.propose_admin(&s.admin, &new_admin);
    s.treasury.accept_admin(&new_admin);
    assert_eq!(s.treasury.get_admin(), new_admin);
}

#[test]
fn double_initialize_fails() {
    let s = setup();
    assert_eq!(
        s.treasury.try_initialize(&s.admin),
        Err(Ok(TreasuryError::AlreadyInitialized.into()))
    );
}
