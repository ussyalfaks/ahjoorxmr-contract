#![cfg(test)]
use super::*;
use crate::errors::{ExtError2, ExtError3};
use soroban_sdk::{
    testutils::{Address as _, Events},
    Address, BytesN, Env, IntoVal, String, Symbol, Vec,
};

struct Setup<'a> {
    env: Env,
    contract_id: Address,
    client: AhjoorContractClient<'a>,
    admin: Address,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();

    let mut members = Vec::new(&env);
    members.push_back(Address::generate(&env));
    members.push_back(Address::generate(&env));

    client.init(
        &admin,
        &members,
        &100,
        &token,
        &3_600,
        &RoscaConfig {
            strategy: PayoutStrategy::RoundRobin,
            custom_order: None,
            penalty_amount: 0,
            exit_penalty_bps: 0,
            collective_goal: None,
            member_goals: None,
            fee_bps: 0,
            fee_recipient: None,
            max_defaults: 3,
            grace_period_ledgers: 0,
            use_timestamp_schedule: false,
            round_duration_seconds: 0,
            max_members: None,
            skip_fee: 0,
            max_skips_per_cycle: 0,
            voting_mode: VotingMode::Equal,
            late_fee_bps: 0,
            grace_period_seconds: 0,
            auction_enabled: false,
            auction_window_ledgers: 0,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );

    Setup { env, contract_id, client, admin }
}

fn count_topic_events(env: &Env, topic: &str) -> u32 {
    let expected_topics = (Symbol::new(env, topic),).into_val(env);
    let mut count = 0u32;
    for event in env.events().all().iter() {
        if event.1 == expected_topics {
            count += 1;
        }
    }
    count
}

fn hash(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

#[test]
fn test_admin_sets_metadata_and_event_is_emitted() {
    let s = setup();
    assert_eq!(s.client.get_group_metadata(&0), None);
    assert_eq!(s.client.get_directory_entry().metadata.len(), 0);

    let name = String::from_str(&s.env, "Lagos Market Traders");
    let category = Symbol::new(&s.env, "traders");
    s.client
        .set_group_metadata(&s.admin, &0, &name, &category, &hash(&s.env, 1));
    assert_eq!(count_topic_events(&s.env, "group_metadata_updated"), 1);

    let metadata = s.client.get_group_metadata(&0).unwrap();
    assert_eq!(metadata.name, name);
    assert_eq!(metadata.category, category);
    assert_eq!(metadata.description_hash, hash(&s.env, 1));

    // Updates replace the previous metadata.
    let renamed = String::from_str(&s.env, "Lagos Traders Circle");
    s.client
        .set_group_metadata(&s.admin, &0, &renamed, &category, &hash(&s.env, 2));
    assert_eq!(count_topic_events(&s.env, "group_metadata_updated"), 1);
    let metadata = s.client.get_group_metadata(&0).unwrap();
    assert_eq!(metadata.name, renamed);
    assert_eq!(metadata.description_hash, hash(&s.env, 2));
}

#[test]
fn test_directory_entry_includes_metadata() {
    let s = setup();
    let name = String::from_str(&s.env, "Family Savings");
    let category = Symbol::new(&s.env, "family");
    s.client
        .set_group_metadata(&s.admin, &0, &name, &category, &hash(&s.env, 3));

    let entry = s.client.get_directory_entry();
    assert_eq!(entry.group_id, s.contract_id);
    assert_eq!(entry.status, GroupStatus::Active);
    assert_eq!(entry.contribution_amount, 100);
    assert_eq!(entry.member_count, 2);
    assert_eq!(entry.current_round, 0);
    let metadata = entry.metadata.get(0).unwrap();
    assert_eq!(metadata.name, name);
    assert_eq!(metadata.category, category);
}

#[test]
fn test_only_admin_or_metadata_co_admin_can_update() {
    let s = setup();
    let name = String::from_str(&s.env, "Group");
    let category = Symbol::new(&s.env, "general");

    let outsider = Address::generate(&s.env);
    let res = s
        .client
        .try_set_group_metadata(&outsider, &0, &name, &category, &hash(&s.env, 1));
    assert_eq!(res.unwrap_err().unwrap(), ExtError2::NotACoAdminRole.into());

    // A co-admin without ManageMetadata is denied.
    let members_admin = Address::generate(&s.env);
    let mut perms = Vec::new(&s.env);
    perms.push_back(CoAdminPermission::ManageMembers);
    s.client.add_co_admin(&s.admin, &members_admin, &perms);
    let res = s
        .client
        .try_set_group_metadata(&members_admin, &0, &name, &category, &hash(&s.env, 1));
    assert_eq!(res.unwrap_err().unwrap(), ExtError2::CoAdminPermissionDenied.into());

    // A co-admin with ManageMetadata may update.
    let metadata_admin = Address::generate(&s.env);
    let mut perms = Vec::new(&s.env);
    perms.push_back(CoAdminPermission::ManageMetadata);
    s.client.add_co_admin(&s.admin, &metadata_admin, &perms);
    s.client
        .set_group_metadata(&metadata_admin, &0, &name, &category, &hash(&s.env, 1));
    assert_eq!(s.client.get_group_metadata(&0).unwrap().name, name);
}

#[test]
fn test_name_length_is_capped() {
    let s = setup();
    let category = Symbol::new(&s.env, "general");

    let max_name = String::from_bytes(&s.env, &[b'a'; 64]);
    s.client
        .set_group_metadata(&s.admin, &0, &max_name, &category, &hash(&s.env, 1));
    assert_eq!(s.client.get_group_metadata(&0).unwrap().name, max_name);

    let too_long = String::from_bytes(&s.env, &[b'a'; 65]);
    let res = s
        .client
        .try_set_group_metadata(&s.admin, &0, &too_long, &category, &hash(&s.env, 1));
    assert_eq!(res.unwrap_err().unwrap(), ExtError3::GroupNameTooLong.into());

    let empty = String::from_str(&s.env, "");
    let res = s
        .client
        .try_set_group_metadata(&s.admin, &0, &empty, &category, &hash(&s.env, 1));
    assert_eq!(res.unwrap_err().unwrap(), ExtError3::EmptyGroupName.into());

    // Rejected updates leave the stored metadata unchanged.
    assert_eq!(s.client.get_group_metadata(&0).unwrap().name, max_name);
}
