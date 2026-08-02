# Cosigner Guarantee in ROSCA

This document explains the co-signer guarantee mechanism in the ROSCA contract. The feature lets a member nominate a trusted co-signer to cover a missed contribution when the member defaults.

## How a co-signer is attached to a member

A member can designate a co-signer by calling `set_co_signer(member, group_id, co_signer)`.

The designation is created in a pending state. The nominated co-signer must then accept the assignment by calling `accept_co_signer(co_signer, group_id, member)`.

Once accepted, the guarantee becomes active. If the co-signer never accepts the designation, the guarantee does not become active and the member is treated as a normal defaulter.

## What the co-signer is liable for

If a member misses a contribution and is marked as a defaulter, the contract checks whether that member has an active co-signer guarantee.

When an active co-signer guarantee exists, the contract opens a short grace window for that member instead of applying the default penalty immediately. During that window, the co-signer may contribute on the member's behalf using `co_signer_contribute(...)`.

If the co-signer makes the contribution in time, the member's missed contribution is covered and the member is not penalized for that round.

If the co-signer does not act before the window expires, the member is treated as a defaulter and the default count is increased.

## How the guarantee is discharged

The guarantee is discharged in one of the following ways:

- The co-signer successfully contributes on the member's behalf during the open grace window.
- The member removes the designation between rounds by calling `remove_co_signer(member, group_id)`.
- The guarantee is never accepted, in which case it remains inactive and has no effect.

A successful co-signer contribution clears the temporary window state for that member. A removed designation clears the stored co-signer record entirely.

## Practical summary

The co-signer guarantee is a fallback mechanism for missed ROSCA contributions:

- a member nominates a co-signer,
- the co-signer accepts the guarantee,
- the contract opens a short window after a default,
- the co-signer can cover the missed contribution,
- and if that does not happen in time, the member still incurs the normal default handling.
