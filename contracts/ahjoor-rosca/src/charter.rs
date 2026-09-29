//! Group charter: an off-chain rules document anchored on-chain by hash.
//!
//! Once a charter exists, members must acknowledge its current version before
//! joining (`join_group_tiered`, `join_with_invite`, `add_member`) and before
//! their next contribution. Changing the charter after the group has been
//! activated (first contribution recorded) goes through a `CharterUpdate`
//! governance proposal instead of taking effect immediately.

use crate::errors::ExtError2;
use crate::{
    events, DataKey, DataKey2, DataKey4, DataKey5, GroupCharter, Proposal, ProposalStatus,
    ProposalType,
};
use soroban_sdk::{panic_with_error, Address, BytesN, Env, Map, String};

const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

/// Default voting window for charter update proposals (3 days).
pub(crate) const CHARTER_VOTING_WINDOW_SECONDS: u64 = 3 * 86_400;

pub(crate) fn get_charter(env: &Env) -> Option<GroupCharter> {
    env.storage().instance().get(&DataKey5::GroupCharter)
}

pub(crate) fn acknowledged_version(env: &Env, member: &Address) -> u32 {
    env.storage()
        .persistent()
        .get(&DataKey5::CharterAck(member.clone()))
        .unwrap_or(0)
}

/// The group counts as activated once its first contribution was recorded.
pub(crate) fn is_group_activated(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey4::GroupActivationEmitted)
        .unwrap_or(false)
}

/// Panics with `CharterNotAcknowledged` if a charter exists and `member` has
/// not acknowledged its current version. No-op for groups without a charter.
pub(crate) fn require_charter_acknowledged(env: &Env, member: &Address) {
    if let Some(charter) = get_charter(env) {
        if acknowledged_version(env, member) != charter.version {
            panic_with_error!(env, ExtError2::CharterNotAcknowledged);
        }
    }
}

/// Stores `charter_hash` / `uri` as the next charter version and emits `CharterSet`.
pub(crate) fn apply_charter(env: &Env, charter_hash: BytesN<32>, uri: String) -> u32 {
    let version = get_charter(env).map(|c| c.version + 1).unwrap_or(1);
    let charter = GroupCharter {
        version,
        charter_hash: charter_hash.clone(),
        uri: uri.clone(),
        set_at_ledger: env.ledger().sequence(),
    };
    env.storage().instance().set(&DataKey5::GroupCharter, &charter);
    events::emit_charter_set(env, version, charter_hash, uri);
    version
}

/// Opens a governance proposal of `proposal_type` created by `proposer`,
/// voting for `CHARTER_VOTING_WINDOW_SECONDS`, and returns its id. Shared by
/// the charter and payout-vesting update flows.
pub(crate) fn open_governance_proposal(
    env: &Env,
    proposer: &Address,
    proposal_type: ProposalType,
    description: &str,
) -> u32 {
    let mut proposal_counter: u32 = env
        .storage()
        .instance()
        .get(&DataKey::ProposalCounter)
        .unwrap_or(0);
    let proposal_id = proposal_counter;
    proposal_counter += 1;

    let quorum_config: Map<ProposalType, u32> = env
        .storage()
        .instance()
        .get(&DataKey2::QuorumConfig)
        .unwrap_or(Map::new(env));
    let required_quorum = quorum_config
        .get(proposal_type)
        .unwrap_or_else(|| {
            let global_q: u32 = env
                .storage()
                .instance()
                .get(&DataKey::QuorumPercentage)
                .unwrap_or(51);
            global_q * 100
        });

    let current_time = env.ledger().timestamp();
    let deadline = current_time + CHARTER_VOTING_WINDOW_SECONDS;
    let proposal = Proposal {
        id: proposal_id,
        proposal_type,
        creator: proposer.clone(),
        description: String::from_str(env, description),
        target_member: proposer.clone(),
        votes_for: 0,
        votes_against: 0,
        created_at: current_time,
        deadline,
        status: ProposalStatus::Pending,
        execution_data: None,
        required_quorum,
    };

    let mut proposals: Map<u32, Proposal> = env
        .storage()
        .instance()
        .get(&DataKey::Proposals)
        .unwrap_or(Map::new(env));
    proposals.set(proposal_id, proposal);
    env.storage().instance().set(&DataKey::Proposals, &proposals);

    let mut proposal_votes: Map<u32, Map<Address, bool>> = env
        .storage()
        .instance()
        .get(&DataKey::ProposalVotes)
        .unwrap_or(Map::new(env));
    proposal_votes.set(proposal_id, Map::new(env));
    env.storage()
        .instance()
        .set(&DataKey::ProposalVotes, &proposal_votes);
    env.storage()
        .instance()
        .set(&DataKey::ProposalCounter, &proposal_counter);

    events::emit_prop_new(
        env,
        proposal_id,
        proposer.clone(),
        proposer.clone(),
        current_time,
        deadline,
    );
    proposal_id
}

/// Opens a `CharterUpdate` proposal carrying the new charter. The charter is
/// applied by `execute_proposal` if the vote passes.
pub(crate) fn propose_charter_update(
    env: &Env,
    proposer: &Address,
    charter_hash: BytesN<32>,
    uri: String,
) -> u32 {
    let proposal_id = open_governance_proposal(
        env,
        proposer,
        ProposalType::CharterUpdate,
        "Group charter update",
    );

    let pending_key = DataKey5::PendingCharter(proposal_id);
    env.storage()
        .persistent()
        .set(&pending_key, &(charter_hash.clone(), uri));
    env.storage().persistent().extend_ttl(
        &pending_key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );

    events::emit_charter_update_proposed(env, proposal_id, proposer.clone(), charter_hash);
    proposal_id
}

/// Called from `execute_proposal` for an approved `CharterUpdate` proposal.
/// A proposal with no pending charter attached (e.g. one opened via the
/// generic `create_proposal`) executes as a no-op.
pub(crate) fn execute_charter_update(env: &Env, proposal_id: u32) {
    let pending_key = DataKey5::PendingCharter(proposal_id);
    let pending: Option<(BytesN<32>, String)> = env.storage().persistent().get(&pending_key);
    if let Some((charter_hash, uri)) = pending {
        env.storage().persistent().remove(&pending_key);
        apply_charter(env, charter_hash, uri);
    }
}

/// Records that `member` accepts charter `version`, which must be current.
pub(crate) fn acknowledge(env: &Env, member: &Address, version: u32) {
    let charter = match get_charter(env) {
        Some(c) => c,
        None => panic_with_error!(env, ExtError2::CharterNotSet),
    };
    if version != charter.version {
        panic_with_error!(env, ExtError2::CharterVersionMismatch);
    }
    let key = DataKey5::CharterAck(member.clone());
    env.storage().persistent().set(&key, &version);
    env.storage()
        .persistent()
        .extend_ttl(&key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    events::emit_charter_acknowledged(env, member.clone(), version);
}
