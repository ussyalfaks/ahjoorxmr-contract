//! Payout splits: send one member's payout to several addresses by basis
//! points. Splits are mutually exclusive with a single payout beneficiary.
//! Any rounding dust is credited to the payout recipient (the primary
//! recipient).

use crate::errors::ExtError3;
use crate::{events, DataKey5};
use soroban_sdk::{panic_with_error, token, Address, Env, Vec};

/// Maximum number of addresses a payout may be split across.
pub(crate) const MAX_PAYOUT_SPLIT_RECIPIENTS: u32 = 10;

fn panic_invalid(env: &Env) -> ! {
    panic_with_error!(env, ExtError3::InvalidPayoutSplit)
}

pub(crate) fn get(env: &Env, member: &Address) -> Option<Vec<(Address, u32)>> {
    env.storage()
        .instance()
        .get(&DataKey5::PayoutSplits(member.clone()))
}

pub(crate) fn validate(env: &Env, splits: &Vec<(Address, u32)>) {
    let count = splits.len();
    if count == 0 || count > MAX_PAYOUT_SPLIT_RECIPIENTS {
        panic_invalid(env);
    }

    let mut seen: Vec<Address> = Vec::new(env);
    let mut total_bps: u32 = 0;
    for (recipient, bps) in splits.iter() {
        if bps == 0 || recipient == env.current_contract_address() || seen.contains(&recipient) {
            panic_invalid(env);
        }
        seen.push_back(recipient);
        total_bps = total_bps.saturating_add(bps);
    }

    if total_bps != 10_000 {
        panic_invalid(env);
    }
}

pub(crate) fn set(env: &Env, member: &Address, splits: &Vec<(Address, u32)>) {
    validate(env, splits);
    env.storage()
        .instance()
        .set(&DataKey5::PayoutSplits(member.clone()), splits);
}

pub(crate) fn clear(env: &Env, member: &Address) {
    env.storage()
        .instance()
        .remove(&DataKey5::PayoutSplits(member.clone()));
}

/// Resolve `amount` into `(recipient, share)` pairs. With splits configured,
/// every recipient receives `amount * bps / 10_000` and any dust goes to
/// `member`. Without splits, the single beneficiary (or the member) receives
/// the whole amount.
pub(crate) fn resolve(env: &Env, member: &Address, amount: i128) -> Vec<(Address, i128)> {
    let mut destinations: Vec<(Address, i128)> = Vec::new(env);

    if let Some(splits) = get(env, member) {
        let mut allocated: i128 = 0;
        for (recipient, bps) in splits.iter() {
            let share = amount * bps as i128 / 10_000;
            if share > 0 {
                destinations.push_back((recipient, share));
                allocated += share;
            }
        }

        let dust = amount - allocated;
        if dust > 0 {
            let mut credited = false;
            for i in 0..destinations.len() {
                let (recipient, share) = destinations.get(i).unwrap();
                if recipient == *member {
                    destinations.set(i, (recipient, share + dust));
                    credited = true;
                    break;
                }
            }
            if !credited {
                destinations.push_back((member.clone(), dust));
            }
        }
    } else {
        let recipient: Address = env
            .storage()
            .instance()
            .get(&DataKey5::PayoutBeneficiary(member.clone()))
            .unwrap_or(member.clone());
        destinations.push_back((recipient, amount));
    }

    destinations
}

/// Transfer a round payout according to the member's splits / beneficiary and
/// emit one delivery event per recipient.
pub(crate) fn deliver(env: &Env, round: u32, member: &Address, token_addr: &Address, amount: i128) {
    let destinations = resolve(env, member, amount);
    let client = token::Client::new(env, token_addr);
    for (recipient, share) in destinations.iter() {
        if share > 0 {
            client.transfer(&env.current_contract_address(), &recipient, &share);
            events::emit_payout_delivered(
                env,
                round,
                member.clone(),
                recipient,
                token_addr.clone(),
                share,
            );
        }
    }
}
