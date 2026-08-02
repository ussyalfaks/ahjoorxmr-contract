use crate::{errors::{Error, ExtError}, events, audit_trail, ContributionEntry, CycleSnapshotData, DataKey, DataKey2, DataKey3, DataKey4, PersistentKey, PayoutRecord, SlotBid, types::{InsuranceClaim, InsuranceCoverageMode}};
use soroban_sdk::{panic_with_error, token, Address, Bytes, BytesN, Env, Map, Vec};

/// Returns the timestamp (seconds) after which the grace period for a given round deadline expires.
///
/// Branches on `DataKey2::UseTimestampSchedule`:
/// - timestamp-mode: adds `GracePeriodSeconds` (seconds) to `round_deadline`
/// - ledger-mode: adds `GracePeriodLedgers` (treated as seconds for timestamp comparison)
pub(crate) fn get_grace_deadline(env: &Env, round_deadline: u64) -> u64 {
    let use_timestamp: bool = env
        .storage()
        .instance()
        .get(&DataKey2::UseTimestampSchedule)
        .unwrap_or(false);
    if use_timestamp {
        let grace_seconds: u64 = env
            .storage()
            .instance()
            .get(&DataKey3::GracePeriodSeconds)
            .unwrap_or(0);
        round_deadline.saturating_add(grace_seconds)
    } else {
        let grace_ledgers: u32 = env
            .storage()
            .instance()
            .get(&DataKey4::GracePeriodLedgers)
            .unwrap_or(0);
        round_deadline.saturating_add(grace_ledgers as u64)
    }
}

const PERSISTENT_LIFETIME_THRESHOLD: u32 = 100_000;
const PERSISTENT_BUMP_AMOUNT: u32 = 120_000;

/// Panics if the contract is currently paused.
pub(crate) fn check_not_paused(env: &Env) {
    let is_paused: bool = env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .or(env.storage().instance().get(&DataKey::IsPaused))
        .unwrap_or(false);
    if is_paused {
        panic_with_error!(env, Error::ContractPaused);
    }
}

/// Panics if the group is currently frozen by the contract-level admin.
pub(crate) fn check_not_frozen(env: &Env) {
    let is_frozen: bool = env
        .storage()
        .instance()
        .get(&DataKey3::IsFrozen)
        .unwrap_or(false);
    if is_frozen {
        panic_with_error!(env, ExtError::GroupFrozen);
    }
}

/// Pays out the current round's pot to the next eligible recipient, records
/// the payout in history, and resets the round state for the next round.
pub(crate) fn complete_round_payout(env: &Env, _paid_members: &Vec<Address>) {
    let current_round: u32 = env
        .storage()
        .instance()
        .get(&DataKey::CurrentRound)
        .unwrap();
    let payout_order: Vec<Address> = env.storage().instance().get(&DataKey::PayoutOrder).unwrap();
    let suspended_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::SuspendedMembers)
        .unwrap_or(Vec::new(env));
    let exited_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::ExitedMembers)
        .unwrap_or(Vec::new(env));

    let skip_requests: Map<(Address, u32), bool> = env
        .storage()
        .instance()
        .get(&DataKey2::SkipRequests)
        .unwrap_or(Map::new(env));

    let mut recipient_idx = (current_round % payout_order.len()) as u32;
    let mut attempts = 0;
    while attempts < payout_order.len() {
        let potential_recipient = payout_order.get(recipient_idx).unwrap();
        let has_skipped = skip_requests.get((potential_recipient.clone(), current_round)).unwrap_or(false);
        if !suspended_members.contains(&potential_recipient)
            && !exited_members.contains(&potential_recipient)
            && !has_skipped
        {
            break;
        }
        recipient_idx = (recipient_idx + 1) % payout_order.len();
        attempts += 1;
    }

    if attempts >= payout_order.len() {
        panic_with_error!(env, Error::AllMembersSuspended);
    }

    let payout_recipient = payout_order.get(recipient_idx).unwrap();
    let preferences: Map<Address, bool> = env
        .storage()
        .instance()
        .get(&DataKey2::ReinvestPreference)
        .unwrap_or(Map::new(env));
    let should_reinvest = preferences.get(payout_recipient.clone()).unwrap_or(false);

    let reward_pool: i128 = env
        .storage()
        .instance()
        .get(&DataKey::RewardPool)
        .unwrap_or(0);
    let base_token: Address = env.storage().instance().get(&DataKey::Token).unwrap();

    let approved_tokens: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::ApprovedTokens)
        .unwrap_or(Vec::new(env));

    // Get protocol fee configuration
    let fee_bps: u32 = env
        .storage()
        .instance()
        .get(&DataKey::FeeBps)
        .unwrap_or(0);
    let fee_recipient_opt: Option<Address> = env
        .storage()
        .instance()
        .get(&DataKey2::FeeRecipient);

    // Apply reputation-gated fee discount if the payout recipient's credit score
    // meets the configured threshold.
    let effective_fee_bps: u32 = if fee_bps > 0 {
        let discount_cfg: Option<crate::RepFeeDiscountConfig> = env
            .storage()
            .instance()
            .get(&DataKey3::RepFeeDiscount);
        if let Some(cfg) = discount_cfg {
            let ms_map: Map<Address, crate::MemberScore> = env
                .storage()
                .persistent()
                .get(&PersistentKey::MemberCreditScores)
                .unwrap_or(Map::new(env));
            let score = ms_map
                .get(payout_recipient.clone())
                .map(|ms| ms.score)
                .unwrap_or(0);
            if score >= cfg.threshold {
                let discounted = fee_bps.saturating_sub(cfg.discount_bps);
                events::emit_rep_fee_discount_applied(
                    env,
                    payout_recipient.clone(),
                    fee_bps,
                    discounted,
                    score,
                );
                discounted
            } else {
                fee_bps
            }
        } else {
            fee_bps
        }
    } else {
        0
    };

    let mut total_payout_history_amt = 0i128;
    let mut reinvested_amount = 0i128;
    let mut total_fee_collected = 0i128;
    let mut insurance_drawn_this_round = 0i128;

    // Calculate expected pot based on member tiers and check for shortfall
    let base_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::ContributionAmt)
        .unwrap_or(0);
    let tiers: Map<Address, u32> = env
        .storage()
        .instance()
        .get(&DataKey2::MemberTiers)
        .unwrap_or(Map::new(env));
    let member_contributions: Map<Address, i128> = env
        .storage()
        .instance()
        .get(&DataKey::MemberContributions)
        .unwrap_or(Map::new(env));

    // Read insurance pool here so we can exclude it from actual_pot.
    let mut insurance_pool: i128 = env
        .storage()
        .instance()
        .get(&DataKey2::InsurancePool)
        .unwrap_or(0);

    // expected_pot = what ALL non-suspended, non-exited members were supposed to contribute.
    // Using all active members (not just paid ones) lets defaulters create a real shortfall.
    let all_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(Vec::new(env));
    let mut expected_pot: i128 = 0;
    for member in all_members.iter() {
        if suspended_members.contains(&member) || exited_members.contains(&member) {
            continue;
        }
        let tier_bps = tiers.get(member.clone()).unwrap_or(10_000);
        let member_expected = (base_amount * tier_bps as i128) / 10_000;
        expected_pot += member_expected;
    }

    // actual_pot = contract balance minus reward pool and insurance reserves.
    // Insurance reserves are not round contributions; excluding them prevents
    // the pool from masking defaulter shortfalls.
    let mut actual_pot: i128 = 0;
    for token_addr in approved_tokens.iter() {
        let client = token::Client::new(env, &token_addr);
        let mut balance = client.balance(&env.current_contract_address());

        if token_addr == base_token {
            balance -= reward_pool;
            balance -= insurance_pool;
            actual_pot = balance;
        }
    }
    let shortfall = expected_pot - actual_pot;
    let coverage_mode: InsuranceCoverageMode = env
        .storage()
        .instance()
        .get(&DataKey2::InsuranceCoverageMode)
        .unwrap_or(InsuranceCoverageMode::Partial);

    if shortfall > 0 && coverage_mode != InsuranceCoverageMode::None {
        let draw_amount = match coverage_mode {
            InsuranceCoverageMode::None => 0,
            InsuranceCoverageMode::Partial => {
                if insurance_pool >= shortfall { shortfall } else { insurance_pool }
            }
            InsuranceCoverageMode::Full => {
                if insurance_pool == 0 {
                    events::emit_insurance_pool_exhausted(env, current_round, shortfall);
                    0
                } else {
                    // Draw as much as available; if pool < shortfall the remainder is uncovered.
                    shortfall.min(insurance_pool)
                }
            }
        };
        if draw_amount > 0 {
            let insurance_pool_before_draw = insurance_pool;
            insurance_drawn_this_round = draw_amount;
            insurance_pool -= draw_amount;
            env.storage().instance().set(&DataKey2::InsurancePool, &insurance_pool);
            events::emit_insurance_paid_out(env, current_round, shortfall, insurance_pool);

            let low_threshold: i128 = env
                .storage()
                .instance()
                .get(&DataKey4::InsurancePoolLowThreshold)
                .unwrap_or(0);
            if low_threshold > 0
                && insurance_pool_before_draw >= low_threshold
                && insurance_pool < low_threshold
            {
                events::emit_insurance_pool_low(env, current_round, low_threshold, insurance_pool);
            }

            let shortfall_remaining = shortfall - draw_amount;
            if insurance_pool == 0 {
                events::emit_insurance_pool_exhausted(env, current_round, shortfall_remaining);
            }
            events::emit_insurance_claim_executed(env, current_round, payout_recipient.clone(), draw_amount);
            let mut claims: Map<u32, Vec<InsuranceClaim>> = env
                .storage()
                .instance()
                .get(&DataKey2::InsuranceClaims)
                .unwrap_or(Map::new(env));
            let mut round_claims: Vec<InsuranceClaim> = claims.get(current_round).unwrap_or(Vec::new(env));
            round_claims.push_back(InsuranceClaim { round: current_round, defaulter: payout_recipient.clone(), amount_covered: draw_amount });
            claims.set(current_round, round_claims);
            env.storage().instance().set(&DataKey2::InsuranceClaims, &claims);
            actual_pot += draw_amount;
        }
    } else if shortfall > 0 && insurance_pool == 0 && coverage_mode != InsuranceCoverageMode::None {
        events::emit_insurance_pool_exhausted(env, current_round, shortfall);
    }

    for token_addr in approved_tokens.iter() {
        let client = token::Client::new(env, &token_addr);
        let mut balance = client.balance(&env.current_contract_address());

        if token_addr == base_token {
            balance -= reward_pool;
            total_payout_history_amt = balance;
        }

        if balance > 0 {
            // Calculate protocol fee (already adjusted for reputation discount)
            let fee_amount = if effective_fee_bps > 0 && fee_recipient_opt.is_some() {
                (balance * (effective_fee_bps as i128)) / 10_000
            } else {
                0
            };

            let payout_amount = balance - fee_amount;

            if should_reinvest && token_addr == base_token {
                reinvested_amount = payout_amount;
                events::emit_payout_reinvested(env, payout_recipient.clone(), current_round, payout_amount);
            } else if payout_amount > 0 {
                // Transfer payout to recipient
                client.transfer(&env.current_contract_address(), &payout_recipient, &payout_amount);
            }

            // Transfer fee to fee recipient
            if fee_amount > 0 {
                if let Some(fee_recipient) = fee_recipient_opt.clone() {
                    client.transfer(&env.current_contract_address(), &fee_recipient, &fee_amount);
                    
                    // Emit fee collected event (only for base token to avoid duplicates)
                    if token_addr == base_token {
                        total_fee_collected = fee_amount;
                        events::emit_fee_collected(env, current_round, fee_amount, fee_recipient);
                    }
                }
            }
        }
    }

    // Persistent: RoundHistory — append new record and extend its individual TTL
    let mut history: Vec<PayoutRecord> = env
        .storage()
        .persistent()
        .get(&PersistentKey::RoundHistory)
        .unwrap_or(Vec::new(env));
    history.push_back(PayoutRecord {
        recipient: payout_recipient.clone(),
        amount: total_payout_history_amt,
    });
    env.storage()
        .persistent()
        .set(&PersistentKey::RoundHistory, &history);
    env.storage().persistent().extend_ttl(
        &PersistentKey::RoundHistory,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );

    events::emit_rd_done(
        env,
        current_round,
        payout_recipient.clone(),
        total_payout_history_amt,
    );

    // #364: Create immutable cycle snapshot before state is reset
    {
        let snap_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Members)
            .unwrap_or(Vec::new(env));
        let mut preimage = Bytes::new(env);
        preimage.extend_from_array(&current_round.to_be_bytes());
        preimage.extend_from_array(&total_payout_history_amt.to_be_bytes());
        preimage.extend_from_array(&(payout_order.len() as u32).to_be_bytes());
        let snap_hash: BytesN<32> = env.crypto().sha256(&preimage).into();
        let cycle_snapshot = CycleSnapshotData {
            cycle_number: current_round,
            members: snap_members,
            contribution_amounts: member_contributions.clone(),
            payout_queue: payout_order.clone(),
            pool_balance: total_payout_history_amt,
            timestamp: env.ledger().timestamp(),
            snapshot_hash: snap_hash.clone(),
        };
        env.storage()
            .persistent()
            .set(&PersistentKey::CycleSnapshot(current_round), &cycle_snapshot);
        env.storage().persistent().extend_ttl(
            &PersistentKey::CycleSnapshot(current_round),
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        events::emit_snapshot_created(env, 0u32, current_round, snap_hash);
    }

    // ── Issue #402: Record cycle audit trail with proper timestamps ────────────
    // Collect contribution entries for audit trail
    let mut contributions: Vec<ContributionEntry> = Vec::new(env);
    for (member, amount) in member_contributions.iter() {
        contributions.push_back(ContributionEntry {
            member: member.clone(),
            amount,
            timestamp: env.ledger().timestamp(),
        });
    }

    // Get defaulters from storage (set during finalize_round)
    let defaulters: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Defaulters)
        .unwrap_or(Vec::new(env));

    // Get skippers for this round
    let skip_requests: Map<(Address, u32), bool> = env
        .storage()
        .instance()
        .get(&DataKey2::SkipRequests)
        .unwrap_or(Map::new(env));
    let mut skippers: Vec<Address> = Vec::new(env);
    for (key, skipped) in skip_requests.iter() {
        let (addr, round_num) = key;
        if skipped && round_num == current_round {
            skippers.push_back(addr);
        }
    }

    // Get cycle timestamps with ledger-mode fix for non-zero values
    let use_timestamp_schedule: bool = env
        .storage()
        .instance()
        .get(&DataKey2::UseTimestampSchedule)
        .unwrap_or(false);

    let cycle_end_timestamp = if use_timestamp_schedule {
        env.ledger().timestamp()
    } else {
        // In ledger-mode, use sequence number as timestamp (fixes Issue #402)
        env.ledger().sequence() as u64
    };

    // Cycle 0's start isn't captured by the "new cycle begins" hook below
    // (that only fires once a preceding round has completed), so fall back
    // to this round's own end-of-processing marker rather than reporting 0.
    let recorded_cycle_start = audit_trail::get_cycle_start_timestamp(env, current_round);
    let cycle_start_timestamp = if recorded_cycle_start > 0 {
        recorded_cycle_start
    } else {
        cycle_end_timestamp
    };

    // Get penalty and insurance amounts from storage
    let penalty_amount: i128 = env
        .storage()
        .instance()
        .get(&DataKey::PenaltyAmount)
        .unwrap_or(0);

    // Calculate penalties collected and insurance drawn from events/defaults
    // (This is conservative; actual penalties are applied per defaulter)
    let penalties_collected = if !defaulters.is_empty() {
        (defaulters.len() as i128) * penalty_amount
    } else {
        0
    };

    let insurance_drawn = insurance_drawn_this_round;

    // Record the cycle audit trail
    audit_trail::record_cycle_audit(
        env,
        current_round,
        total_payout_history_amt,
        payout_recipient.clone(),
        total_payout_history_amt,
        contributions,
        defaulters,
        skippers,
        penalties_collected,
        total_fee_collected,
        insurance_drawn,
        cycle_start_timestamp,
        cycle_end_timestamp,
    );

    reset_round_state(env, current_round);

    // Apply reinvestment to the next round's contributions
    if should_reinvest && reinvested_amount > 0 {
        let mut next_contributions: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&DataKey::MemberContributions)
            .unwrap_or(Map::new(env));
        
        next_contributions.set(payout_recipient.clone(), reinvested_amount);
        env.storage()
            .instance()
            .set(&DataKey::MemberContributions, &next_contributions);
        
        // Check if this reinvestment fulfills the next round's requirement
        let base_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::ContributionAmt)
            .unwrap_or(0);
        let tiers: Map<Address, u32> = env
            .storage()
            .instance()
            .get(&DataKey2::MemberTiers)
            .unwrap_or(Map::new(env));
        let tier_bps = tiers.get(payout_recipient.clone()).unwrap_or(10_000);
        let member_required = (base_amount * tier_bps as i128) / 10_000;

        if reinvested_amount >= member_required {
            let mut next_paid_members: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::PaidMembers)
                .unwrap_or(Vec::new(env));
            if !next_paid_members.contains(&payout_recipient) {
                next_paid_members.push_back(payout_recipient.clone());
                env.storage()
                    .instance()
                    .set(&DataKey::PaidMembers, &next_paid_members);
            }

            // Track reward participation for the next round
            let mut total_participations: u32 = env
                .storage()
                .instance()
                .get(&DataKey::TotalParticipations)
                .unwrap_or(0);
            let mut member_participation: Map<Address, u32> = env
                .storage()
                .instance()
                .get(&DataKey::MemberParticipation)
                .unwrap_or(Map::new(env));

            let current_participation = member_participation.get(payout_recipient.clone()).unwrap_or(0);
            member_participation.set(payout_recipient.clone(), current_participation + 1);
            total_participations += 1;

            env.storage()
                .instance()
                .set(&DataKey::TotalParticipations, &total_participations);
            env.storage()
                .instance()
                .set(&DataKey::MemberParticipation, &member_participation);
        }
    }
}

/// Advances the round counter, clears paid-members and per-round contributions,
/// and sets a new deadline.
pub(crate) fn reset_round_state(env: &Env, current_round: u32) {
    // #227: Apply pending round duration if one was scheduled
    let pending_duration: Option<u64> = env.storage().instance().get(&DataKey4::PendingRoundDuration);
    let duration: u64 = if let Some(pending) = pending_duration {
        env.storage().instance().set(&DataKey::RoundDuration, &pending);
        env.storage().instance().remove(&DataKey4::PendingRoundDuration);
        // Also update RoundDurationSeconds for timestamp-based scheduling
        env.storage().instance().set(&DataKey2::RoundDurationSeconds, &pending);
        events::emit_round_duration_applied(env, current_round + 1, pending);
        pending
    } else {
        env.storage().instance().get(&DataKey::RoundDuration).unwrap()
    };
    let new_round = current_round + 1;
    env.storage()
        .instance()
        .set(&DataKey::CurrentRound, &new_round);
    env.storage()
        .instance()
        .set(&DataKey::PaidMembers, &Vec::<Address>::new(env));
    env.storage().instance().set(
        &DataKey::MemberContributions,
        &Map::<Address, i128>::new(env),
    );
    // Note: DataKey::Defaulters is deliberately left as-is here — close_round/
    // finalize_round just populated it with the round that's ending, and
    // penalise_defaulter/request_penalty_grace need it to survive into the
    // next transaction. It gets overwritten fresh the next time a round closes.
    env.storage().instance().set(
        &DataKey::RoundDeadline,
        &(env.ledger().timestamp() + duration),
    );

    // Update timestamp-based deadline if enabled
    let use_timestamp: bool = env
        .storage()
        .instance()
        .get(&DataKey2::UseTimestampSchedule)
        .unwrap_or(false);

    if use_timestamp {
        let duration_seconds: u64 = env
            .storage()
            .instance()
            .get(&DataKey2::RoundDurationSeconds)
            .unwrap_or(0);
        let next_timestamp_deadline = env.ledger().timestamp() + duration_seconds;
        env.storage()
            .instance()
            .set(&DataKey::RoundDeadlineTimestamp, &next_timestamp_deadline);
        events::emit_round_deadline_timestamp_set(env, new_round, next_timestamp_deadline);
    }

    // ── Issue #402: Record cycle start timestamp when a new cycle begins ─────────
    let payout_order: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PayoutOrder)
        .unwrap_or(Vec::new(env));
    let cycle_len = payout_order.len() as u32;
    if cycle_len > 0 && new_round % cycle_len == 0 {
        // New cycle starts, record the timestamp with ledger-mode fix
        let cycle_number = new_round / cycle_len;
        let cycle_start_timestamp = if use_timestamp {
            env.ledger().timestamp()
        } else {
            // In ledger-mode, use sequence number as timestamp (fixes Issue #402)
            env.ledger().sequence() as u64
        };
        audit_trail::record_cycle_start(env, cycle_number, cycle_start_timestamp);
    }

    // Slot Auction: open a new auction at the start of each cycle
    let auction_enabled: bool = env
        .storage()
        .instance()
        .get(&DataKey3::AuctionEnabled)
        .unwrap_or(false);
    if auction_enabled {
        let payout_order_len: u32 = {
            let order: Vec<Address> = env
                .storage()
                .instance()
                .get(&DataKey::PayoutOrder)
                .unwrap_or(Vec::new(env));
            order.len() as u32
        };
        let is_cycle_start = payout_order_len > 0 && new_round % payout_order_len == 0;
        if is_cycle_start {
            let window: u64 = env
                .storage()
                .instance()
                .get(&DataKey3::AuctionWindowLedgers)
                .unwrap_or(0);
            let open_until = env.ledger().timestamp() + window;
            env.storage()
                .instance()
                .set(&DataKey3::AuctionOpenUntil, &open_until);
            // Clear any leftover bids from a previous auction
            env.storage()
                .instance()
                .set(&DataKey3::AuctionBids, &Vec::<SlotBid>::new(env));
            env.storage()
                .instance()
                .set(&DataKey3::AuctionRound, &new_round);
        }
    }

    events::emit_reset(env, current_round);
}

/// Resets a member's default count and removes them from the suspended list.
pub(crate) fn execute_penalty_appeal(env: &Env, member: &Address) {
    let mut default_count: Map<Address, u32> = env
        .storage()
        .instance()
        .get(&DataKey::DefaultCount)
        .unwrap_or(Map::new(env));

    default_count.set(member.clone(), 0);
    env.storage()
        .instance()
        .set(&DataKey::DefaultCount, &default_count);

    let suspended_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::SuspendedMembers)
        .unwrap_or(Vec::new(env));
    let mut new_suspended = Vec::new(env);
    for m in suspended_members.iter() {
        if m != *member {
            new_suspended.push_back(m);
        }
    }
    env.storage()
        .instance()
        .set(&DataKey::SuspendedMembers, &new_suspended);

    events::emit_appeal_ok(env, member.clone());
}

/// Updates the quorum percentage if the value is within [1, 100].
pub(crate) fn execute_rule_change(env: &Env, new_quorum: Option<i128>) {
    if let Some(quorum) = new_quorum {
        if quorum >= 1 && quorum <= 100 {
            env.storage()
                .instance()
                .set(&DataKey::QuorumPercentage, &(quorum as u32));
            events::emit_rule_upd(env, quorum);
        }
    }
}

/// Updates the maximum member limit if the value is within [1, 100] and >= current count.
///
/// Validates the i128 range (including negativity) *before* casting to u32 so a
/// negative input cannot wrap to a large u32 and rely on a later bounds check.
pub(crate) fn execute_max_members_update(env: &Env, new_max_val: Option<i128>) {
    if let Some(new_max_i128) = new_max_val {
        if new_max_i128 < 1 || new_max_i128 > 100 {
            panic_with_error!(env, Error::InvalidMaxMembers);
        }
        let new_max = new_max_i128 as u32;

        let current_members: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::Members)
            .unwrap_or(Vec::new(env));

        if new_max >= current_members.len() as u32 {
            let old_max: u32 = env
                .storage()
                .instance()
                .get(&DataKey::MaxMembers)
                .unwrap_or(50);

            env.storage()
                .instance()
                .set(&DataKey::MaxMembers, &new_max);

            events::emit_max_members_upd(env, old_max, new_max);
        }
    }
}

/// Removes a member from both the members list and the payout order.
pub(crate) fn execute_member_removal(env: &Env, member: &Address) {
    let old_members: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::Members)
        .unwrap_or(Vec::new(env));
    let mut new_members: Vec<Address> = Vec::new(env);
    for m in old_members.iter() {
        if m != *member {
            new_members.push_back(m);
        }
    }
    env.storage()
        .instance()
        .set(&DataKey::Members, &new_members);

    let old_order: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PayoutOrder)
        .unwrap_or(Vec::new(env));
    let mut new_order: Vec<Address> = Vec::new(env);
    for m in old_order.iter() {
        if m != *member {
            new_order.push_back(m);
        }
    }
    env.storage()
        .instance()
        .set(&DataKey::PayoutOrder, &new_order);

    events::emit_mem_del(env, member.clone());
}

/// Removes `member` from `DataKey::PayoutOrder` and renumbers the following
/// slots, fixing the dangling-entry / payout-gap defect described in #389.
///
/// compaction only runs when the exiting member's payout slot has NOT yet been
/// paid out, i.e. `current_round <= slot_index`. When `current_round > slot_index`
/// the slot has already been visited by an earlier round; renumbering later
/// slots in that case would shift active pays into the wrong rounds, so we
/// leave the layout untouched and rely on the exited-member check inside
/// `complete_round_payout` to skip the vacated slot.
///
/// `current_round` is taken as a parameter so the caller does not have to
/// re-read the storage key (avoids a duplicate `get(&DataKey::CurrentRound)`).
///
/// Returns `(compacted, slot_index, old_len, new_len, skip_reason)`:
/// - `compacted`: `true` only when the vector was rewritten, `false` on every
///   defensive branch.
/// - `slot_index`: 0-based index of the member inside `PayoutOrder` before
///   any compaction. `u32::MAX` when the member was not found (defensive);
///   otherwise in `[0, old_len)`.
/// - `old_len` / `new_len`: lengths before / after; equal when `compacted ==
///   false`.
/// - `skip_reason`: `0` when compaction ran, `1` when the past-slot guard
///   tripped, `2` when the layout was left untouched for a defensive reason
///   (empty `PayoutOrder` or member absent from it).
pub(crate) fn compact_payout_order(
    env: &Env,
    member: &Address,
    current_round: u32,
) -> (bool, u32, u32, u32, u32) {
    let payout_order: Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::PayoutOrder)
        .unwrap_or_else(|| Vec::new(env));
    let old_len = payout_order.len();

    // Defensive: nothing to compact against an empty payout order.
    if old_len == 0 {
        return (false, 0, 0, 0, 2);
    }

    // Locate the member's slot in PayoutOrder. Sentinel when not found so we
    // can distinguish "not found" from "found at index 0".
    let mut slot_index: u32 = u32::MAX;
    for (idx, addr) in payout_order.iter().enumerate() {
        if addr == *member {
            slot_index = idx as u32;
            break;
        }
    }

    if slot_index == u32::MAX {
        // Member absent from the order — no compaction to perform.
        return (false, u32::MAX, old_len, old_len, 2);
    }

    // Skip compaction if the exiting member's slot was already paid out by
    // an earlier round. Renumbering here would shift later slots into the
    // wrong round and cause misdirected payouts.
    if current_round > slot_index {
        return (false, slot_index, old_len, old_len, 1);
    }

    // Rebuild the vector without the exited member — promotes every member
    // after the exited one up by one slot (and one round).
    let mut new_order: Vec<Address> = Vec::new(env);
    for addr in payout_order.iter() {
        if addr != *member {
            new_order.push_back(addr.clone());
        }
    }
    let new_len = new_order.len();

    env.storage().instance().set(&DataKey::PayoutOrder, &new_order);

    (true, slot_index, old_len, new_len, 0)
}

