# Merchant KYB (Know Your Business) Verification

This document describes the on-chain KYB verification system in the
`ahjoor-payments` contract. KYB gates payment creation: merchants without valid
KYB cannot accept new payments when enforcement is enabled.

## Overview

KYB verification is an admin-managed, off-chain-verified, on-chain-attested
compliance gate. The contract stores only a SHA-256 hash of the off-chain KYB
documents, a jurisdiction code, and an expiry ledger. No raw documents are
stored on-chain.

KYB enforcement is a **global toggle**. When disabled (the default), all
merchants can accept payments regardless of KYB status.

## Status Transition Diagram

```text
                 set_merchant_kyb()
  (no record) ──────────────────────> Active
                 renew_merchant_kyb()
  Expired ───────────────────────────> Active
  Active  ───── revoke_merchant_kyb() ──> Revoked
  Active  ───── ledger > expiry ────────> Expired
```

## How a Merchant Gets KYB Verification

KYB verification is entirely admin-driven. Merchants cannot self-serve.

### 1. Enable Enforcement (one-time)

The contract admin enables global KYB enforcement:

```text
set_kyb_enforcement(admin, enabled: true)
```

This is stored in instance storage and applies to all merchants. When
`enabled` is `false` (the default), KYB checks are skipped entirely.

### 2. Off-Chain Document Review

The admin reviews the merchant's business documents off-chain (registration,
jurisdiction, beneficial ownership, etc.). This step is not performed by the
contract.

### 3. Store Verification On-Chain

Once satisfied, the admin records the verification:

```text
set_merchant_kyb(admin, merchant, kyb_hash, expiry_ledger, jurisdiction)
```

| Parameter | Description |
| --------- | ----------- |
| `admin` | Contract admin; must sign and match stored admin address. |
| `merchant` | The merchant address being verified. |
| `kyb_hash` | SHA-256 hash of the off-chain KYB documents. |
| `expiry_ledger` | Stellar ledger sequence after which KYB expires. |
| `jurisdiction` | Two-letter jurisdiction code (e.g., `"NG"`, `"GH"`). |

This creates a `MerchantKYB` record in persistent storage and emits a
`MerchantKYBSet` event.

## What Payment Operations Are Blocked Without KYB

KYB enforcement is checked **only at payment creation time**. The check runs
inside `create_payment_with_expiry`, which is the core path for all payment
creation variants:

| Function | KYB enforced? |
| -------- | ------------- |
| `create_payment` | Yes |
| `create_payment_with_invoice` | Yes |
| `create_payment_with_options` | Yes |
| `create_payment_with_expiry` | Yes (direct check) |

All other operations are **not** gated by KYB:

- `complete_payment` / `settle_payment`
- `expire_payment`
- `dispute_payment` / `resolve_dispute`
- `authorize_payment` / `capture_payment`
- Subscription operations
- Referral / loyalty / withdrawal operations
- Admin functions

When enforcement is enabled and a merchant lacks valid KYB, the contract
panics with one of two errors:

| Error | Code | Condition |
| ----- | ---- | --------- |
| `KYBVerificationRequired` | 33 | No KYB record exists, or record was revoked. |
| `MerchantKYBExpired` | 72 | KYB record exists but `current_ledger > expiry_ledger`. |

## How Verification Status Is Checked On-Chain

### Internal Enforcement Check

When `create_payment_with_expiry` is called and enforcement is enabled, the
contract performs three sequential checks:

1. **No record** — Looks up `DataKey2::MerchantKYB(merchant)` in persistent
   storage. If no record exists, panics with `KYBVerificationRequired`.
2. **Revoked** — If the record exists and `kyb.revoked == true`, panics with
   `KYBVerificationRequired`.
3. **Expired** — If the current ledger sequence exceeds `kyb.expiry_ledger`,
   panics with `MerchantKYBExpired`.

If all three checks pass, payment creation proceeds.

### Read-Only Status Query

Any caller can query a merchant's KYB status without authentication:

```text
get_merchant_kyb_status(merchant) -> KYBStatus
```

The returned `KYBStatus` struct:

| Field | Type | Description |
| ----- | ---- | ----------- |
| `verified` | `bool` | `true` if the record exists, is not revoked, and has not expired. |
| `expiry_ledger` | `u64` | The ledger sequence when KYB expires. `0` if no record. |
| `jurisdiction` | `String` | Jurisdiction code. Empty string if no record. |

The `verified` field is computed on-read: `!revoked && current_ledger <= expiry_ledger`.

## Renewal and Revocation

### Renewing KYB

When a merchant's KYB is about to expire (or has expired), the admin renews it:

```text
renew_merchant_kyb(admin, merchant, new_kyb_hash, new_expiry_ledger, jurisdiction)
```

This overwrites the existing `MerchantKYB` record with new values and sets
`revoked = false`. A `MerchantKYBSet` event is emitted. Once renewed, payment
creation succeeds again.

### Revoking KYB

The admin can revoke a merchant's KYB without deleting the record:

```text
revoke_merchant_kyb(admin, merchant)
```

This sets `kyb.revoked = true` on the existing record and emits a
`MerchantKYBRevoked` event. The record is preserved for audit purposes.

## Storage

| Key | Type | Content |
| ---- | ---- | ------- |
| `DataKey2::MerchantKYB(Address)` | Persistent | `MerchantKYB { kyb_hash, expiry_ledger, jurisdiction, revoked }` |
| `DataKey3::KYBRequired` | Instance | `bool` — global enforcement flag (primary key) |
| `DataKey2::KYBEnforcementEnabled` | Instance | `bool` — legacy enforcement flag (kept in sync) |

The `is_kyb_enforcement_enabled` function checks `KYBRequired` first, then
falls back to the legacy `KYBEnforcementEnabled` key. Both default to `false`.

## Events

| Event | Fields | Emitted When |
| ----- | ------ | ------------ |
| `MerchantKYBSet` | `merchant`, `kyb_hash`, `expiry_ledger`, `jurisdiction` | `set_merchant_kyb` or `renew_merchant_kyb` succeeds. |
| `MerchantKYBRevoked` | `merchant` | `revoke_merchant_kyb` succeeds. |

## Error Reference

| Error | Code | Description |
| ----- | ---- | ----------- |
| `KYBVerificationRequired` | 33 | KYB verification required but merchant not verified. |
| `MerchantKYBExpired` | 72 | Merchant has KYB on record, but it is now expired. |

## Example Flow

```text
1. Admin calls set_kyb_enforcement(admin, true)
   -> Global enforcement enabled.

2. Admin reviews merchant documents off-chain.

3. Admin calls set_merchant_kyb(admin, merchant, hash, ledger+10000, "GH")
   -> MerchantKYB record created. MerchantKYBSet emitted.

4. Customer calls create_payment(customer, merchant, amount, token, ...)
   -> KYB check passes. Payment created.

5. 10,000 ledgers pass. KYB expires.

6. Customer calls create_payment(customer, merchant, amount, token, ...)
   -> Panics with MerchantKYBExpired (code 72).

7. Admin calls renew_merchant_kyb(admin, merchant, new_hash, ledger+10000, "GH")
   -> Record renewed. MerchantKYBSet emitted.

8. Customer calls create_payment(customer, merchant, amount, token, ...)
   -> KYB check passes again. Payment created.
```
