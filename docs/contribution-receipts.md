# ROSCA Contribution Receipts

This document describes the design, data format, issuance lifecycle, and lookup methods for **NFT-style Contribution Receipts** in the Ahjoor ROSCA contract (Issue #448).

---

## 1. Receipt Format and Data Structures

Contribution receipts act as immutable, on-chain proof of contribution for ROSCA members upon the completion of each round. Each receipt contains detailed metadata regarding the contribution and a cryptographic hash to verify its authenticity.

### Data Structure: `ContributionReceipt`

Defined in `contracts/ahjoor-rosca/src/types.rs`:

```rust
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContributionReceipt {
    pub receipt_id: u32,
    pub member: Address,
    pub round: u32,
    pub amount_contributed: i128,
    pub token: Address,
    pub minted_at: u64,
    pub receipt_hash: BytesN<32>,
}
```

#### Field Descriptions

| Field | Type | Description |
| --- | --- | --- |
| `receipt_id` | `u32` | Globally unique incremental receipt identifier assigned sequentially starting at `0`. |
| `member` | `Address` | Stellar address of the contributing member. |
| `round` | `u32` | ROSCA round number for which the contribution was finalized. |
| `amount_contributed` | `i128` | Total token amount contributed by the member for the specified round. |
| `token` | `Address` | Contract address of the token used for the contribution. |
| `minted_at` | `u64` | Ledger timestamp (in seconds) when the receipt was minted. |
| `receipt_hash` | `BytesN<32>` | 256-bit SHA-256 deterministic cryptographic hash of the receipt payload. |

---

### Cryptographic Receipt Hash

To ensure data integrity and facilitate off-chain validation, each receipt includes a 32-byte SHA-256 hash (`receipt_hash`).

The hash preimage is constructed deterministically from binary serialization of the receipt payload:

$$\text{preimage} = \text{receipt\_id} \mathbin{\Vert} \text{round} \mathbin{\Vert} \text{XDR}(\text{member})$$

Where:
- `receipt_id` is encoded as big-endian 4-byte integer (`counter.to_be_bytes()`).
- `round` is encoded as big-endian 4-byte integer (`current_round.to_be_bytes()`).
- `member` is serialized via standard Soroban XDR encoding (`member.to_xdr(&env)`).

---

### Storage Keys & TTL Policy

Receipts are stored using Soroban storage primitives (`DataKey3` enum in `types.rs`):

| Key Variant | Storage Type | Content | TTL Management |
| --- | --- | --- | --- |
| `ContributionReceiptCounter` | Instance | `u32` - global incremental counter of minted receipts. | Extended via `INSTANCE_BUMP_AMOUNT` |
| `ContributionReceipt(u32)` | Persistent | `ContributionReceipt` struct mapped by `receipt_id`. | Extended via `PERSISTENT_BUMP_AMOUNT` |
| `MemberReceiptIds(Address)` | Persistent | `Vec<u32>` - list of receipt IDs owned by the specified `member`. | Extended via `PERSISTENT_BUMP_AMOUNT` |

---

### Event Definition: `ContributionReceiptMinted`

When a receipt is issued, the contract emits an event defined in `contracts/ahjoor-rosca/src/events.rs`:

```rust
pub struct ContributionReceiptMinted {
    pub receipt_id: u32,
    pub member: Address,
    pub round: u32,
    pub amount_contributed: i128,
    pub receipt_hash: BytesN<32>,
}
```

Event topics: `("contribution_receipt_minted", member)`
