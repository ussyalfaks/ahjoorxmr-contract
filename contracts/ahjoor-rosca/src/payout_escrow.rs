//! Route a member's upcoming ROSCA payout straight into an ahjoor-escrow
//! agreement. If escrow creation fails (or no escrow contract is configured),
//! the payout falls back to a direct transfer to the member and a fallback
//! event is emitted.

use crate::errors::ExtError3;
use crate::escrow_client::PayoutEscrowClient;
use crate::{events, DataKey5, PayoutEscrowTarget};
use soroban_sdk::{panic_with_error, token, Address, Env, Vec};

pub(crate) fn validate_target(env: &Env, target: &PayoutEscrowTarget) {
    if target.deadline <= env.ledger().timestamp()
        || target.seller == env.current_contract_address()
        || target.arbiter == env.current_contract_address()
    {
        panic_with_error!(env, ExtError3::InvalidPayoutEscrowTarget);
    }
}

pub(crate) fn get_target(env: &Env, member: &Address) -> Option<PayoutEscrowTarget> {
    env.storage()
        .instance()
        .get(&DataKey5::PayoutEscrowTarget(member.clone()))
}

pub(crate) fn set_target(env: &Env, member: &Address, target: &PayoutEscrowTarget) {
    validate_target(env, target);
    env.storage()
        .instance()
        .set(&DataKey5::PayoutEscrowTarget(member.clone()), target);
}

pub(crate) fn clear_target(env: &Env, member: &Address) {
    env.storage()
        .instance()
        .remove(&DataKey5::PayoutEscrowTarget(member.clone()));
}

/// Attempts to route `amount` into the member's configured escrow target.
/// Returns `false` when no target is configured so the caller can continue
/// with normal payout delivery.
pub(crate) fn try_route(
    env: &Env,
    round: u32,
    member: &Address,
    token_addr: &Address,
    amount: i128,
) -> bool {
    let target = match get_target(env, member) {
        Some(target) => target,
        None => return false,
    };

    // The target is for the member's upcoming payout only.
    clear_target(env, member);

    // Move the payout to the member first: ahjoor-escrow funds the escrow by
    // pulling the amount from the buyer (the member).
    token::Client::new(env, token_addr).transfer(&env.current_contract_address(), member, &amount);

    let escrow_contract: Option<Address> = env.storage().instance().get(&DataKey5::EscrowContract);

    if let Some(escrow_contract) = escrow_contract {
        let client = PayoutEscrowClient::new(env, &escrow_contract);
        let result = client.try_create_escrow(
            member,
            &target.seller,
            &target.arbiter,
            &amount,
            token_addr,
            &target.deadline,
            &None,
            &Vec::new(env),
            &false,
            &0u32,
        );
        if let Ok(Ok(escrow_id)) = result {
            events::emit_payout_escrow_created(
                env,
                round,
                member.clone(),
                escrow_id,
                target.seller,
                token_addr.clone(),
                amount,
            );
            return true;
        }
    }

    // The payout already sits with the member, so the fallback is a direct
    // transfer. Emit an event so indexers can see that escrow routing failed.
    events::emit_payout_escrow_fallback(
        env,
        round,
        member.clone(),
        target.seller,
        token_addr.clone(),
        amount,
    );
    true
}
