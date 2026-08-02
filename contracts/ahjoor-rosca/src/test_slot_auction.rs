#![cfg(test)]
use super::*;
use soroban_sdk::token::Client as TokenClient;
use soroban_sdk::token::StellarAssetClient as TokenAdminClient;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Create a test environment with `n` members, each minted `mint_amount` tokens.
fn setup_with_members<'a>(
    n: usize,
    mint_amount: i128,
) -> (
    Env,
    AhjoorContractClient<'a>,
    Address,
    Address,
    TokenClient<'a>,
    TokenAdminClient<'a>,
    soroban_sdk::Vec<Address>,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(AhjoorContract, ());
    let client = AhjoorContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let token_client = TokenClient::new(&env, &token_admin);
    let token_admin_client = TokenAdminClient::new(&env, &token_admin);

    let mut members = soroban_sdk::Vec::new(&env);
    for _ in 0..n {
        let addr = Address::generate(&env);
        if mint_amount > 0 {
            token_admin_client.mint(&addr, &mint_amount);
        }
        members.push_back(addr);
    }

    (env, client, admin, token_admin, token_client, token_admin_client, members)
}

/// Initialise the contract with auction enabled.
fn init_with_auction(
    env: &Env,
    client: &AhjoorContractClient<'_>,
    admin: &Address,
    members: &soroban_sdk::Vec<Address>,
    token: &Address,
    auction_window_ledgers: u64,
) {
    client.init(
        admin,
        members,
        &100,
        token,
        &3600,
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
            auction_enabled: true,
            auction_window_ledgers,
            randomize_payout_order: false,
            reserve_enabled: false,
            reserve_contribution_bps: 0,
        },
        &None,
    );
}

/// Initialise the contract with auction disabled.
fn init_without_auction(
    env: &Env,
    client: &AhjoorContractClient<'_>,
    admin: &Address,
    members: &soroban_sdk::Vec<Address>,
    token: &Address,
) {
    client.init(
        admin,
        members,
        &100,
        token,
        &3600,
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
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Single bidder wins the auction, gets swapped into desired slot,
/// and the winning bid is distributed as a bonus to remaining members.
#[test]
fn test_single_bidder_wins_auction() {
    let (env, client, admin, token_addr, token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    // Mint extra tokens to members for bidding
    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    // auction_window_ledgers = 500 seconds
    init_with_auction(&env, &client, &admin, &members, &token_addr, 500);

    // Complete round 0 so reset_round_state fires and opens auction at cycle start (round 1 = cycle start for 3-member group? No — cycle start is when new_round % len == 0, i.e. round 0 is cycle start)
    // Actually round 0 is the first round. After completing round 0, new_round = 1.
    // Cycle start: new_round % 3 == 0 → round 3, 6, 9...
    // So we need to complete 3 rounds to get to cycle start.
    // Let's complete rounds 0, 1, 2 to trigger cycle start at round 3.

    env.ledger().set_timestamp(100);
    // Round 0: all contribute
    for i in 0..3 {
        client.contribute(&members.get(i).unwrap(), &token_addr, &100);
    }
    // Round 1: all contribute
    env.ledger().set_timestamp(200);
    for i in 0..3 {
        client.contribute(&members.get(i).unwrap(), &token_addr, &100);
    }
    // Round 2: all contribute — this triggers reset to round 3 (cycle start)
    env.ledger().set_timestamp(300);
    for i in 0..3 {
        client.contribute(&members.get(i).unwrap(), &token_addr, &100);
    }

    // Now we're at round 3 (cycle start). Auction should be open until 300 + 500 = 800.
    // Member 2 (slot index 2) wants to move to slot 0.
    let bidder = members.get(2).unwrap();
    let bid_amount: i128 = 300;

    let bidder_balance_before = token_client.balance(&bidder);
    client.place_slot_bid(&bidder, &0, &bid_amount);
    let bidder_balance_after = token_client.balance(&bidder);
    assert_eq!(bidder_balance_before - bidder_balance_after, bid_amount, "Bid amount should be deducted from bidder");

    // Advance past auction window
    env.ledger().set_timestamp(900);

    // Record balances of non-winning members before resolution
    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();
    let bal0_before = token_client.balance(&member0);
    let bal1_before = token_client.balance(&member1);

    client.resolve_slot_auction();

    // Winner (member 2) should now be at slot 0 in payout order
    // Non-winners (member 0, member 1) should each receive bid_amount / 2 = 150
    let bonus = bid_amount / 2; // 2 eligible non-winning members
    let bal0_after = token_client.balance(&member0);
    let bal1_after = token_client.balance(&member1);
    assert_eq!(bal0_after - bal0_before, bonus, "Member 0 should receive bonus");
    assert_eq!(bal1_after - bal1_before, bonus, "Member 1 should receive bonus");
}

/// Multiple bidders: highest bid wins; tie-break by earliest submission.
#[test]
fn test_multi_bidder_highest_wins() {
    let (env, client, admin, token_addr, token_client, token_admin_client, members) =
        setup_with_members(4, 10_000);

    for i in 0..4 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 1000);

    // Complete 4 rounds to reach cycle start (round 4 = 4 % 4 == 0)
    env.ledger().set_timestamp(100);
    for _ in 0..4 {
        for i in 0..4 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    // Auction is now open (round 4, cycle start). Window closes at 500 + 1000 = 1500.
    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();
    let member2 = members.get(2).unwrap();

    // member0 bids 200 at t=510
    env.ledger().set_timestamp(510);
    client.place_slot_bid(&member0, &0, &200);

    // member1 bids 500 at t=520 — highest bid, should win
    env.ledger().set_timestamp(520);
    client.place_slot_bid(&member1, &0, &500);

    // member2 bids 200 at t=530 — same as member0 but later
    env.ledger().set_timestamp(530);
    client.place_slot_bid(&member2, &0, &200);

    // Advance past window
    env.ledger().set_timestamp(1600);

    let bal0_before = token_client.balance(&member0);
    let bal2_before = token_client.balance(&member2);

    client.resolve_slot_auction();

    // member0 and member2: refund (200) + winning_bid / eligible_count (500/3=166 bonus) = 366
    let eligible_count: i128 = 3; // member0, member2, member3 (3 non-winning active members)
    let bonus = 500i128 / eligible_count;
    assert_eq!(token_client.balance(&member0) - bal0_before, 200 + bonus, "Loser member0 should be refunded plus bonus");
    assert_eq!(token_client.balance(&member2) - bal2_before, 200 + bonus, "Loser member2 should be refunded plus bonus");
}

/// Tie-break: two equal bids — earliest submission wins.
#[test]
fn test_tie_break_earliest_submission_wins() {
    let (env, client, admin, token_addr, token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 1000);

    // Complete 3 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();

    // member0 bids 300 at t=410 (earlier)
    env.ledger().set_timestamp(410);
    client.place_slot_bid(&member0, &2, &300);

    // member1 bids 300 at t=420 (later — should lose tie-break)
    env.ledger().set_timestamp(420);
    client.place_slot_bid(&member1, &2, &300);

    // Advance past window
    env.ledger().set_timestamp(1500);

    let bal1_before = token_client.balance(&member1);
    client.resolve_slot_auction();

    // member1: refund (300) + winning_bid / eligible_count (300/2=150 bonus) = 450
    let eligible_count: i128 = 2; // member1 and member2 are eligible non-winning members
    let bonus = 300i128 / eligible_count;
    assert_eq!(
        token_client.balance(&member1) - bal1_before,
        300 + bonus,
        "Later equal bidder should be refunded plus receive bonus"
    );
}

/// No bids placed → resolve_slot_auction is a no-op, order unchanged.
#[test]
fn test_no_bids_no_op() {
    let (env, client, admin, token_addr, _token_client, _token_admin_client, members) =
        setup_with_members(3, 10_000);

    init_with_auction(&env, &client, &admin, &members, &token_addr, 500);

    // Complete 3 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    // Advance past auction window without placing any bids
    env.ledger().set_timestamp(1000);

    // Should succeed without panicking
    client.resolve_slot_auction();
}

/// Bidding after the auction window closes is rejected with AuctionWindowClosed.
#[test]
fn test_post_window_bid_rejected() {
    let (env, client, admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 500);

    // Complete 3 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    // Advance past auction window
    env.ledger().set_timestamp(1000);

    let bidder = members.get(0).unwrap();
    let result = client.try_place_slot_bid(&bidder, &0, &100);
    assert!(result.is_err(), "Bid after window close should be rejected");
}

/// place_slot_bid on a group with auction_enabled = false panics with AuctionNotEnabled.
#[test]
fn test_auction_not_enabled_rejected() {
    let (env, client, admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_without_auction(&env, &client, &admin, &members, &token_addr);

    env.ledger().set_timestamp(100);
    let bidder = members.get(0).unwrap();
    let result = client.try_place_slot_bid(&bidder, &0, &100);
    assert!(result.is_err(), "Bid on non-auction group should be rejected");
}

/// update_slot_bid atomically replaces the previous bid.
#[test]
fn test_update_slot_bid_replaces_previous() {
    let (env, client, admin, token_addr, token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 1000);

    // Complete 3 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    let bidder = members.get(0).unwrap();

    // Place initial bid of 200
    env.ledger().set_timestamp(410);
    client.place_slot_bid(&bidder, &2, &200);
    let bal_after_first_bid = token_client.balance(&bidder);

    // Update bid to 400 — should refund 200 and deduct 400 (net -200 from bal_after_first_bid)
    env.ledger().set_timestamp(420);
    client.update_slot_bid(&bidder, &1, &400);
    let bal_after_update = token_client.balance(&bidder);

    // Net change from bal_after_first_bid: refund 200, deduct 400 → -200
    assert_eq!(
        bal_after_first_bid - bal_after_update,
        200,
        "Update should net-deduct the difference (400 - 200 = 200)"
    );
}

/// update_slot_bid with no existing bid panics with NoBidFound.
#[test]
fn test_update_slot_bid_no_existing_bid_fails() {
    let (env, client, admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 1000);

    // Complete 3 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    env.ledger().set_timestamp(410);
    let bidder = members.get(0).unwrap();
    let result = client.try_update_slot_bid(&bidder, &1, &200);
    assert!(result.is_err(), "update_slot_bid with no existing bid should fail");
}

/// resolve_slot_auction before window closes is rejected.
#[test]
fn test_resolve_before_window_closes_rejected() {
    let (env, client, admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 1000);

    // Complete 3 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    // Try to resolve while window is still open (t=410, window closes at 400+1000=1400)
    env.ledger().set_timestamp(410);
    let result = client.try_resolve_slot_auction();
    assert!(result.is_err(), "Resolving while window is open should fail");
}

/// #644: get_slot_bids returns an empty Vec when no bids have been placed.
#[test]
fn test_get_slot_bids_empty_round() {
    let (env, client, admin, token_addr, _token_client, _token_admin_client, members) =
        setup_with_members(3, 10_000);

    init_with_auction(&env, &client, &admin, &members, &token_addr, 500);

    // Complete 3 rounds to reach cycle start (auction opens)
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    // No bids placed — get_slot_bids should return empty Vec
    let bids = client.get_slot_bids();
    assert_eq!(bids.len(), 0, "Empty round should return empty bids");
}

/// #644: get_slot_bids returns all bids placed during the auction window.
#[test]
fn test_get_slot_bids_multiple_bids() {
    let (env, client, admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(4, 10_000);

    for i in 0..4 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 1000);

    // Complete 4 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..4 {
        for i in 0..4 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    // Auction is now open. Place multiple bids.
    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();
    let member2 = members.get(2).unwrap();

    env.ledger().set_timestamp(510);
    client.place_slot_bid(&member0, &0, &200);

    env.ledger().set_timestamp(520);
    client.place_slot_bid(&member1, &1, &350);

    env.ledger().set_timestamp(530);
    client.place_slot_bid(&member2, &2, &150);

    // get_slot_bids should return all 3 bids
    let bids = client.get_slot_bids();
    assert_eq!(bids.len(), 3, "Should have 3 bids");

    // Verify bid details
    let bid0 = bids.get(0).unwrap();
    assert_eq!(bid0.bidder, member0, "First bid should be from member0");
    assert_eq!(bid0.desired_slot, 0u32, "First bid desired_slot should be 0");
    assert_eq!(bid0.amount, 200i128, "First bid amount should be 200");

    let bid1 = bids.get(1).unwrap();
    assert_eq!(bid1.bidder, member1, "Second bid should be from member1");
    assert_eq!(bid1.desired_slot, 1u32, "Second bid desired_slot should be 1");
    assert_eq!(bid1.amount, 350i128, "Second bid amount should be 350");

    let bid2 = bids.get(2).unwrap();
    assert_eq!(bid2.bidder, member2, "Third bid should be from member2");
    assert_eq!(bid2.desired_slot, 2u32, "Third bid desired_slot should be 2");
    assert_eq!(bid2.amount, 150i128, "Third bid amount should be 150");
}

/// #644: get_slot_bids reflects updated bids correctly.
#[test]
fn test_get_slot_bids_after_update() {
    let (env, client, admin, token_addr, _token_client, token_admin_client, members) =
        setup_with_members(3, 10_000);

    for i in 0..3 {
        token_admin_client.mint(&members.get(i).unwrap(), &5_000);
    }

    init_with_auction(&env, &client, &admin, &members, &token_addr, 1000);

    // Complete 3 rounds to reach cycle start
    env.ledger().set_timestamp(100);
    for _ in 0..3 {
        for i in 0..3 {
            client.contribute(&members.get(i).unwrap(), &token_addr, &100);
        }
        env.ledger().set_timestamp(env.ledger().timestamp() + 100);
    }

    let member0 = members.get(0).unwrap();
    let member1 = members.get(1).unwrap();

    // Place two initial bids
    env.ledger().set_timestamp(410);
    client.place_slot_bid(&member0, &1, &200);
    client.place_slot_bid(&member1, &2, &300);

    // get_slot_bids should show 2 bids
    let bids_before = client.get_slot_bids();
    assert_eq!(bids_before.len(), 2, "Should have 2 bids before update");

    // member0 updates bid to 500
    env.ledger().set_timestamp(420);
    client.update_slot_bid(&member0, &1, &500);

    // get_slot_bids should still show 2 bids, but member0's amount should be updated to 500
    let bids_after = client.get_slot_bids();
    assert_eq!(bids_after.len(), 2, "Should still have 2 bids after update");

    // Find member0's updated bid
    let mut found_updated_bid = false;
    for bid in bids_after.iter() {
        if bid.bidder == member0 {
            assert_eq!(bid.amount, 500i128, "member0's bid should be updated to 500");
            found_updated_bid = true;
            break;
        }
    }
    assert!(found_updated_bid, "member0's updated bid should be in get_slot_bids");
}
