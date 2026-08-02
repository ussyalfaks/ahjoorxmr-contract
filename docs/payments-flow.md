# Payments Authorization and Capture Flow

This document describes the two-step authorization and capture lifecycle used by
the `ahjoor-payments` contract.

The flow is designed for merchants that need to reserve customer funds first and
settle them later. Authorization moves the funds into contract escrow, capture
settles the payment, and a missed capture window lets the authorization expire.

## Status Transition Diagram

```text
Pending -> Authorized -> Captured  (settled)
                     -> Expired   (missed capture window)
                     -> Disputed
```

In contract storage, a successful capture is finalized as
`PaymentStatus::Completed` and emits `PaymentCaptured`. Integrators may display
that terminal settlement step as "Captured" in user-facing flows.

## Lifecycle

### 1. Authorize

Call:

```text
authorize_payment(merchant, customer, token, amount, capture_deadline_ledger)
```

The merchant/payee signs the contract call. The customer/payer must have already
approved the payment contract to transfer the authorized amount of `token`.

When authorization succeeds:

- `amount` is transferred from the customer into the payment contract.
- A new `Payment` is stored with `status = PaymentStatus::Authorized`.
- `Payment.capture_deadline` is set to `capture_deadline_ledger`.
- `PaymentAuthorized` is emitted.

`capture_deadline_ledger` must be greater than the current ledger sequence. If
it is not in the future, authorization fails.

### 2. Capture

Call:

```text
capture_payment(merchant, payment_id)
```

The merchant/payee signs the capture call. Capture is only valid while the
payment is still `Authorized` and the current ledger sequence is less than or
equal to `Payment.capture_deadline`.

When capture succeeds:

- The payment is finalized and funds settle to the merchant after contract fee
  logic.
- The stored payment reaches the settled terminal state
  `PaymentStatus::Completed`.
- `PaymentCaptured` is emitted with the captured amount.

If capture is attempted after the capture deadline, the contract raises
`CapturePastDeadline`.

### 3. Missed Capture

Call:

```text
expire_payment(payment_id)
```

If an authorized payment is not captured before its capture window closes, it
can be expired. This function is callable by anyone once the current ledger
sequence is greater than `Payment.capture_deadline`.

When expiry succeeds:

- The escrowed amount is transferred back to the customer/payer.
- The stored payment moves to `PaymentStatus::Expired`.
- `PaymentExpired` and `PaymentStatusChanged` are emitted.

### 4. Dispute During Authorization

Call:

```text
dispute_payment(customer, payment_id, reason)
```

An authorized payment can be disputed before it is captured or expired. The
current contract entrypoint requires the customer/payer to sign the dispute
call.

When dispute succeeds:

- The payment moves from `PaymentStatus::Authorized` to
  `PaymentStatus::Disputed`.
- A temporary dispute record is stored with the dispute reason.
- `PaymentDisputed` and `PaymentStatusChanged` are emitted.

Disputed payments leave the normal capture path and must be resolved through the
contract's dispute resolution functions.

## Payment Struct Field

`Payment.capture_deadline` stores the ledger sequence after which an authorized
payment can no longer be captured.

- `0` means the payment is not using the authorization/capture path.
- A non-zero value is set by `authorize_payment`.
- `capture_payment` rejects captures when the current ledger sequence is greater
  than this value.
- `expire_payment` uses this value to decide when an authorized payment can be
  expired.

## Events

### PaymentAuthorized

Emitted when funds are reserved in escrow by `authorize_payment`.

Fields:

| Field | Meaning |
| --- | --- |
| `payment_id` | ID of the newly authorized payment. |
| `customer` | Customer/payer whose funds were moved into escrow. |
| `merchant` | Merchant/payee that can later capture the payment. |
| `amount` | Authorized token amount. |
| `capture_deadline_ledger` | Ledger sequence by which capture must happen. |

### PaymentCaptured

Emitted when an authorized payment is successfully captured and settled.

Fields:

| Field | Meaning |
| --- | --- |
| `payment_id` | ID of the captured payment. |
| `amount` | Gross authorized amount captured before final settlement accounting. |

## Integration Notes

- Ask the customer to approve the payment contract for the token amount before
  calling `authorize_payment`.
- Surface the capture deadline in the UI as a ledger-based countdown, not a wall
  clock timestamp.
- Disable capture actions once the current ledger sequence is greater than
  `Payment.capture_deadline`.
- Offer an expiry action after the deadline so users can release missed
  authorizations back to the payer.
- Treat `PaymentCaptured` plus `PaymentStatus::Completed` as the settled state.

## Buyer Trust Tiers

The `ahjoor-payments` contract enforces spending limits based on a buyer's trust tier. This lets merchants extend higher limits to buyers with an established payment history while restricting new or unverified buyers.

### Tier Levels

`BuyerTrustTierLevel` currently defines:

- `New` — the default tier for a buyer with no tier explicitly set.
- `Trusted` — a higher tier with a higher spending limit.

### Setting Tier Limits

A merchant configures the spending limit for each tier with:

`set_tier_spending_limit(merchant, tier, limit, period_seconds)`

- `merchant` must sign the call.
- `limit` is the maximum total amount a buyer at that tier can spend within a rolling window of `period_seconds`.
- Each tier has its own independent limit — setting the `New` tier limit does not affect the `Trusted` tier limit, and vice versa.

### Assigning a Buyer's Tier

A merchant assigns or updates a buyer's tier with:

`set_buyer_tier(merchant, buyer, tier)`

- `merchant` must sign the call.
- A buyer's tier is not automatic — it must be explicitly set by the merchant. Until set, a buyer defaults to the `New` tier.

### Effect on Payments

When a buyer attempts a payment via `create_payment` / `try_complete_payment`:

- The contract checks the buyer's current tier and that tier's configured spending limit.
- If completing the payment would exceed the buyer's tier limit for the current period, the payment call fails.
- If the payment is within the buyer's tier limit, it proceeds normally through the standard authorize/capture flow described above.

This means the same payment amount can succeed for a `Trusted` buyer and fail for a `New` buyer if it exceeds the `New` tier's configured limit.

### Tier Changes Over Time

A buyer's tier is not automatically upgraded by the contract based on payment history. Tier changes happen only when the merchant explicitly calls `set_buyer_tier` again with a new tier value. Merchants are responsible for deciding when a buyer has earned a tier upgrade (or should be downgraded) and applying it via this call.

## Merchant Collateral

The `ahjoor-payments` contract supports a merchant collateral mechanism to protect customers and incentivize honest behavior. Merchants deposit collateral in the configured settlement token (typically USDC), which acts as a security deposit that can be slashed if disputes are resolved in favor of the customer.

### When Collateral is Required and How Much

1. **Merchant Approval Gate**: When merchant open mode is disabled (`set_merchant_open_mode(false)`), a merchant must be approved (`approve_merchant`) before they can accept payments. Approval requires the merchant to have deposited at least the minimum collateral threshold.
2. **Minimum Collateral**: The default minimum collateral required is `1_000_000` (representing 1 USDC, assuming 7 decimal places). The admin can adjust this limit dynamically by calling `set_min_collateral(amount)`.

### How and When Collateral is Released

- **Withdrawal**: A merchant can withdraw their deposited collateral at any time using `withdraw_collateral(merchant, amount)`.
- **Minimum Balance Enforcement**: The contract rejects any withdrawal that would cause the merchant's remaining collateral balance to fall below the minimum required collateral (`min_collateral`). To withdraw all collateral, the merchant must be unapproved or the minimum collateral limit must be set to `0`.

### Conditions Under Which Collateral is Forfeited (Slashed)

- **Dispute Resolution in Customer's Favor**: When a customer disputes a payment (`dispute_payment`) and the dispute is resolved in the customer's favor (`resolve_dispute(payment_id, false)`):
  - The customer is refunded the disputed payment amount.
  - The merchant's collateral is slashed by the equivalent payment amount.
- **Slashing Cap**: The slashed amount is capped at the merchant's current collateral balance. If the dispute amount is greater than the available collateral, the collateral balance is reduced to `0`.
- **Token Specificity**: Collateral slashing only occurs for payments processed using the configured collateral/settlement token (e.g., USDC). Disputes resolved on payments made in non-settlement/other whitelisted tokens do not result in a slash of the merchant's collateral.