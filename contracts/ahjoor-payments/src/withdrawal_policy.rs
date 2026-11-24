//! #1169: Multisig approval for large merchant withdrawals.
//!
//! A merchant can register a withdrawal policy: a signer set, an approval
//! threshold, and an `amount_threshold`. Once a policy exists,
//! `withdraw_collateral` with the merchant key alone is limited to
//! `amount_threshold` per rolling [`WITHDRAWAL_POLICY_WINDOW_SECONDS`]
//! window (so a large withdrawal cannot be split into many small ones).
//! Anything larger must go through `request_large_withdrawal`, which locks the
//! amount and queues it until `threshold` signers have approved it.
//!
//! Changing or removing an existing policy needs approval from the current
//! policy's `threshold` signers in the same call.

use crate::{
    events, AhjoorPaymentsContract, AhjoorPaymentsContractArgs, AhjoorPaymentsContractClient,
    DataKey, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD,
};
use soroban_sdk::{
    contracterror, contractevent, contractimpl, contracttype, panic_with_error, token, Address,
    Env, Vec,
};

/// Maximum number of signers in a withdrawal policy.
pub const MAX_WITHDRAWAL_SIGNERS: u32 = 10;

/// Rolling window over which single-key withdrawals are summed and compared
/// against the policy's `amount_threshold`.
pub const WITHDRAWAL_POLICY_WINDOW_SECONDS: u64 = 24 * 60 * 60;

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WithdrawalPolicyError {
    /// Signers empty/too many/duplicated, threshold out of range, or negative amount threshold.
    InvalidWithdrawalPolicy = 120,
    /// The merchant has no withdrawal policy.
    NoWithdrawalPolicy = 121,
    /// Address is not a signer of the merchant's current policy.
    NotAPolicySigner = 122,
    /// Fewer than `threshold` current signers approved the policy change.
    InsufficientPolicyApprovals = 123,
    /// The same approver was listed more than once.
    DuplicatePolicyApprover = 124,
    /// Withdrawal exceeds the single-key allowance; use `request_large_withdrawal`.
    LargeWithdrawalRequiresApproval = 125,
    /// Withdrawal amount must be positive.
    InvalidWithdrawalAmount = 126,
    /// Withdrawal exceeds the merchant's collateral balance.
    InsufficientCollateral = 127,
    /// Withdrawal would drop collateral below the configured minimum.
    BelowMinimumCollateral = 128,
    /// No large withdrawal request with this ID.
    WithdrawalRequestNotFound = 129,
    /// The request was already executed or cancelled.
    WithdrawalRequestNotPending = 130,
    /// This signer already approved the request.
    WithdrawalAlreadyApproved = 131,
    /// Caller is not the merchant that made the request.
    NotWithdrawalRequester = 132,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WithdrawalPolicy {
    pub signers: Vec<Address>,
    /// Number of signer approvals required (1..=signers.len()).
    pub threshold: u32,
    /// Maximum the merchant key may withdraw alone per window.
    pub amount_threshold: i128,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LargeWithdrawalStatus {
    Pending = 0,
    Executed = 1,
    Cancelled = 2,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LargeWithdrawal {
    pub id: u32,
    pub merchant: Address,
    pub amount: i128,
    pub approvals: Vec<Address>,
    pub status: LargeWithdrawalStatus,
    pub created_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SingleKeyWithdrawalWindow {
    pub window_start: u64,
    pub withdrawn: i128,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WithdrawalPolicyKey {
    Policy(Address),
    Window(Address),
    Counter,
    Request(u32),
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct WithdrawalPolicyUpdated {
    pub merchant: Address,
    pub threshold: u32,
    pub signer_count: u32,
    pub amount_threshold: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct WithdrawalPolicyRemoved {
    pub merchant: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct LargeWithdrawalRequested {
    pub request_id: u32,
    pub merchant: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct LargeWithdrawalApproved {
    pub request_id: u32,
    pub signer: Address,
    pub approvals: u32,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct LargeWithdrawalExecuted {
    pub request_id: u32,
    pub merchant: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct LargeWithdrawalCancelled {
    pub request_id: u32,
    pub merchant: Address,
    pub amount: i128,
}

fn persist<K, V>(env: &Env, key: &K, value: &V)
where
    K: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
    V: soroban_sdk::IntoVal<Env, soroban_sdk::Val>,
{
    env.storage().persistent().set(key, value);
    env.storage().persistent().extend_ttl(
        key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

fn load_policy(env: &Env, merchant: &Address) -> Option<WithdrawalPolicy> {
    env.storage()
        .persistent()
        .get(&WithdrawalPolicyKey::Policy(merchant.clone()))
}

fn require_policy(env: &Env, merchant: &Address) -> WithdrawalPolicy {
    match load_policy(env, merchant) {
        Some(p) => p,
        None => panic_with_error!(env, WithdrawalPolicyError::NoWithdrawalPolicy),
    }
}

fn validate_policy(env: &Env, signers: &Vec<Address>, threshold: u32, amount_threshold: i128) {
    let n = signers.len();
    if n == 0
        || n > MAX_WITHDRAWAL_SIGNERS
        || threshold == 0
        || threshold > n
        || amount_threshold < 0
    {
        panic_with_error!(env, WithdrawalPolicyError::InvalidWithdrawalPolicy);
    }
    for i in 0..n {
        let a = signers.get(i).unwrap();
        for j in (i + 1)..n {
            if signers.get(j).unwrap() == a {
                panic_with_error!(env, WithdrawalPolicyError::InvalidWithdrawalPolicy);
            }
        }
    }
}

/// Requires `policy.threshold` distinct current signers among `approvers`,
/// each authorizing the call. `merchant` has already authorized, so it is not
/// asked twice (a second `require_auth` for one address would fail).
fn require_quorum(
    env: &Env,
    merchant: &Address,
    policy: &WithdrawalPolicy,
    approvers: &Vec<Address>,
) {
    let n = approvers.len();
    for i in 0..n {
        let a = approvers.get(i).unwrap();
        for j in (i + 1)..n {
            if approvers.get(j).unwrap() == a {
                panic_with_error!(env, WithdrawalPolicyError::DuplicatePolicyApprover);
            }
        }
        if !policy.signers.contains(&a) {
            panic_with_error!(env, WithdrawalPolicyError::NotAPolicySigner);
        }
        if a != *merchant {
            a.require_auth();
        }
    }
    if n < policy.threshold {
        panic_with_error!(env, WithdrawalPolicyError::InsufficientPolicyApprovals);
    }
}

fn load_request(env: &Env, request_id: u32) -> LargeWithdrawal {
    match env
        .storage()
        .persistent()
        .get(&WithdrawalPolicyKey::Request(request_id))
    {
        Some(r) => r,
        None => panic_with_error!(env, WithdrawalPolicyError::WithdrawalRequestNotFound),
    }
}

fn collateral_token(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::UsdcToken)
        .expect("Collateral token not configured")
}

/// Hook for `withdraw_collateral`: with a policy in place, the merchant key
/// alone may withdraw at most `amount_threshold` per rolling window.
pub(crate) fn enforce_single_key_withdrawal(env: &Env, merchant: &Address, amount: i128) {
    let policy = match load_policy(env, merchant) {
        Some(p) => p,
        None => return,
    };
    let key = WithdrawalPolicyKey::Window(merchant.clone());
    let now = env.ledger().timestamp();
    let mut window: SingleKeyWithdrawalWindow =
        env.storage()
            .persistent()
            .get(&key)
            .unwrap_or(SingleKeyWithdrawalWindow {
                window_start: now,
                withdrawn: 0,
            });
    if now.saturating_sub(window.window_start) >= WITHDRAWAL_POLICY_WINDOW_SECONDS {
        window.window_start = now;
        window.withdrawn = 0;
    }
    let total = window.withdrawn.saturating_add(amount);
    if total > policy.amount_threshold {
        panic_with_error!(env, WithdrawalPolicyError::LargeWithdrawalRequiresApproval);
    }
    window.withdrawn = total;
    persist(env, &key, &window);
}

#[contractimpl]
impl AhjoorPaymentsContract {
    /// #1169: Set or replace the merchant's withdrawal policy.
    ///
    /// The first policy needs only the merchant's authorization. Replacing an
    /// existing policy also needs `threshold` distinct signers of the
    /// *current* policy listed in `approvers` (each must authorize).
    pub fn set_withdrawal_policy(
        env: Env,
        merchant: Address,
        signers: Vec<Address>,
        threshold: u32,
        amount_threshold: i128,
        approvers: Vec<Address>,
    ) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        validate_policy(&env, &signers, threshold, amount_threshold);

        if let Some(current) = load_policy(&env, &merchant) {
            require_quorum(&env, &merchant, &current, &approvers);
        }

        let signer_count = signers.len();
        persist(
            &env,
            &WithdrawalPolicyKey::Policy(merchant.clone()),
            &WithdrawalPolicy {
                signers,
                threshold,
                amount_threshold,
            },
        );
        WithdrawalPolicyUpdated {
            merchant,
            threshold,
            signer_count,
            amount_threshold,
        }
        .publish(&env);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// #1169: Remove the merchant's withdrawal policy. Needs `threshold`
    /// distinct current signers in `approvers`.
    pub fn remove_withdrawal_policy(env: Env, merchant: Address, approvers: Vec<Address>) {
        Self::require_not_paused(&env);
        merchant.require_auth();
        let current = require_policy(&env, &merchant);
        require_quorum(&env, &merchant, &current, &approvers);

        env.storage()
            .persistent()
            .remove(&WithdrawalPolicyKey::Policy(merchant.clone()));
        env.storage()
            .persistent()
            .remove(&WithdrawalPolicyKey::Window(merchant.clone()));
        WithdrawalPolicyRemoved { merchant }.publish(&env);
    }

    /// #1169: The merchant's withdrawal policy, if any.
    pub fn get_withdrawal_policy(env: Env, merchant: Address) -> Option<WithdrawalPolicy> {
        load_policy(&env, &merchant)
    }

    /// #1169: Queue a collateral withdrawal for signer approval. The amount is
    /// deducted from the merchant's collateral immediately (so it cannot be
    /// withdrawn twice) and paid out once `threshold` signers approve.
    pub fn request_large_withdrawal(env: Env, merchant: Address, amount: i128) -> u32 {
        Self::require_not_paused(&env);
        merchant.require_auth();
        require_policy(&env, &merchant);

        if amount <= 0 {
            panic_with_error!(&env, WithdrawalPolicyError::InvalidWithdrawalAmount);
        }
        let key = DataKey::MerchantCollateral(merchant.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        if amount > current {
            panic_with_error!(&env, WithdrawalPolicyError::InsufficientCollateral);
        }
        let remaining = current - amount;
        if remaining < Self::get_min_collateral_internal(&env) {
            panic_with_error!(&env, WithdrawalPolicyError::BelowMinimumCollateral);
        }
        persist(&env, &key, &remaining);

        let request_id: u32 = env
            .storage()
            .instance()
            .get(&WithdrawalPolicyKey::Counter)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&WithdrawalPolicyKey::Counter, &(request_id + 1));

        persist(
            &env,
            &WithdrawalPolicyKey::Request(request_id),
            &LargeWithdrawal {
                id: request_id,
                merchant: merchant.clone(),
                amount,
                approvals: Vec::new(&env),
                status: LargeWithdrawalStatus::Pending,
                created_at: env.ledger().timestamp(),
            },
        );
        LargeWithdrawalRequested {
            request_id,
            merchant,
            amount,
        }
        .publish(&env);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        request_id
    }

    /// #1169: A current policy signer approves a queued withdrawal. When the
    /// approvals from current signers reach the policy threshold, the
    /// withdrawal is paid to the merchant's payout address. Returns `true`
    /// if this approval executed the withdrawal.
    pub fn approve_large_withdrawal(env: Env, signer: Address, request_id: u32) -> bool {
        Self::require_not_paused(&env);
        signer.require_auth();

        let mut request = load_request(&env, request_id);
        if request.status != LargeWithdrawalStatus::Pending {
            panic_with_error!(&env, WithdrawalPolicyError::WithdrawalRequestNotPending);
        }
        let policy = require_policy(&env, &request.merchant);
        if !policy.signers.contains(&signer) {
            panic_with_error!(&env, WithdrawalPolicyError::NotAPolicySigner);
        }
        if request.approvals.contains(&signer) {
            panic_with_error!(&env, WithdrawalPolicyError::WithdrawalAlreadyApproved);
        }
        request.approvals.push_back(signer.clone());

        // Approvals from signers removed by a later policy change don't count.
        let mut valid: u32 = 0;
        for a in request.approvals.iter() {
            if policy.signers.contains(&a) {
                valid += 1;
            }
        }
        LargeWithdrawalApproved {
            request_id,
            signer,
            approvals: valid,
        }
        .publish(&env);

        let executed = valid >= policy.threshold;
        if executed {
            request.status = LargeWithdrawalStatus::Executed;
            token::Client::new(&env, &collateral_token(&env)).transfer(
                &env.current_contract_address(),
                Self::resolve_payout_address(&env, &request.merchant),
                &request.amount,
            );
            events::emit_collateral_withdrawn(&env, request.merchant.clone(), request.amount);
            LargeWithdrawalExecuted {
                request_id,
                merchant: request.merchant.clone(),
                amount: request.amount,
            }
            .publish(&env);
        }
        persist(&env, &WithdrawalPolicyKey::Request(request_id), &request);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        executed
    }

    /// #1169: The requesting merchant cancels a queued withdrawal; the locked
    /// amount is returned to their collateral balance.
    pub fn cancel_large_withdrawal(env: Env, merchant: Address, request_id: u32) {
        Self::require_not_paused(&env);
        merchant.require_auth();

        let mut request = load_request(&env, request_id);
        if request.merchant != merchant {
            panic_with_error!(&env, WithdrawalPolicyError::NotWithdrawalRequester);
        }
        if request.status != LargeWithdrawalStatus::Pending {
            panic_with_error!(&env, WithdrawalPolicyError::WithdrawalRequestNotPending);
        }

        let key = DataKey::MerchantCollateral(merchant.clone());
        let current: i128 = env.storage().persistent().get(&key).unwrap_or(0);
        persist(&env, &key, &current.saturating_add(request.amount));

        request.status = LargeWithdrawalStatus::Cancelled;
        persist(&env, &WithdrawalPolicyKey::Request(request_id), &request);
        LargeWithdrawalCancelled {
            request_id,
            merchant,
            amount: request.amount,
        }
        .publish(&env);
    }

    /// #1169: A queued/processed large withdrawal request.
    pub fn get_large_withdrawal(env: Env, request_id: u32) -> LargeWithdrawal {
        load_request(&env, request_id)
    }
}
