//! Shared role-based access control helpers.
//!
//! Every Ahjoor contract used to carry its own admin / co-admin / operator /
//! delegate bookkeeping with slightly different rules. This module gives them
//! one storage layout and one set of checks:
//!
//! * [`grant_role`]   – give `account` the `role` (idempotent).
//! * [`revoke_role`]  – take the `role` away from `account` (idempotent).
//! * [`has_role`]     – read-only membership test.
//! * [`require_role`] – `account.require_auth()` + panic with
//!   [`RoleError::MissingRole`] when the role is not held.
//!
//! Roles are plain `Symbol`s chosen by the calling contract (for example
//! `symbol_short!("operator")`). The module does **not** decide who may grant
//! or revoke a role; the calling contract gates `grant_role` / `revoke_role`
//! behind its own admin check, exactly as it did before.
//!
//! Storage: one persistent entry per `(role, account)` pair under
//! [`RoleKey::Role`]. The key enum has its own type name, so its encoded form
//! never collides with a contract's `DataKey` variants.

use soroban_sdk::{contracterror, contractevent, contracttype, panic_with_error, Address, Env, Symbol};

/// TTL bump applied to role entries whenever they are written or checked.
const ROLE_TTL_THRESHOLD: u32 = 17_280 * 30; // ~30 days
const ROLE_TTL_BUMP: u32 = 17_280 * 60; // ~60 days

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum RoleError {
    /// The account does not hold the required role.
    MissingRole = 9_001,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RoleKey {
    /// `(role, account)` → `true` while the role is held.
    Role(Symbol, Address),
}

#[contractevent(topics = ["role_granted"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleGranted {
    #[topic]
    pub role: Symbol,
    #[topic]
    pub account: Address,
}

#[contractevent(topics = ["role_revoked"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RoleRevoked {
    #[topic]
    pub role: Symbol,
    #[topic]
    pub account: Address,
}

/// Grants `role` to `account`. Returns `true` when the role was newly
/// granted and `false` when the account already held it (no event emitted).
pub fn grant_role(env: &Env, role: &Symbol, account: &Address) -> bool {
    let key = RoleKey::Role(role.clone(), account.clone());
    let storage = env.storage().persistent();
    if storage.has(&key) {
        storage.extend_ttl(&key, ROLE_TTL_THRESHOLD, ROLE_TTL_BUMP);
        return false;
    }
    storage.set(&key, &true);
    storage.extend_ttl(&key, ROLE_TTL_THRESHOLD, ROLE_TTL_BUMP);
    RoleGranted { role: role.clone(), account: account.clone() }.publish(env);
    true
}

/// Revokes `role` from `account`. Returns `true` when the role was held and
/// has been removed, `false` when there was nothing to revoke.
pub fn revoke_role(env: &Env, role: &Symbol, account: &Address) -> bool {
    let key = RoleKey::Role(role.clone(), account.clone());
    let storage = env.storage().persistent();
    if !storage.has(&key) {
        return false;
    }
    storage.remove(&key);
    RoleRevoked { role: role.clone(), account: account.clone() }.publish(env);
    true
}

/// Returns `true` when `account` currently holds `role`.
pub fn has_role(env: &Env, role: &Symbol, account: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&RoleKey::Role(role.clone(), account.clone()))
}

/// Requires `account`'s authorization and that it holds `role`.
///
/// Panics with [`RoleError::MissingRole`] when the role is not held.
pub fn require_role(env: &Env, role: &Symbol, account: &Address) {
    account.require_auth();
    require_role_no_auth(env, role, account);
}

/// Like [`require_role`] but without calling `require_auth`, for call sites
/// that have already authenticated `account` (avoids a duplicate auth entry).
pub fn require_role_no_auth(env: &Env, role: &Symbol, account: &Address) {
    let key = RoleKey::Role(role.clone(), account.clone());
    let storage = env.storage().persistent();
    if !storage.has(&key) {
        panic_with_error!(env, RoleError::MissingRole);
    }
    storage.extend_ttl(&key, ROLE_TTL_THRESHOLD, ROLE_TTL_BUMP);
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, symbol_short, testutils::Address as _, Env};

    /// Minimal host contract so the helpers run inside a contract frame
    /// (storage and auth need one).
    #[contract]
    struct RolesHarness;

    #[contractimpl]
    impl RolesHarness {
        pub fn grant(env: Env, role: Symbol, account: Address) -> bool {
            grant_role(&env, &role, &account)
        }
        pub fn revoke(env: Env, role: Symbol, account: Address) -> bool {
            revoke_role(&env, &role, &account)
        }
        pub fn has(env: Env, role: Symbol, account: Address) -> bool {
            has_role(&env, &role, &account)
        }
        pub fn require(env: Env, role: Symbol, account: Address) {
            require_role(&env, &role, &account)
        }
        pub fn require_no_auth(env: Env, role: Symbol, account: Address) {
            require_role_no_auth(&env, &role, &account)
        }
    }

    fn setup() -> (Env, RolesHarnessClient<'static>) {
        let env = Env::default();
        let id = env.register(RolesHarness, ());
        let client = RolesHarnessClient::new(&env, &id);
        (env, client)
    }

    #[test]
    fn has_role_false_by_default() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        assert!(!c.has(&symbol_short!("operator"), &a));
    }

    #[test]
    fn grant_then_has_role() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        assert!(c.grant(&symbol_short!("operator"), &a));
        assert!(c.has(&symbol_short!("operator"), &a));
    }

    #[test]
    fn grant_twice_is_idempotent() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        assert!(c.grant(&symbol_short!("operator"), &a));
        assert!(!c.grant(&symbol_short!("operator"), &a));
        assert!(c.has(&symbol_short!("operator"), &a));
    }

    #[test]
    fn roles_are_scoped_per_role_and_account() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        c.grant(&symbol_short!("operator"), &a);
        assert!(!c.has(&symbol_short!("co_admin"), &a));
        assert!(!c.has(&symbol_short!("operator"), &b));
    }

    #[test]
    fn revoke_removes_role() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        c.grant(&symbol_short!("operator"), &a);
        assert!(c.revoke(&symbol_short!("operator"), &a));
        assert!(!c.has(&symbol_short!("operator"), &a));
    }

    #[test]
    fn revoke_missing_role_returns_false() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        assert!(!c.revoke(&symbol_short!("operator"), &a));
    }

    #[test]
    fn revoke_leaves_other_roles_intact() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        c.grant(&symbol_short!("operator"), &a);
        c.grant(&symbol_short!("co_admin"), &a);
        c.revoke(&symbol_short!("operator"), &a);
        assert!(c.has(&symbol_short!("co_admin"), &a));
    }

    #[test]
    fn require_role_passes_when_held() {
        let (env, c) = setup();
        env.mock_all_auths();
        let a = Address::generate(&env);
        c.grant(&symbol_short!("operator"), &a);
        c.require(&symbol_short!("operator"), &a);
        // require_role must have demanded the account's auth.
        assert_eq!(env.auths().len(), 1);
        assert_eq!(env.auths()[0].0, a);
    }

    #[test]
    fn require_role_fails_when_missing() {
        let (env, c) = setup();
        env.mock_all_auths();
        let a = Address::generate(&env);
        let res = c.try_require(&symbol_short!("operator"), &a);
        assert_eq!(res, Err(Ok(RoleError::MissingRole.into())));
    }

    #[test]
    fn require_role_fails_after_revoke() {
        let (env, c) = setup();
        env.mock_all_auths();
        let a = Address::generate(&env);
        c.grant(&symbol_short!("operator"), &a);
        c.revoke(&symbol_short!("operator"), &a);
        let res = c.try_require(&symbol_short!("operator"), &a);
        assert_eq!(res, Err(Ok(RoleError::MissingRole.into())));
    }

    #[test]
    fn require_role_fails_without_auth() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        c.grant(&symbol_short!("operator"), &a);
        // No mock_all_auths: the auth check must reject.
        assert!(c.try_require(&symbol_short!("operator"), &a).is_err());
    }

    #[test]
    fn require_role_no_auth_skips_auth() {
        let (env, c) = setup();
        let a = Address::generate(&env);
        c.grant(&symbol_short!("operator"), &a);
        // Succeeds with no auth mocked.
        c.require_no_auth(&symbol_short!("operator"), &a);
        let missing = Address::generate(&env);
        let res = c.try_require_no_auth(&symbol_short!("operator"), &missing);
        assert_eq!(res, Err(Ok(RoleError::MissingRole.into())));
    }
}
