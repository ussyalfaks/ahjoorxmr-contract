---
name: Feature - Invoice Line Items with Committed Hash
about: Add invoice line items with a committed hash to prevent tampering
title: "feat(payments): Add invoice line items with a committed hash"
labels: enhancement, ahjoor-payments
assignees: ''
---

## Problem Statement
Currently, invoice line items in the `MultiTokenInvoice` structure are stored as plain data without cryptographic commitment. This creates potential vulnerabilities:

- Line item details can be disputed after creation
- No cryptographic proof that original invoice terms haven't been modified
- Difficult to prove invoice authenticity in dispute scenarios
- Cannot verify off-chain stored invoice details match on-chain commitment
- Lack of tamper-evidence for audit trails

## Proposed Solution
Add a cryptographic commitment (hash) of invoice line items at invoice creation time that serves as an immutable proof of the original invoice terms.

### High-Level Approach
1. When creating an invoice, compute a hash of the serialized line items
2. Store this hash commitment on-chain alongside the invoice
3. Provide verification functions to check line items against the committed hash
4. Ensure the hash is immutable after invoice creation
5. Include the hash in relevant events for off-chain tracking

## Implementation Details

### 1. Type Changes to `MultiTokenInvoice`

Add a new field to the `MultiTokenInvoice` struct:

```rust
pub struct MultiTokenInvoice {
    // ... existing fields ...
    pub line_items: Vec<InvoiceLineItem>,
    
    /// Cryptographic commitment (hash) of the line items at creation time.
    /// Used to prove that line item details have not been tampered with.
    /// Computed as: sha256(serialized(line_items))
    pub line_items_hash: BytesN<32>,
    
    pub status: InvoiceStatus,
    // ... remaining fields ...
}
```

### 2. Hash Computation Function

Add a helper function to compute the line items hash:

```rust
/// Compute a cryptographic hash commitment of invoice line items.
/// 
/// The hash is computed by:
/// 1. Serializing the Vec<InvoiceLineItem> to bytes
/// 2. Computing SHA-256 of the serialized bytes
/// 3. Returning the 32-byte hash
/// 
/// This hash serves as an immutable proof of the original line items
/// and can be used to verify that line items haven't been tampered with.
fn compute_line_items_hash(env: &Env, line_items: &Vec<InvoiceLineItem>) -> BytesN<32> {
    let serialized = line_items.serialize(env);
    env.crypto().sha256(&serialized)
}
```

### 3. Hash Verification Function

Add a public function to verify line items against the committed hash:

```rust
/// Verify that provided line items match the committed hash in an invoice.
/// 
/// # Parameters
/// - `invoice_id`: The invoice ID to check
/// - `claimed_line_items`: The line items to verify
/// 
/// # Returns
/// - `true` if the hash of `claimed_line_items` matches the stored commitment
/// - `false` otherwise
/// 
/// # Use Cases
/// - Verify that off-chain invoice details match on-chain commitment
/// - Prove invoice authenticity in disputes
/// - Validate that invoice terms haven't been modified
pub fn verify_line_items_hash(
    env: Env,
    invoice_id: u32,
    claimed_line_items: Vec<InvoiceLineItem>,
) -> bool;
```

### 4. Update `create_invoice` Function

Modify the invoice creation to compute and store the hash:

```rust
pub fn create_invoice(
    env: Env,
    merchant: Address,
    customer: Address,
    total_amount: i128,
    base_currency: Address,
    accepted_tokens: Vec<Address>,
    preferred_settlement_token: Address,
    line_items: Vec<InvoiceLineItem>,
    due_date: u64,
    metadata: Map<String, String>,
) -> u32 {
    merchant.require_auth();
    
    // Compute committed hash of line items
    let line_items_hash = compute_line_items_hash(&env, &line_items);
    
    // Validate line items match total_amount
    validate_line_items(&line_items, total_amount)?;
    
    let invoice_id = generate_invoice_id(&env);
    let invoice = MultiTokenInvoice {
        invoice_id,
        merchant: merchant.clone(),
        customer: customer.clone(),
        created_at: env.ledger().timestamp(),
        due_date,
        total_amount,
        base_currency,
        accepted_tokens,
        preferred_settlement_token,
        line_items,
        line_items_hash,  // Store the commitment
        status: InvoiceStatus::Issued,
        payments_received: Map::new(&env),
        conversion_rates: Map::new(&env),
        settlement_conversion_rate: 0,
        oracle_contract: None,
        metadata,
    };
    
    save_invoice(&env, &invoice);
    emit_invoice_created(&env, invoice_id, merchant, customer, line_items_hash);
    
    invoice_id
}
```

### 5. Event Updates

Update or create events to include the hash commitment:

```rust
#[contractevent]
#[derive(Clone, Debug)]
pub struct InvoiceCreatedWithCommitment {
    pub invoice_id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub total_amount: i128,
    pub base_currency: Address,
    /// Committed hash of the line items for tamper-evidence
    pub line_items_hash: BytesN<32>,
    pub created_at: u64,
    pub due_date: u64,
}
```

### 6. Additional Query Functions

```rust
/// Get the committed line items hash for an invoice.
/// Returns None if invoice doesn't exist.
pub fn get_line_items_hash(env: Env, invoice_id: u32) -> Option<BytesN<32>>;

/// Get full invoice line items and verify them against the commitment.
/// Returns the line items and a boolean indicating if they match the hash.
pub fn get_verified_line_items(
    env: Env,
    invoice_id: u32,
) -> Option<(Vec<InvoiceLineItem>, bool)>;
```

## Use Cases

### 1. Dispute Resolution
```rust
// Customer claims invoice was modified after creation
// Original line items can be verified against the immutable hash commitment

let original_line_items = get_original_invoice_details_from_offchain_storage();
let is_authentic = contract.verify_line_items_hash(
    invoice_id,
    original_line_items
);

if is_authentic {
    // Proves the invoice hasn't been tampered with
    proceed_with_original_terms();
} else {
    // Line items don't match - possible tampering detected
    escalate_dispute();
}
```

### 2. Off-Chain Storage Verification
```rust
// Store detailed invoice off-chain (IPFS, database) for cost savings
// Keep only the hash on-chain for verification

// On-chain: just the commitment
let invoice = contract.create_invoice(
    merchant,
    customer,
    total,
    currency,
    tokens,
    settlement_token,
    line_items.clone(),
    due_date,
    metadata
);

// Off-chain: full details with reference to on-chain hash
store_to_ipfs(InvoiceDetails {
    line_items: line_items,
    hash_commitment: invoice.line_items_hash,
    invoice_id: invoice_id,
});

// Later: verify off-chain data matches on-chain commitment
let retrieved_items = fetch_from_ipfs(ipfs_cid);
assert!(contract.verify_line_items_hash(invoice_id, retrieved_items));
```

### 3. Audit Trail and Compliance
```rust
// Auditors can verify historical invoices haven't been altered
// Hash provides cryptographic proof of original terms

for invoice_id in get_all_invoices() {
    let invoice = contract.get_invoice(invoice_id);
    let stored_items = invoice.line_items;
    let commitment = invoice.line_items_hash;
    
    // Recompute hash and compare
    let computed_hash = compute_line_items_hash(&stored_items);
    assert_eq!(computed_hash, commitment, "Invoice {} tampered!", invoice_id);
}
```

### 4. Multi-Party Verification
```rust
// All parties receive the hash at creation
// Any party can later verify they have the authentic invoice

// Merchant, customer, and arbiter all receive invoice creation event
// with line_items_hash included

// Later, any party can verify their copy is authentic
let my_invoice_copy = load_from_local_storage(invoice_id);
let is_authentic = contract.verify_line_items_hash(
    invoice_id,
    my_invoice_copy.line_items
);
```

## Technical Considerations

### Hash Function Choice
- Use SHA-256 via `env.crypto().sha256()` for standardization
- Provides 256-bit security which is sufficient for invoice commitments
- Widely supported and understood

### Serialization
- Use Soroban's built-in serialization for consistency
- Ensure deterministic serialization (same input = same output)
- Order of fields in `InvoiceLineItem` matters

### Gas/Storage Costs
- Adding one `BytesN<32>` field: minimal storage increase (32 bytes)
- Hash computation on creation: one-time cost, O(n) in number of line items
- Verification: optional, only when needed

### Immutability
- Hash is set at creation and NEVER modified
- Even if invoice status changes, hash remains unchanged
- Provides permanent proof of original terms

## Error Handling

Add error cases if needed:

```rust
pub enum MultiTokenInvoiceError {
    // ... existing errors ...
    
    /// Line items verification failed - hash mismatch
    LineItemsHashMismatch = 14,
    
    /// Cannot modify line items after invoice creation (hash is committed)
    LineItemsImmutable = 15,
}
```

## Testing Requirements

### Unit Tests
```rust
#[test]
fn test_line_items_hash_computed_on_creation() {
    // Create invoice with line items
    // Verify hash is computed and stored
    // Verify hash is not zero/empty
}

#[test]
fn test_verify_line_items_hash_success() {
    // Create invoice
    // Retrieve line items
    // Verify they match the hash
}

#[test]
fn test_verify_line_items_hash_fails_on_modification() {
    // Create invoice
    // Modify line items slightly
    // Verify hash check fails
}

#[test]
fn test_line_items_hash_different_for_different_items() {
    // Create two invoices with different line items
    // Verify they have different hashes
}

#[test]
fn test_line_items_hash_same_for_identical_items() {
    // Create two invoices with identical line items
    // Verify they have the same hash (deterministic)
}

#[test]
fn test_get_verified_line_items() {
    // Create invoice
    // Retrieve via get_verified_line_items
    // Verify tuple includes items and true verification
}

#[test]
fn test_hash_immutable_through_status_changes() {
    // Create invoice
    // Store original hash
    // Change invoice status (paid, cancelled, etc.)
    // Verify hash hasn't changed
}
```

### Integration Tests
```rust
#[test]
fn test_dispute_resolution_with_hash_verification() {
    // Simulate dispute scenario
    // Original invoice vs. claimed modified invoice
    // Verify hash proves authenticity
}

#[test]
fn test_off_chain_storage_verification_flow() {
    // Simulate storing details off-chain
    // Retrieve and verify against on-chain hash
}
```

## Security Considerations

### Strengths
- **Tamper-evident**: Any modification to line items is detectable
- **Cryptographic proof**: SHA-256 provides strong collision resistance
- **Immutable commitment**: Set once at creation, never modified
- **Transparent verification**: Anyone with invoice ID can verify

### Limitations
- **Hash collision**: Theoretically possible but computationally infeasible with SHA-256
- **Garbage in, garbage out**: Hash proves consistency, not correctness of original data
- **Pre-image attack**: Attacker cannot reverse hash to discover line items (privacy preserved)

### Best Practices
- Always verify line items before processing payments
- Store hash in events for off-chain tracking
- Use hash for audit logging
- Include hash in dispute resolution protocols

## Migration Strategy

### For Existing Invoices
- Invoices created before this feature won't have a hash
- Options:
  1. Compute hash retroactively (if line items are still available)
  2. Mark with sentinel value (e.g., all zeros)
  3. Add `Option<BytesN<32>>` for backward compatibility

### Recommended Approach
Use `Option<BytesN<32>>`:
```rust
pub struct MultiTokenInvoice {
    // ... fields ...
    /// Line items hash commitment (None for legacy invoices created before feature)
    pub line_items_hash: Option<BytesN<32>>,
    // ... remaining fields ...
}
```

## Documentation Requirements

- [ ] Update `MultiTokenInvoice` struct documentation
- [ ] Document hash computation algorithm
- [ ] Add verification function examples
- [ ] Include dispute resolution workflow
- [ ] Explain off-chain storage pattern
- [ ] Security considerations and limitations
- [ ] Migration guide for existing integrations

## Acceptance Criteria

- [ ] `line_items_hash` field added to `MultiTokenInvoice` struct
- [ ] Hash computation function implemented
- [ ] Hash is computed and stored at invoice creation
- [ ] `verify_line_items_hash()` function implemented and working
- [ ] `get_line_items_hash()` query function implemented
- [ ] Events updated to include hash commitment
- [ ] All specified unit tests pass
- [ ] Integration tests demonstrate real-world scenarios
- [ ] Hash remains immutable through status changes
- [ ] Backward compatibility maintained (or migration path documented)
- [ ] Code documentation explains hash purpose and usage
- [ ] Security considerations are documented

## Related Code Files

- `contracts/ahjoor-payments/src/multi_token_invoice.rs` - Type definitions
- `contracts/ahjoor-payments/src/multi_token_invoice_impl.rs` - Implementation
- `contracts/ahjoor-payments/src/lib.rs` - Public API
- `contracts/ahjoor-payments/src/events.rs` - Event definitions

## Alternative Approaches Considered

### 1. Merkle Tree
- **Pros**: Can prove individual line items without revealing others
- **Cons**: More complex, higher gas costs, overkill for typical invoices
- **Decision**: Simple hash is sufficient for most use cases

### 2. Hash Each Line Item Separately
- **Pros**: Granular verification
- **Cons**: More storage (N hashes vs 1), more complex verification
- **Decision**: Single hash of all items is simpler and sufficient

### 3. Use Metadata Field
- **Pros**: No struct changes needed
- **Cons**: Less type-safe, harder to enforce, easy to forget
- **Decision**: First-class field is clearer and more robust

## Future Enhancements

- Support for partial line item verification (Merkle tree upgrade)
- Cross-contract hash verification protocols
- Integration with dispute resolution contracts
- Automated hash verification on payment acceptance
- Hash-based invoice templates and cloning

## References

- SHA-256 documentation: [NIST FIPS 180-4](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.180-4.pdf)
- Soroban crypto API: `env.crypto().sha256()`
- Related: Multi-token invoice feature (#354)
