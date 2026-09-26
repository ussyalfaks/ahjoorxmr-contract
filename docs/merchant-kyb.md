# Merchant KYB Verification

> **Status:** Implemented in `contracts/ahjoor-payments/src/lib.rs`
> (`set_merchant_kyb`, `set_kyb_enforcement`, `renew_merchant_kyb`,
> `revoke_merchant_kyb`, `get_merchant_kyb_status`)
> and tested in `contracts/ahjoor-payments/src/test_kyb.rs`.

---

## Overview

`ahjoor-payments` supports optional **Know-Your-Business (KYB)** verification
for merchants. When enforcement is enabled, every `create_payment` call checks
that the target merchant has a current, non-revoked KYB record on-chain before
the payment is accepted. Merchants without valid KYB records are blocked from
receiving payments until the admin sets or renews their record.

KYB records store a hash of off-chain compliance documents together with an
expiry ledger number and a jurisdiction code, giving integrators a minimal
on-chain compliance anchor without storing sensitive business data directly.

| Fact | Value |
| --- | --- |
| Enforcement toggle | `set_kyb_enforcement(admin, enabled)` |
| Record creation | `set_merchant_kyb(admin, merchant, kyb_hash, expiry_ledger, jurisdiction)` |
| Record renewal | `renew_merchant_kyb(admin, merchant, new_kyb_hash, new_expiry_ledger, jurisdiction)` |
| Revocation | `revoke_merchant_kyb(admin, merchant)` |
| Status query | `get_merchant_kyb_status(merchant) → KYBStatus` |
| Storage key | `DataKey2::MerchantKYB(merchant)` |

---

## KYB Data Structures

```rust
pub struct MerchantKYB {
    pub kyb_hash: BytesN<32>,     // Hash of off-chain compliance documents
    pub expiry_ledger: u64,       // Ledger sequence number after which the record is expired
    pub jurisdiction: String,     // ISO country code or jurisdiction identifier
    pub revoked: bool,            // True if admin has explicitly revoked this record
}

pub struct KYBStatus {
    pub verified: bool,           // True if record exists, is not revoked, and not expired
    pub expiry_ledger: u64,       // Stored expiry ledger (0 if no record exists)
    pub jurisdiction: String,     // Stored jurisdiction (empty if no record exists)
}
```

`verified = true` only when **all three** conditions hold:
1. A record exists for the merchant.
2. `revoked == false`.
3. `current_ledger_sequence <= expiry_ledger`.

---

## Setting KYB Enforcement

```rust
pub fn set_kyb_enforcement(env: Env, admin: Address, enabled: bool)
```

**Authentication:** `admin.require_auth()` — caller must match the stored admin.

When `enabled = true`, every `create_payment` call verifies that:
1. A `MerchantKYB` record exists for the target merchant.
2. The record is not revoked.
3. The record has not expired (`current_ledger <= expiry_ledger`).

If any condition fails, `create_payment` returns an error:
- No record → `Error::KYBVerificationRequired`
- Record expired → `ExtError::MerchantKYBExpired`

When `enabled = false` (default), KYB checks are skipped entirely and
merchants without records can still receive payments.

The flag is stored in two instance-storage keys for backward compatibility:
`DataKey3::KYBRequired` and `DataKey2::KYBEnforcementEnabled`.

---

## Setting a Merchant's KYB Record

```rust
pub fn set_merchant_kyb(
    env: Env,
    admin: Address,
    merchant: Address,
    kyb_hash: BytesN<32>,
    expiry_ledger: u64,
    jurisdiction: String,
)
```

**Authentication:** `admin.require_auth()` — only admin.

Creates (or overwrites) the KYB record for `merchant` with:
- `kyb_hash`: a 32-byte commitment to the off-chain compliance documents.
- `expiry_ledger`: the ledger sequence number at which the record expires.
  Expiry is checked against `env.ledger().sequence()` at payment time.
- `jurisdiction`: a free-form string, typically an ISO 3166-1 alpha-2 country
  code (e.g. `"NG"`, `"GH"`).

The record is stored with persistent TTL extension and `revoked = false`.
Emits `MerchantKYBSet`.

---

## Renewing a Merchant's KYB Record

```rust
pub fn renew_merchant_kyb(
    env: Env,
    admin: Address,
    merchant: Address,
    new_kyb_hash: BytesN<32>,
    new_expiry_ledger: u64,
    jurisdiction: String,
)
```

**Authentication:** Admin only.

Overwrites the existing KYB record with a new hash and a new expiry ledger,
and resets `revoked = false`. This is identical to `set_merchant_kyb` in
effect — it is provided as a semantically distinct entry point to make renewal
flows clear in audit logs.

Typical renewal flow:
1. Admin receives updated compliance documents from the merchant.
2. Admin computes `new_kyb_hash = sha256(documents)`.
3. Admin calls `renew_merchant_kyb` with the new hash and a future
   `new_expiry_ledger`.
4. Merchant can immediately accept new payments.

Emits `MerchantKYBSet` (same event as initial registration).

---

## Revoking a Merchant's KYB Record

```rust
pub fn revoke_merchant_kyb(env: Env, admin: Address, merchant: Address)
```

**Authentication:** Admin only.

Sets `MerchantKYB.revoked = true` for the merchant. The record remains in
storage (expiry and hash are preserved) but `get_merchant_kyb_status` will
return `verified = false` and new payment creation will fail if enforcement is
enabled.

Revocation is non-destructive — the record can be reinstated by calling
`set_merchant_kyb` or `renew_merchant_kyb`.

Emits `MerchantKYBRevoked`.

---

## Querying KYB Status (`get_merchant_kyb_status`)

```rust
pub fn get_merchant_kyb_status(env: Env, merchant: Address) -> KYBStatus
```

**Authentication:** None — permissionless read.

Returns the `KYBStatus` struct for the merchant:

| Scenario | `verified` | `expiry_ledger` | `jurisdiction` |
| --- | --- | --- | --- |
| No record exists | `false` | `0` | `""` |
| Record exists, not expired, not revoked | `true` | stored value | stored value |
| Record exists, expired | `false` | stored value | stored value |
| Record exists, revoked | `false` | stored value | stored value |

The call also refreshes the persistent storage TTL for the record.

Expiry is measured in **ledger sequence numbers**, not timestamps. Callers
should convert real-world expiry dates to the expected ledger number using
approximate ledger cadence for their network.

---

## What `set_kyb_enforcement` Gates

When enforcement is enabled (`set_kyb_enforcement(admin, true)`), the only
function currently gated is `create_payment`. The check occurs at the start of
`create_payment` before any funds move:

```text
create_payment(customer, merchant, amount, token, …)
       │
       ├──► KYB enforcement disabled → proceeds normally
       │
       └──► KYB enforcement enabled
                   │
                   ├── no KYB record for merchant → Error::KYBVerificationRequired
                   ├── record revoked             → Error::KYBVerificationRequired
                   ├── record expired             → ExtError::MerchantKYBExpired
                   └── record valid               → proceeds normally
```

Other payment operations (refunds, disputes, releases) are not gated by KYB —
only the creation of new payments is blocked.

---

## Flow Diagram

```text
Admin calls set_kyb_enforcement(admin, true)
       │
       ▼
Admin calls set_merchant_kyb(admin, merchant, hash, expiry, jurisdiction)
       │ MerchantKYB { kyb_hash, expiry_ledger, jurisdiction, revoked: false }
       ▼
Customer calls create_payment(…, merchant, …)
       │ checks: !revoked && ledger.sequence() <= expiry_ledger
       ▼
   ┌───┴──────────────────────────┐
   │ Valid record                 │ Expired / revoked / missing
   ▼                              ▼
 Payment created            Error returned

       [Later: record nears expiry]
Admin calls renew_merchant_kyb(admin, merchant, new_hash, new_expiry, jurisdiction)
       │ Overwrites record, revoked reset to false
       ▼
 Merchant can accept payments again

       [Compliance issue found]
Admin calls revoke_merchant_kyb(admin, merchant)
       │ kyb.revoked = true
       ▼
 Merchant blocked until renewed
```

---

## Events

| Event | Emitted By | Payload | Description |
| --- | --- | --- | --- |
| `MerchantKYBSet` | `set_merchant_kyb`, `renew_merchant_kyb` | `(merchant, kyb_hash, expiry_ledger, jurisdiction)` | KYB record created or renewed. |
| `MerchantKYBRevoked` | `revoke_merchant_kyb` | `(merchant)` | KYB record revoked. |

---

## Error Codes

| Code | Name | Description |
| --- | --- | --- |
| — | `Error::KYBVerificationRequired` | Enforcement enabled; merchant has no valid KYB record. |
| 72 | `ExtError::MerchantKYBExpired` | Merchant KYB record has passed its `expiry_ledger`. |

---

## Function Reference

| Function | Caller | Purpose |
| --- | --- | --- |
| `set_kyb_enforcement(admin, enabled)` | Admin | Toggles KYB enforcement on/off globally. |
| `set_merchant_kyb(admin, merchant, kyb_hash, expiry_ledger, jurisdiction)` | Admin | Creates or overwrites a merchant KYB record. |
| `renew_merchant_kyb(admin, merchant, new_kyb_hash, new_expiry_ledger, jurisdiction)` | Admin | Renews an existing KYB record with a new hash and expiry. |
| `revoke_merchant_kyb(admin, merchant)` | Admin | Marks a merchant's KYB record as revoked. |
| `get_merchant_kyb_status(merchant)` | Anyone | Returns `KYBStatus` with `verified`, `expiry_ledger`, and `jurisdiction`. |

---

## Storage Reference

| Key | Storage Type | Data Stored |
| --- | --- | --- |
| `DataKey3::KYBRequired` | Instance | `bool` — primary enforcement flag. |
| `DataKey2::KYBEnforcementEnabled` | Instance | `bool` — legacy enforcement flag kept in sync. |
| `DataKey2::MerchantKYB(merchant)` | Persistent | `MerchantKYB` struct with hash, expiry, jurisdiction, and revoked flag. |
