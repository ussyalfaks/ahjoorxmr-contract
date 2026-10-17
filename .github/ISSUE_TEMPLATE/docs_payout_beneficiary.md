---
name: Documentation - Payout Beneficiary Nomination
about: Document the payout beneficiary nomination feature in ahjoor-rosca
title: "docs: Document payout beneficiary nomination in ahjoor-rosca"
labels: documentation, ahjoor-rosca
assignees: ''
---

## Overview
The ahjoor-rosca contract allows ROSCA members to nominate a beneficiary address to receive their payout instead of receiving it directly. This feature enables flexible fund distribution scenarios and enhances the utility of ROSCA payouts.

## What Needs to be Documented

### 1. Feature Purpose and Use Cases
Document why payout beneficiary nomination exists:
- **Direct-to-dependent payments**: Send payout directly to family members, dependents, or business partners
- **Automated bill payment**: Route payout to service providers
- **Trust arrangements**: Send payout to custodial accounts
- **Charitable giving**: Direct payout to charitable organizations
- **Business operations**: Route payouts to business accounts separate from personal wallets

### 2. How It Works
Explain the beneficiary nomination mechanism:
- Members can optionally nominate a beneficiary address
- When no beneficiary is set, the member receives the payout directly
- When a beneficiary is set, they receive the payout instead of the member
- The member remains responsible for contributions regardless of beneficiary
- Beneficiary settings do not affect contribution obligations

### 3. API Functions

#### Setting a Beneficiary
- `set_payout_beneficiary(member: Address, beneficiary: Address)`
  - Who can call it: The member themselves (requires auth)
  - Parameters and validation
  - Restrictions and timing considerations
  - Events emitted: `PayoutBeneficiarySet`
  - Error conditions

#### Clearing a Beneficiary
- `clear_payout_beneficiary(member: Address)`
  - Who can call it: The member themselves
  - Resets payout routing back to the member
  - Events emitted: `PayoutBeneficiaryCleared`
  - Error conditions

#### Querying Beneficiary
- `get_payout_beneficiary(member: Address) -> Option<Address>`
  - Returns the nominated beneficiary if set
  - Returns `None` if no beneficiary is nominated

### 4. Restrictions and Validations

#### Member-Only Access
- Only active ROSCA members can set/clear beneficiaries
- Non-members cannot use this feature
- Error: Member validation failure

#### Invalid Beneficiary Addresses
- Cannot set the contract itself as beneficiary
- Error: `InvalidBeneficiary` (135)
- Rationale: Prevents accidental fund locking

#### Lock During Payout Round
- Beneficiary changes are locked during the member's payout round
- Prevents manipulation after payout order is determined
- Error: `BeneficiaryLocked` (134)
- Lock applies to both `set_payout_beneficiary` and `clear_payout_beneficiary`
- Example: If member1 is scheduled for payout in round 0, they cannot change beneficiary during round 0

### 5. Interaction with Other Features

#### Reinvestment Preference
- **Priority**: Reinvestment preference takes priority over beneficiary nomination
- If `reinvest_preference` is set to `true`, the payout stays in the contract
- Beneficiary setting is ignored when reinvestment is enabled
- Member must disable reinvestment for beneficiary routing to take effect

#### Payout Vesting
- When payout vesting is enabled, funds are locked for gradual release
- On vesting release (via `claim_vested`), funds are sent to the beneficiary
- If no beneficiary is set at claim time, member receives the vested funds
- Beneficiary can be changed between vesting lock and claim
- Test reference: `test_payout_to_beneficiary_on_claim`

#### Holdback Release
- Holdback amounts are also paid to the nominated beneficiary
- Applies when holdback is released after all contributions are made
- Reference: `holdback.rs` - `release_holdback` function

### 6. Events

#### PayoutBeneficiarySet
```rust
pub struct PayoutBeneficiarySet {
    pub member: Address,
    pub beneficiary: Address,
}
```
- Emitted when a member nominates or changes their beneficiary
- Indexed by member address for easy querying

#### PayoutBeneficiaryCleared
```rust
pub struct PayoutBeneficiaryCleared {
    pub member: Address,
}
```
- Emitted when a member removes their beneficiary nomination
- Payout routing returns to the member

#### RoundPayoutDelivered
```rust
pub struct RoundPayoutDelivered {
    pub round: u32,
    pub member: Address,
    pub beneficiary: Address,
    pub token: Address,
    pub amount: i128,
}
```
- The `beneficiary` field shows the actual recipient
- When no beneficiary is nominated, `beneficiary` equals `member`
- When beneficiary is nominated, `beneficiary` is the nominated address

### 7. Storage

#### Data Key
- `DataKey5::PayoutBeneficiary(Address)` - Stores beneficiary address per member
- Key structure: Maps member address to beneficiary address
- Storage type: Instance storage
- Optional value: Returns `None` when no beneficiary is set

#### Storage Reads
Internal helper function:
```rust
fn payout_destination(env: &Env, member: &Address) -> Address {
    env.storage()
        .instance()
        .get(&DataKey5::PayoutBeneficiary(member.clone()))
        .unwrap_or(member.clone())
}
```

### 8. Error Handling
Document all beneficiary-related errors:

- `BeneficiaryLocked` (134)
  - Thrown when attempting to change beneficiary during member's payout round
  - Prevents gaming the system
  - Both set and clear operations are blocked

- `InvalidBeneficiary` (135)
  - Thrown when beneficiary address is invalid
  - Currently: Cannot be the contract itself
  - Prevents fund locking scenarios

### 9. Code Examples and Workflows

#### Example 1: Basic Beneficiary Setup
```rust
// Member nominates their spouse to receive payout
let member = Address::generate(&env);
let spouse = Address::generate(&env);

// Must be done before payout round starts
client.set_payout_beneficiary(&member, &spouse);

// Verify it's set
assert_eq!(client.get_payout_beneficiary(&member), Some(spouse.clone()));

// When member's payout round arrives, spouse receives funds automatically
```

#### Example 2: Clearing Beneficiary
```rust
// Member decides to receive payout directly
client.clear_payout_beneficiary(&member);

assert_eq!(client.get_payout_beneficiary(&member), None);
// Future payouts will go to member directly
```

#### Example 3: Timing Restrictions
```rust
// Round 0: member1 is scheduled for payout
// Attempting to change beneficiary fails
let result = client.try_set_payout_beneficiary(&member1, &beneficiary);
assert_eq!(result.unwrap_err().unwrap(), ExtError2::BeneficiaryLocked.into());

// After round completes, changes are allowed again
```

#### Example 4: With Reinvestment
```rust
// Member sets both beneficiary and reinvestment
client.set_payout_beneficiary(&member, &beneficiary);
client.set_reinvest_preference(&member, &true);

// Reinvestment takes priority - funds stay in contract
// Beneficiary setting is ignored until reinvestment is disabled
```

### 10. Security Considerations
- Members maintain control: Only member can set/nominate their own beneficiary
- No admin override: Admins cannot change member beneficiaries
- Lock prevents manipulation: Cannot change during active payout round
- Invalid address protection: Contract cannot be beneficiary

### 11. Best Practices

#### For Members
- Set beneficiary **before** your payout round begins
- Verify beneficiary address carefully (irreversible once round starts)
- Understand interaction with reinvestment preference
- Clear beneficiary if circumstances change (when not locked)

#### For Integrators
- Query `get_payout_beneficiary` before payout rounds to show routing
- Handle `BeneficiaryLocked` error gracefully in UI
- Show clear warnings about timing restrictions
- Display current beneficiary status in member dashboard

#### For Group Admins
- Educate members about beneficiary feature
- Explain timing restrictions in group charter
- Monitor `PayoutBeneficiarySet` events for audit trails

### 12. Implementation References
Key code locations:
- Type definition: `contracts/ahjoor-rosca/src/types.rs` - `DataKey5::PayoutBeneficiary`
- Main payout logic: `contracts/ahjoor-rosca/src/internals.rs` - payout destination routing
- Vesting integration: `contracts/ahjoor-rosca/src/vesting.rs` - `payout_destination` helper
- Holdback integration: `contracts/ahjoor-rosca/src/holdback.rs` - release routing
- Events: `contracts/ahjoor-rosca/src/events.rs` - beneficiary events
- Errors: `contracts/ahjoor-rosca/src/errors.rs` - `BeneficiaryLocked`, `InvalidBeneficiary`

### 13. Test Coverage
Reference tests demonstrating the feature:
- `test_set_and_clear_payout_beneficiary` - Basic operations
- `test_beneficiary_rejected_for_non_member` - Access control
- `test_beneficiary_cannot_be_contract` - Invalid address validation
- `test_beneficiary_change_locked_during_payout_round` - Timing restrictions
- `test_payout_routed_to_beneficiary` - Actual payout routing
- `test_reinvest_preference_takes_priority_over_beneficiary` - Feature interaction
- `test_payout_to_beneficiary_on_claim` - Vesting integration

## Documentation Location
Create comprehensive documentation in:
- `contracts/ahjoor-rosca/docs/PAYOUT_BENEFICIARY.md` (new file)
- Or add detailed section to `FEATURE_IMPLEMENTATION.md`
- Update inline code documentation

## Acceptance Criteria
- [ ] Feature purpose and use cases are clearly explained
- [ ] All API functions are documented with signatures
- [ ] Restrictions and validations are thoroughly described
- [ ] Interactions with reinvestment and vesting are explained
- [ ] All events are documented with field descriptions
- [ ] Error conditions are clearly explained
- [ ] At least 4 practical code examples are provided
- [ ] Security considerations are addressed
- [ ] Best practices for members and integrators are included
- [ ] Storage structure is documented
- [ ] Test references are provided

## Related Code Files
- `contracts/ahjoor-rosca/src/lib.rs` - Public API implementation
- `contracts/ahjoor-rosca/src/internals.rs` - Payout routing logic
- `contracts/ahjoor-rosca/src/vesting.rs` - Vesting integration
- `contracts/ahjoor-rosca/src/holdback.rs` - Holdback integration
- `contracts/ahjoor-rosca/src/types.rs` - Type definitions
- `contracts/ahjoor-rosca/src/errors.rs` - Error codes
- `contracts/ahjoor-rosca/src/events.rs` - Event definitions
- `contracts/ahjoor-rosca/src/test_payout_beneficiary.rs` - Full test suite
- `contracts/ahjoor-rosca/src/test_payout_vesting.rs` - Vesting interaction tests
