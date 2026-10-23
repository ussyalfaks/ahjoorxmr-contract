//! Group metadata: a display name, category and description hash per group,
//! editable by the admin (or a co-admin holding `ManageMetadata`), and the
//! public directory entry that carries it.

use crate::errors::ExtError3;
use crate::{
    events, DataKey, DataKey2, DataKey6, GroupDirectoryEntry, GroupMetadata, GroupStatus,
};
use soroban_sdk::{panic_with_error, Address, BytesN, Env, String, Symbol, Vec};

/// Maximum group name length, in bytes.
pub(crate) const MAX_GROUP_NAME_LEN: u32 = 64;

pub(crate) fn get_metadata(env: &Env) -> Option<GroupMetadata> {
    env.storage().instance().get(&DataKey6::GroupMetadata)
}

pub(crate) fn set_metadata(
    env: &Env,
    updated_by: &Address,
    name: String,
    category: Symbol,
    description_hash: BytesN<32>,
) {
    let len = name.len();
    if len == 0 {
        panic_with_error!(env, ExtError3::EmptyGroupName);
    }
    if len > MAX_GROUP_NAME_LEN {
        panic_with_error!(env, ExtError3::GroupNameTooLong);
    }

    let metadata = GroupMetadata {
        name: name.clone(),
        category: category.clone(),
        description_hash: description_hash.clone(),
        updated_at_ledger: env.ledger().sequence(),
    };
    env.storage()
        .instance()
        .set(&DataKey6::GroupMetadata, &metadata);

    events::emit_group_metadata_updated(env, updated_by.clone(), name, category, description_hash);
}

pub(crate) fn directory_entry(env: &Env) -> GroupDirectoryEntry {
    let members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(Vec::new(env));
    GroupDirectoryEntry {
        group_id: env.current_contract_address(),
        metadata: match get_metadata(env) {
            Some(metadata) => Vec::from_array(env, [metadata]),
            None => Vec::new(env),
        },
        status: env
            .storage()
            .instance()
            .get(&DataKey2::GroupStatus)
            .unwrap_or(GroupStatus::Active),
        token: env
            .storage()
            .instance()
            .get(&DataKey::Token)
            .expect("Not initialized"),
        contribution_amount: env
            .storage()
            .instance()
            .get(&DataKey::ContributionAmt)
            .unwrap_or(0),
        member_count: members.len(),
        max_members: env
            .storage()
            .instance()
            .get(&DataKey::MaxMembers)
            .unwrap_or(50),
        current_round: env
            .storage()
            .instance()
            .get(&DataKey::CurrentRound)
            .unwrap_or(0),
    }
}
