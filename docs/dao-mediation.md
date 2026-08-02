# DAO Mediation for Disputed Payments

This document explains the on-chain DAO mediation path used by the payments contract when a disputed payment needs an impartial resolution.

## When a dispute is escalated

A payment enters the DAO mediation flow only after the payment has already been marked as disputed.

The escalation step is initiated by either:

- the payment customer, or
- the contract admin.

The contract requires the payment to be in the `Disputed` state before the case can be opened. If the payment is still pending, authorized, completed, or already refunded, escalation fails.

Once escalation succeeds:

- a new mediation case is created for the disputed payment,
- the case receives a unique case ID,
- the vote window starts immediately, and
- the case remains pending until the window closes or the verdict is executed.

## Who can participate

DAO mediation is enabled by the admin through contract configuration.

The admin sets:

- the list of DAO member addresses,
- the vote window duration in seconds, and
- the minimum number of votes required before a verdict can be executed.

Only registered DAO members may vote. Each DAO member may cast one vote per mediation case, and each vote is either:

- for the merchant, or
- for the customer.

## How a resolution is reached

A mediation case remains open until the configured vote window closes.

The resolution rules are:

1. The vote window must expire before the verdict can be executed.
2. A minimum number of votes must have been cast.
3. The side with more votes wins.
4. If the vote count is tied, the customer side wins by default.

This means a tied outcome results in a refund to the customer rather than settlement to the merchant.

## What happens to funds during mediation

While the mediation case is active, the disputed payment stays in its existing escrowed/dispute state. No final settlement is applied until the DAO verdict is executed.

After the vote window closes and the verdict is executed:

- If the merchant wins, the payment is finalized as completed and the funds are settled to the merchant.
- If the customer wins, the contract refunds the outstanding escrowed amount back to the customer and marks the payment as refunded.

The contract also clears the temporary dispute record once the verdict is executed.

## Practical summary

In short, the DAO mediation process is a structured dispute resolution path for disputed payments:

- the payment is escalated from a dispute into a DAO case,
- registered DAO members vote on the outcome,
- the vote window closes,
- the verdict is executed,
- and funds are either settled to the merchant or refunded to the customer depending on the result.
