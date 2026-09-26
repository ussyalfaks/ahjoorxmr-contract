# ROSCA Governance Quorum Configuration

This document describes how quorum requirements are configured and evaluated for
governance votes in the `ahjoor-rosca` contract.

## Overview

Governance proposals in `ahjoor-rosca` are decided by a vote among the ROSCA's
active members. A proposal only passes when the share of participating votes
meets or exceeds the quorum requirement that applies to that proposal type.

Quorum is configured at two levels:

- A **default quorum percentage** that applies to every proposal type unless
  overridden.
- **Per-proposal-type overrides** that let the group require a different quorum
  for specific kinds of proposals.

The relevant contract entry points live in
`contracts/ahjoor-rosca/src/lib.rs`:

- `set_quorum_per_type` — set (or update) the quorum percentage for a specific
  proposal type.
- `get_quorum_percentage` — read the default quorum percentage.
- `get_quorum_for_type` — read the effective quorum percentage for a given
  proposal type, taking overrides into account.

## Default quorum vs. per-type overrides

The contract maintains a default quorum percentage that is used whenever no
override exists for a proposal type. This default is what `get_quorum_percentage`
returns.

When a group wants a stricter or looser threshold for a particular kind of
proposal, it calls `set_quorum_per_type` with the proposal type and the desired
percentage. That value is stored as an override for that type only; it does not
change the default and does not affect other proposal types.

To determine the quorum that actually applies to a proposal, use
`get_quorum_for_type`. It resolves the effective value as follows:

1. If an override has been set for the proposal type via
   `set_quorum_per_type`, that override is returned.
2. Otherwise, the default quorum percentage (as returned by
   `get_quorum_percentage`) is returned.

This means the default acts as a fallback: per-type overrides always take
precedence for the types they target, while every other type continues to use the
default.

## How quorum is evaluated against active membership

Quorum is measured against the set of **active members** of the ROSCA at the time
the vote is evaluated. Members who are not active do not count toward the quorum
denominator.

For a proposal of a given type, the contract:

1. Resolves the effective quorum percentage with `get_quorum_for_type`.
2. Counts the votes cast by active members.
3. Compares the participating share against the effective quorum percentage.

The proposal satisfies quorum only when the participating share meets or exceeds
the effective percentage. Because the denominator is the active membership, a
change in active membership changes the number of votes needed to reach the same
percentage.

## Example

Suppose the default quorum percentage is 50% and the group sets a 66% override
for a specific proposal type:

- A proposal of the overridden type requires 66% of active members to
  participate.
- A proposal of any other type requires the default 50% of active members to
  participate.

Calling `get_quorum_for_type` for the overridden type returns 66%, while calling
it for any other type returns 50% (the default from `get_quorum_percentage`).

## Related tests

The quorum configuration and evaluation behavior is covered by
`contracts/ahjoor-rosca/src/test_quorum.rs`.
