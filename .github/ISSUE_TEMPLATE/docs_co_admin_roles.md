---
name: Documentation - Co-Admin Roles and Permissions
about: Document the co-admin roles and permissions system in ahjoor-rosca
title: "docs: Document co-admin roles and permissions in ahjoor-rosca"
labels: documentation, ahjoor-rosca
assignees: ''
---

## Overview
The ahjoor-rosca contract implements a sophisticated co-admin system that allows the primary admin to delegate specific administrative permissions to other addresses. This issue tracks the documentation of this feature.

## What Needs to be Documented

### 1. Co-Admin System Architecture
- **Purpose**: Explain why co-admins exist and what problems they solve
- **Relationship to Primary Admin**: Clarify the hierarchy and how co-admins differ from the primary admin
- **Scope-based Permissions**: Document the permission model

### 2. Available Co-Admin Permissions
Document each permission type from `CoAdminPermission` enum:

- `ManageMembers` (0) - Can call `add_member` / `remove_member`
- `ManageBans` (1) - Can ban/unban members
- `ManageRoundConfig` (2) - Can adjust round timing and extension parameters
- `ManageMetadata` (3) - Can update group name, category, and metadata
- `ManagePauseFreezeEmergency` (4) - Can pause/unpause, freeze/unfreeze
- `ViewSensitiveData` (5) - Can read audit trails and member histories

### 3. Co-Admin Management Functions
Document the following API functions:

#### Adding Co-Admins
- `add_co_admin(admin: Address, co_admin: Address, permissions: Vec<CoAdminPermission>)`
  - Who can call it (primary admin only)
  - Parameter requirements
  - Validations performed
  - Events emitted: `CoAdminAdded`
  - Error conditions: `CoAdminAlreadyExists`, `CoAdminEmptyPermissions`

#### Removing Co-Admins
- `remove_co_admin(admin: Address, co_admin: Address)`
  - Who can call it
  - Events emitted: `CoAdminRemoved`
  - Error conditions: `CoAdminNotFound`

#### Updating Permissions
- `set_co_admin_permissions(admin: Address, co_admin: Address, permissions: Vec<CoAdminPermission>)`
  - How to modify existing co-admin permissions
  - Events emitted: `CoAdminPermissionsUpdated`
  - Validation requirements

#### Status Management
- `set_co_admin_active(admin: Address, co_admin: Address, active: bool)`
  - Temporarily enable/disable co-admin without removing them
  - Events emitted: `CoAdminStatusChanged`

#### Query Functions
- `get_co_admin(co_admin: Address) -> Option<CoAdminRecord>`
- `list_co_admins() -> Vec<CoAdminRecord>`
- `has_co_admin_permission(co_admin: Address, permission: CoAdminPermission) -> bool`

### 4. Co-Admin Record Structure
Document the `CoAdminRecord` type:
```rust
pub struct CoAdminRecord {
    pub co_admin: Address,
    pub permissions: Vec<CoAdminPermission>,
    pub added_at_ledger: u32,
    pub active: bool,
}
```

### 5. Access Control Patterns
Document how functions check for co-admin permissions:
- Functions that accept EITHER primary admin OR co-admin with specific permission
- How `require_co_admin_permission` helper works
- Examples of permission checks in various functions

### 6. Error Handling
Document all co-admin related errors from `ExtError2`:
- `NotACoAdminRole` (126) - Caller is not a co-admin
- `CoAdminPermissionDenied` (127) - Co-admin lacks required permission
- `CoAdminAlreadyExists` (128) - Address already registered as co-admin
- `CoAdminNotFound` (129) - No co-admin record found
- `CoAdminEmptyPermissions` (130) - Cannot create co-admin with empty permissions list

### 7. Events
Document all co-admin related events:
- `CoAdminAdded` - New co-admin registered
- `CoAdminRemoved` - Co-admin deleted
- `CoAdminStatusChanged` - Co-admin activated/deactivated
- `CoAdminPermissionsUpdated` - Permissions modified

### 8. Use Cases and Examples
Provide practical examples:
- Setting up a member manager co-admin
- Creating a metadata manager for branding updates
- Emergency response team with pause/freeze permissions
- Multi-role co-admin with multiple permissions
- Temporarily disabling a co-admin
- Upgrading co-admin permissions

### 9. Best Practices
- Recommended permission separation strategies
- Security considerations
- When to use co-admins vs. primary admin
- Audit trail considerations

### 10. Storage and Data Keys
Document storage:
- `DataKey5::CoAdmins` - Map of all co-admin records
- `DataKey5::CoAdminPermissions` - Permission mappings
- Storage type (instance storage)
- TTL and persistence considerations

## Documentation Location
Create or update documentation in one of:
- `contracts/ahjoor-rosca/docs/CO_ADMIN_SYSTEM.md` (new file)
- Add section to existing `FEATURE_IMPLEMENTATION.md`
- Create inline code documentation

## Acceptance Criteria
- [ ] All co-admin permissions are documented with clear descriptions
- [ ] All public API functions are documented with parameters and return values
- [ ] Error conditions are clearly explained
- [ ] Events are documented with field descriptions
- [ ] At least 3 practical use case examples are included
- [ ] Storage structure is explained
- [ ] Best practices section is included
- [ ] Code examples are provided where helpful

## Related Code
- `contracts/ahjoor-rosca/src/lib.rs` - Main implementation
- `contracts/ahjoor-rosca/src/types.rs` - Type definitions
- `contracts/ahjoor-rosca/src/errors.rs` - Error definitions
- `contracts/ahjoor-rosca/src/events.rs` - Event definitions
- `contracts/ahjoor-rosca/src/test_*.rs` - Test examples

## References
- Related test: `test_only_admin_or_metadata_co_admin_can_update`
- Related test: `test_only_admin_or_round_config_co_admin_sets_maximum`
- Existing issue #1087: test(rosca): Add test coverage for co-admin management
