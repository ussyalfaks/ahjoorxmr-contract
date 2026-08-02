# Emergency Reserve (ahjoor-rosca)

This document describes the ROSCA emergency reserve feature implemented in `contracts/ahjoor-rosca`.

## How the reserve is funded

The reserve is optional and is enabled through the ROSCA configuration (`RoscaConfig`) with:

- `reserve_enabled: true`
- `reserve_contribution_bps`: a surcharge applied to each contribution routed into the reserve

When the reserve is enabled, a portion of each contribution is diverted into the group reserve instead of being treated as ordinary round funds. The reserve balance is tracked on-chain in `EmergencyReserveBalance`.

## When members can draw from the reserve

A member may request an emergency loan from the reserve by calling `request_emergency_loan(amount, repayment_window_ledgers)`.

The draw is allowed only when all of the following are true:

- the reserve feature is enabled for the group
- the member does not already have an outstanding reserve loan
- the requested amount is positive
- the requested amount does not exceed the maximum allowed fraction of the reserve balance (currently capped at 50% of the reserve)
- the reserve balance is large enough to cover the loan amount

If the reserve balance is insufficient, the loan request fails.

## Who authorizes a draw and how it is repaid

A draw is authorized by the member who requests the emergency loan. The contract does not require an admin or group vote for the initial loan request; the borrower receives the funds directly from the reserve once the request is accepted.

Repayment is also member-driven. The borrower can repay the loan by calling `repay_emergency_loan(loan_id, amount)`. Repayments are transferred back to the contract and increase the reserve balance. The loan record tracks:

- the original loan amount
- the amount already repaid
- the repayment deadline ledger
- whether the loan has defaulted

A partial or full repayment is allowed until the loan balance is fully settled. Once fully repaid, the member’s outstanding-loan marker is cleared.

## Operational notes

- The reserve is intended as a liquidity buffer for members facing short-term cash-flow needs.
- Loans are limited to a capped fraction of the reserve balance to avoid draining the reserve too aggressively.
- The reserve balance is replenished as members repay emergency loans.
