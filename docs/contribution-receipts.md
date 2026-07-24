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

---

## 2. Receipt Issuance Mechanism and Lifecycle

Contribution receipts are **minted automatically** during round finalization at the conclusion of each ROSCA round.

### Issuing Function

Receipts are minted by the contract entrypoint:

```rust
pub fn finalize_round(env: Env)
```

Defined in `contracts/ahjoor-rosca/src/lib.rs`.

### Caller & Access Control

- **Role Requirement**: Only the ROSCA contract **Admin** (`admin.require_auth()`) can execute `finalize_round`.
- **System Guards**: Contract must not be paused (`check_not_paused`) and not frozen (`check_not_frozen`).

### Prerequisites & Timing

- The round deadline must have expired (`env.ledger().timestamp() > deadline`). If called before the deadline, it panics with `Error::DeadlineNotPassed`.

### Member Eligibility

- Receipts are issued **only to members who have fully paid** their required contribution for the round (`paid_members` set).
- Members who paid via multiple partial installments are included once their cumulative payments reach the required `contribution_amount`.
- Defaulters (members who failed to pay by the round deadline) do **not** receive a contribution receipt for that round.

---

### Minting Algorithm (Step-by-Step)

During `finalize_round()`, the contract performs the following steps for minting receipts:

```text
               +----------------------------------+
               |      finalize_round(env)         |
               +----------------------------------+
                                |
                                v
               +----------------------------------+
               | Fetch paid_members & current_round|
               +----------------------------------+
                                |
                                v
                +--------------------------------+
                | Read ContributionReceiptCounter|
                +--------------------------------+
                                |
                                v
                   +------------------------+
                   |  For each paid_member  |
                   +------------------------+
                                |
         +----------------------+----------------------+
         |                                             |
         v                                             v
+-------------------------------+         +----------------------------+
| 1. Construct SHA-256 Preimage |         | 2. Build Receipt Struct    |
| (counter || round || member)  |         | (receipt_id, member, round,|
+-------------------------------+         | amount, token, timestamp,  |
         |                                | receipt_hash)              |
         +----------------------+---------+----------------------------+
                                |
                                v
                 +-----------------------------+
                 | 3. Set persistent storage:  |
                 | ContributionReceipt(counter)|
                 +-----------------------------+
                                |
                                v
                 +-----------------------------+
                 | 4. Append counter to        |
                 | MemberReceiptIds(member)    |
                 +-----------------------------+
                                |
                                v
                 +-----------------------------+
                 | 5. Emit Event:              |
                 | ContributionReceiptMinted   |
                 +-----------------------------+
                                |
                                v
                 +-----------------------------+
                 | 6. counter += 1             |
                 +-----------------------------+
                                |
                                v
                 +-----------------------------+
                 | Save updated Counter        |
                 +-----------------------------+
```

1. **Retrieve Token & Counter**: Reads token contract address (`DataKey::Token`) and current global counter (`DataKey3::ContributionReceiptCounter`, defaulting to `0`).
2. **Iterate Paid Members**: Iterates sequentially over `paid_members`:
   - Looks up `amount_contributed` from `DataKey::MemberContributions`.
   - Computes deterministic SHA-256 hash `receipt_hash` over `(counter || round || member_xdr)`.
   - Constructs `ContributionReceipt` with `minted_at = env.ledger().timestamp()`.
   - Writes receipt to persistent storage (`DataKey3::ContributionReceipt(counter)`).
   - Appends `counter` to the vector stored under `DataKey3::MemberReceiptIds(member)`.
   - Bumps storage TTL for both persistent entries to ensure longevity (`PERSISTENT_LIFETIME_THRESHOLD`, `PERSISTENT_BUMP_AMOUNT`).
   - Emits `ContributionReceiptMinted` event.
   - Increments `counter` by `1`.
3. **Persist Global Counter**: Writes updated `counter` back to `DataKey3::ContributionReceiptCounter` in instance storage.

---

## 3. Receipt Retrieval, Querying, and Verification

ROSCA members, client applications, and off-chain indexers can retrieve and cryptographically verify contribution receipts using the read-only contract query functions or Soroban event RPC subscriptions.

### Read-Only Query API

Defined in `contracts/ahjoor-rosca/src/lib.rs`:

#### 1. `get_contribution_receipt(env: Env, receipt_id: u32) -> ContributionReceipt`

Retrieves a single contribution receipt by its unique `receipt_id`.

- **Parameters**: `receipt_id` (`u32`) - unique numeric identifier of the receipt.
- **Returns**: `ContributionReceipt` struct containing full receipt details.
- **Errors**: Panics with `ExtError2::ReceiptNotFound` (numeric code `203`) if the `receipt_id` does not exist in storage.

#### 2. `get_member_receipt_ids(env: Env, member: Address) -> Vec<u32>`

Retrieves all receipt IDs minted for a specific member address across all rounds.

- **Parameters**: `member` (`Address`) - Stellar address of the member.
- **Returns**: `Vec<u32>` - list of numeric receipt IDs ordered chronologically by minting order. Returns an empty vector `[]` if the member has no receipts.

#### 3. `get_contribution_receipt_count(env: Env) -> u32`

Returns the cumulative total number of contribution receipts minted by the contract instance so far.

- **Parameters**: None.
- **Returns**: `u32` - total receipt count.

---

### Stellar CLI Usage Examples

#### Look up a receipt by ID

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- get_contribution_receipt \
  --receipt_id 0
```

#### List all receipt IDs for a member

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- get_member_receipt_ids \
  --member G...ADDRESS
```

#### Query total receipt count

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- get_contribution_receipt_count
```

---

### Off-Chain Receipt Verification

Clients or third-party verifiers can verify that a receipt is authentic and un-tampered by re-computing its SHA-256 hash:

1. Retrieve the receipt struct via `get_contribution_receipt(receipt_id)`.
2. Construct the binary payload:
   - `counter_bytes` = 4-byte big-endian byte array of `receipt_id`.
   - `round_bytes` = 4-byte big-endian byte array of `round`.
   - `member_xdr` = Soroban XDR byte array of `member` Address.
   - `payload` = `counter_bytes` + `round_bytes` + `member_xdr`.
3. Compute 256-bit SHA-256 hash over `payload`.
4. Assert that `SHA256(payload) == receipt.receipt_hash`.

If the hashes match, the receipt is confirmed to be authentic and generated directly by the Ahjoor ROSCA smart contract.

---

### Event Subscriptions

Applications can listen for real-time receipt issuance by subscribing to Soroban RPC event streams for topic:

```json
{
  "topics": ["contribution_receipt_minted", "<MEMBER_ADDRESS>"]
}
```

Each event payload provides the `receipt_id`, `member`, `round`, `amount_contributed`, and `receipt_hash`.
