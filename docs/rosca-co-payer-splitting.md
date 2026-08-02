# Co-Payer Contribution Splitting in ROSCA

This document explains how a `ahjoor-rosca` member can split their per-round
contribution obligation across one or more co-payers, so that other addresses
cover part (or all) of the member's slot payment on their behalf.

---

## 1. How Co-Payers Are Registered for a Member's Slot

A member registers their own co-payer split by calling:

```rust
register_co_payer_splits(member: Address, splits: Vec<CoPayerSplit>)
```

```rust
pub struct CoPayerSplit {
    pub co_payer: Address,
    pub amount: i128,
}
```

- **Authentication:** The transaction must be signed and authorized by the
  `member` themselves (`require_auth()`). A co-payer cannot register a split
  on behalf of a member.
- **Membership Check:** `member` must be an active, registered member of the
  ROSCA group (not previously exited).
- **One Active Split Per Member:** A member may only have one registered
  split at a time. Calling `register_co_payer_splits` while a split is
  already on file panics with `CopayerSplitsAlreadySet`; the member must
  call `revoke_co_payer_splits` first before registering a new one.
- **Amount Validation:** Every `CoPayerSplit.amount` must be strictly
  positive (`Error::AmountMustBePositive`), and the sum of all split amounts
  must exactly equal the member's required contribution for the round
  (`base_amount × tier_bps / 10_000`, i.e. the member's tier-adjusted
  contribution). Any mismatch panics with `CopayerAmountsMismatch`.
- On success, the split list is stored under `DataKey3::CoPayerSplits(member)`
  and a `CoPayerSplitRegistered` event is emitted with the co-payer count and
  total split amount.

Registered splits can be read back at any time with:

```rust
get_co_payer_splits(member: Address) -> Vec<CoPayerSplit>
```

which returns an empty vector if no split is registered.

## 2. How the Split Ratio Is Set and Enforced

The "ratio" between co-payers is simply the set of exact token amounts each
`CoPayerSplit` specifies — there is no implicit percentage; the member
chooses the exact amount each co-payer will contribute when the split is
registered. Enforcement happens at two points:

1. **Registration time:** the amounts must sum to exactly the member's
   required contribution (see above), so the split always fully covers the
   member's obligation for the round.
2. **Contribution time:** the member (or anyone triggering the flow on the
   member's behalf) calls:

   ```rust
   contribute_split(member: Address, token: Address)
   ```

   - Requires `member.require_auth()`.
   - Panics with `GroupNotYetActive` if the group has not reached its
     configured start time yet, `NotAMember`/`MemberHasExited` if `member`
     is not an active member, and `TokenNotApproved` if `token` is not on
     the group's approved-token list — the same guards the standard
     contribution flow enforces.
   - Panics with `NoCopayersRegistered` if no split exists for `member`.
   - Panics with `Error::AlreadyContributed` if the member already paid for
     the current round.
   - For each `CoPayerSplit` in the registered list, the contract calls
     `token_client.transfer(&split.co_payer, &contract, &split.amount)` —
     tokens move **directly from each co-payer to the contract**. Each
     co-payer must have pre-approved (via `approve`) the contract to transfer
     at least `split.amount` of the token before this call.
   - Once all transfers succeed, `member` is marked as paid for the round,
     the sum of transfers is recorded as the member's contribution amount,
     and a `CoPayerContributed` event is emitted per co-payer plus a regular
     contribution event for the member.

A member can remove their registered split entirely with:

```rust
revoke_co_payer_splits(member: Address)
```

which requires the member's own authorization, panics with
`NoCopayersRegistered` if nothing is registered, and emits
`CoPayerSplitRevoked`.

## 3. What Happens If One Co-Payer Fails to Contribute

`contribute_split` transfers from every co-payer in the split list within a
single contract invocation. Soroban token transfers panic if the source
account has not approved a sufficient allowance (or lacks sufficient
balance). Because all state changes in one contract call are atomic:

- If **any** co-payer's transfer fails (missing/insufficient allowance,
  insufficient balance, etc.), the entire `contribute_split` call reverts.
- **No partial contribution is recorded** — the member is not marked as
  paid, no funds move from the co-payers who *would* have succeeded, and no
  events are emitted for that call.
- The member remains unpaid for the round until `contribute_split` is
  retried successfully (e.g. after the failing co-payer grants/tops up their
  allowance), or the member falls back to paying directly via the standard
  contribution flow, or revokes the split and re-registers a different one.

This "all co-payers succeed or none do" semantics avoids a co-payer being
charged for a contribution that ultimately doesn't cover the member's full
obligation.
