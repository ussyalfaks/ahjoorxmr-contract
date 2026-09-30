use soroban_sdk::{contractevent, Address, BytesN, Env, String, Symbol, Vec};

/// Event: Escrow created
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowCreated {
    pub escrow_id: u32,
    pub buyer: Address,
    pub seller: Address,
    pub arbiter: Address,
    pub amount: i128,
    pub token: Address,
    pub deadline: u64,
}

/// Event: Escrow creation includes a lock window
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowTimeLocked {
    pub escrow_id: u32,
    pub locked_until: u64,
}

/// Event: Batch escrow created summary
#[contractevent]
#[derive(Clone, Debug)]
pub struct BatchEscrowCreated {
    pub count: u32,
    pub first_id: u32,
    pub last_id: u32,
}

/// Event: Escrow released to seller
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowReleased {
    pub escrow_id: u32,
    pub seller: Address,
    pub amount: i128,
}

/// Event: Escrow partially released to seller
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialReleased {
    pub escrow_id: u32,
    pub released_amount: i128,
    pub remaining_amount: i128,
}

/// Event: Milestone approved and amount released to seller (#136)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MilestoneApproved {
    pub escrow_id: u32,
    pub milestone_index: u32,
    pub amount_released: i128,
}

/// Event: Escrow disputed
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowDisputed {
    pub escrow_id: u32,
    pub disputer: Address,
    pub reason: String,
}

/// Event: Partial dispute raised — undisputed portion released to seller
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialDisputeRaised {
    pub escrow_id: u32,
    pub dispute_amount: i128,
    pub released_amount: i128,
}

/// Event: Dispute resolved
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeResolved {
    pub escrow_id: u32,
    pub release_to_seller: bool,
    pub resolved_by: Address,
}

/// Event: Dispute resolved with a percentage split between buyer and seller
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeResolvedSplit {
    pub escrow_id: u32,
    pub buyer_percent: u32,
    pub seller_percent: u32,
    pub buyer_amount: i128,
    pub seller_amount: i128,
    pub resolved_by: Address,
}

/// Event: Dispute escalation threshold reached
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeEscalated {
    pub escrow_id: u32,
    pub timeout_seconds: u64,
}

/// Event: Protocol fee paid on dispute resolution
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowProtocolFeePaid {
    pub escrow_id: u32,
    pub fee_amount: i128,
    pub fee_recipient: Address,
}

/// Event: Arbiter fee paid on dispute resolution
#[contractevent]
#[derive(Clone, Debug)]
pub struct ArbiterFeePaid {
    pub escrow_id: u32,
    pub arbiter: Address,
    pub fee_amount: i128,
}

/// Event: Insurance payout claimed
#[contractevent]
#[derive(Clone, Debug)]
pub struct InsuranceClaimed {
    pub escrow_id: u32,
    pub claimant: Address,
    pub amount: i128,
}

/// Event: Oracle-triggered release executed
#[contractevent]
#[derive(Clone, Debug)]
pub struct OracleReleaseTriggered {
    pub escrow_id: u32,
    pub oracle_price: i128,
    pub base: Address,
    pub quote: Address,
    pub comparison: u32,
    pub threshold_price: i128,
}

/// Event: Escrow refunded to buyer
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowRefunded {
    pub escrow_id: u32,
    pub buyer: Address,
    pub amount: i128,
}

/// Event: Multi-seller escrow released with distributions (#317)
#[contractevent]
#[derive(Clone, Debug)]
pub struct MultiSellerEscrowReleased {
    pub escrow_id: u32,
    pub distributions: Vec<(Address, i128)>,
}

/// Event: Conditional release triggered by oracle (#318)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ConditionalReleaseTriggered {
    pub escrow_id: u32,
    pub oracle_contract: Address,
    pub condition_value: i128,
}

/// Event: Release condition waived by mutual agreement (#318)
#[contractevent]
#[derive(Clone, Debug)]
pub struct ReleaseConditionWaived {
    pub escrow_id: u32,
}

/// Event: Contract WASM upgraded
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractUpgraded {
    pub old_version: u32,
    pub new_version: u32,
    pub by_admin: Address,
}

/// Event: Deadline extension proposed by a participant
#[contractevent]
#[derive(Clone, Debug)]
pub struct DeadlineExtensionProposed {
    pub escrow_id: u32,
    pub proposer: Address,
    pub new_deadline: u64,
    pub proposed_at: u64,
}

/// Event: Deadline updated after counterparty acceptance
#[contractevent]
#[derive(Clone, Debug)]
pub struct DeadlineExtended {
    pub escrow_id: u32,
    pub old_deadline: u64,
    pub new_deadline: u64,
}

/// Event: Contract paused
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractPaused {
    pub admin: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Event: Contract resumed
#[contractevent]
#[derive(Clone, Debug)]
pub struct ContractResumed {
    pub admin: Address,
    pub timestamp: u64,
}

/// Event: Token Allowlisted
#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenAllowlisted {
    pub admin: Address,
    pub token: Address,
}

/// Event: Token Removed From Allowlist
#[contractevent]
#[derive(Clone, Debug)]
pub struct TokenRemovedFromAllowlist {
    pub admin: Address,
    pub token: Address,
}

/// Event: Arbiter added to or removed from the pool
#[contractevent]
#[derive(Clone, Debug)]
pub struct ArbiterPoolUpdated {
    pub arbiter: Address,
    pub added: bool,
}

/// Event: Arbiter assigned to an escrow via pool round-robin
#[contractevent]
#[derive(Clone, Debug)]
pub struct ArbiterAssigned {
    pub escrow_id: u32,
    pub arbiter: Address,
}

/// Event: Evidence hash submitted for a dispute
#[contractevent]
#[derive(Clone, Debug)]
pub struct EvidenceSubmitted {
    pub escrow_id: u32,
    pub party: Address,
    pub evidence_hash: BytesN<32>,
}

/// Event: Escrow renewed automatically on release
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowAutoRenewed {
    pub old_escrow_id: u32,
    pub new_escrow_id: u32,
    pub renewals_remaining: u32,
}

/// Event: Buyer role transferred to another address
#[contractevent]
#[derive(Clone, Debug)]
pub struct BuyerRoleTransferred {
    pub escrow_id: u32,
    pub old_buyer: Address,
    pub new_buyer: Address,
}

// --- Helper Emission Functions ---

pub fn emit_escrow_created(
    e: &Env,
    escrow_id: u32,
    buyer: Address,
    seller: Address,
    arbiter: Address,
    amount: i128,
    token: Address,
    deadline: u64,
) {
    EscrowCreated {
        escrow_id,
        buyer,
        seller,
        arbiter,
        amount,
        token,
        deadline,
    }
    .publish(e);
}

pub fn emit_escrow_time_locked(e: &Env, escrow_id: u32, locked_until: u64) {
    EscrowTimeLocked {
        escrow_id,
        locked_until,
    }
    .publish(e);
}

pub fn emit_batch_escrow_created(e: &Env, count: u32, first_id: u32, last_id: u32) {
    BatchEscrowCreated {
        count,
        first_id,
        last_id,
    }
    .publish(e);
}

pub fn emit_escrow_released(e: &Env, escrow_id: u32, seller: Address, amount: i128) {
    EscrowReleased {
        escrow_id,
        seller,
        amount,
    }
    .publish(e);
}

/// Event: Escrow receipt minted and assigned to seller
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowReceiptMinted {
    pub receipt_id: u32,
    pub escrow_id: u32,
    pub seller: Address,
}

/// Event: Escrow receipt transferred to new holder
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowReceiptTransferred {
    pub receipt_id: u32,
    pub from: Address,
    pub to: Address,
}

/// Event: Escrow receipt burned (on release or cancel)
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowReceiptBurned {
    pub receipt_id: u32,
    pub escrow_id: u32,
}

pub fn emit_escrow_receipt_minted(e: &Env, receipt_id: u32, escrow_id: u32, seller: Address) {
    EscrowReceiptMinted {
        receipt_id,
        escrow_id,
        seller,
    }
    .publish(e);
}

pub fn emit_escrow_receipt_transferred(e: &Env, receipt_id: u32, from: Address, to: Address) {
    EscrowReceiptTransferred {
        receipt_id,
        from,
        to,
    }
    .publish(e);
}

pub fn emit_escrow_receipt_burned(e: &Env, receipt_id: u32, escrow_id: u32) {
    EscrowReceiptBurned { receipt_id, escrow_id }.publish(e);
}

pub fn emit_partial_released(
    e: &Env,
    escrow_id: u32,
    released_amount: i128,
    remaining_amount: i128,
) {
    PartialReleased {
        escrow_id,
        released_amount,
        remaining_amount,
    }
    .publish(e);
}

pub fn emit_milestone_approved(
    e: &Env,
    escrow_id: u32,
    milestone_index: u32,
    amount_released: i128,
) {
    MilestoneApproved {
        escrow_id,
        milestone_index,
        amount_released,
    }
    .publish(e);
}

pub fn emit_escrow_disputed(e: &Env, escrow_id: u32, disputer: Address, reason: String) {
    EscrowDisputed {
        escrow_id,
        disputer,
        reason,
    }
    .publish(e);
}

pub fn emit_partial_dispute_raised(
    e: &Env,
    escrow_id: u32,
    dispute_amount: i128,
    released_amount: i128,
) {
    PartialDisputeRaised {
        escrow_id,
        dispute_amount,
        released_amount,
    }
    .publish(e);
}

pub fn emit_dispute_resolved(
    e: &Env,
    escrow_id: u32,
    release_to_seller: bool,
    resolved_by: Address,
) {
    DisputeResolved {
        escrow_id,
        release_to_seller,
        resolved_by,
    }
    .publish(e);
}

pub fn emit_dispute_resolved_split(
    e: &Env,
    escrow_id: u32,
    buyer_percent: u32,
    seller_percent: u32,
    buyer_amount: i128,
    seller_amount: i128,
    resolved_by: Address,
) {
    DisputeResolvedSplit {
        escrow_id,
        buyer_percent,
        seller_percent,
        buyer_amount,
        seller_amount,
        resolved_by,
    }
    .publish(e);
}

pub fn emit_dispute_escalated(e: &Env, escrow_id: u32, timeout_seconds: u64) {
    DisputeEscalated {
        escrow_id,
        timeout_seconds,
    }
    .publish(e);
}

pub fn emit_protocol_fee_paid(e: &Env, escrow_id: u32, fee_amount: i128, fee_recipient: Address) {
    EscrowProtocolFeePaid {
        escrow_id,
        fee_amount,
        fee_recipient,
    }
    .publish(e);
}

pub fn emit_arbiter_fee_paid(e: &Env, escrow_id: u32, arbiter: Address, fee_amount: i128) {
    ArbiterFeePaid {
        escrow_id,
        arbiter,
        fee_amount,
    }
    .publish(e);
}

pub fn emit_insurance_claimed(e: &Env, escrow_id: u32, claimant: Address, amount: i128) {
    InsuranceClaimed {
        escrow_id,
        claimant,
        amount,
    }
    .publish(e);
}

pub fn emit_oracle_release_triggered(
    e: &Env,
    escrow_id: u32,
    oracle_price: i128,
    base: Address,
    quote: Address,
    comparison: u32,
    threshold_price: i128,
) {
    OracleReleaseTriggered {
        escrow_id,
        oracle_price,
        base,
        quote,
        comparison,
        threshold_price,
    }
    .publish(e);
}

pub fn emit_escrow_refunded(e: &Env, escrow_id: u32, buyer: Address, amount: i128) {
    EscrowRefunded {
        escrow_id,
        buyer,
        amount,
    }
    .publish(e);
}

pub fn emit_contract_upgraded(e: &Env, old_version: u32, new_version: u32, by_admin: Address) {
    ContractUpgraded {
        old_version,
        new_version,
        by_admin,
    }
    .publish(e);
}
pub fn emit_deadline_extension_proposed(
    e: &Env,
    escrow_id: u32,
    proposer: Address,
    new_deadline: u64,
    proposed_at: u64,
) {
    DeadlineExtensionProposed {
        escrow_id,
        proposer,
        new_deadline,
        proposed_at,
    }
    .publish(e);
}

pub fn emit_contract_paused(e: &Env, admin: Address, reason: String, timestamp: u64) {
    ContractPaused {
        admin,
        reason,
        timestamp,
    }
    .publish(e);
}

pub fn emit_deadline_extended(e: &Env, escrow_id: u32, old_deadline: u64, new_deadline: u64) {
    DeadlineExtended {
        escrow_id,
        old_deadline,
        new_deadline,
    }
    .publish(e);
}

pub fn emit_contract_resumed(e: &Env, admin: Address, timestamp: u64) {
    ContractResumed { admin, timestamp }.publish(e);
}

pub fn emit_token_allowlisted(e: &Env, admin: Address, token: Address) {
    TokenAllowlisted { admin, token }.publish(e);
}

pub fn emit_token_removed_from_allowlist(e: &Env, admin: Address, token: Address) {
    TokenRemovedFromAllowlist { admin, token }.publish(e);
}

/// Event: Escrow template created
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowTemplateCreated {
    pub template_id: u32,
    pub creator: Address,
}

/// Event: Escrow template config updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowTemplateUpdated {
    pub template_id: u32,
    pub creator: Address,
}

/// Event: Escrow template deactivated
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowTemplateDeactivated {
    pub template_id: u32,
    pub creator: Address,
}

/// Event: Escrow created from a template
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowCreatedFromTemplate {
    pub escrow_id: u32,
    pub template_id: u32,
}

pub fn emit_escrow_template_created(e: &Env, template_id: u32, creator: Address) {
    EscrowTemplateCreated { template_id, creator }.publish(e);
}

pub fn emit_escrow_template_updated(e: &Env, template_id: u32, creator: Address) {
    EscrowTemplateUpdated { template_id, creator }.publish(e);
}

pub fn emit_escrow_template_deactivated(e: &Env, template_id: u32, creator: Address) {
    EscrowTemplateDeactivated { template_id, creator }.publish(e);
}

pub fn emit_escrow_created_from_template(e: &Env, escrow_id: u32, template_id: u32) {
    EscrowCreatedFromTemplate { escrow_id, template_id }.publish(e);
}

pub fn emit_arbiter_pool_updated(e: &Env, arbiter: Address, added: bool) {
    ArbiterPoolUpdated { arbiter, added }.publish(e);
}

pub fn emit_arbiter_assigned(e: &Env, escrow_id: u32, arbiter: Address) {
    ArbiterAssigned { escrow_id, arbiter }.publish(e);
}

pub fn emit_evidence_submitted(
    e: &Env,
    escrow_id: u32,
    party: Address,
    evidence_hash: BytesN<32>,
) {
    EvidenceSubmitted {
        escrow_id,
        party,
        evidence_hash,
    }
    .publish(e);
}

pub fn emit_escrow_auto_renewed(
    e: &Env,
    old_escrow_id: u32,
    new_escrow_id: u32,
    renewals_remaining: u32,
) {
    EscrowAutoRenewed {
        old_escrow_id,
        new_escrow_id,
        renewals_remaining,
    }
    .publish(e);
}

// --- Auto-Renewal Clause Events (recurring service agreements) ---

/// Event: Escrow auto-renewed with renewal index tracking.
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowAutoRenewedV2 {
    pub original_escrow_id: u32,
    pub new_escrow_id: u32,
    pub renewal_index: u32,
}

/// Event: Auto-renewal failed (e.g. insufficient buyer allowance).
#[contractevent]
#[derive(Clone, Debug)]
pub struct RenewalFailed {
    pub escrow_id: u32,
    pub renewal_index: u32,
    pub reason: String,
}

/// Event: Buyer cancelled future auto-renewals for an escrow.
#[contractevent]
#[derive(Clone, Debug)]
pub struct AutoRenewalCancelled {
    pub escrow_id: u32,
}

pub fn emit_escrow_auto_renewed_v2(
    e: &Env,
    original_escrow_id: u32,
    new_escrow_id: u32,
    renewal_index: u32,
) {
    EscrowAutoRenewedV2 {
        original_escrow_id,
        new_escrow_id,
        renewal_index,
    }
    .publish(e);
}

pub fn emit_renewal_failed(e: &Env, escrow_id: u32, renewal_index: u32, reason: String) {
    RenewalFailed {
        escrow_id,
        renewal_index,
        reason,
    }
    .publish(e);
}

pub fn emit_auto_renewal_cancelled(e: &Env, escrow_id: u32) {
    AutoRenewalCancelled { escrow_id }.publish(e);
}

pub fn emit_buyer_role_transferred(
    e: &Env,
    escrow_id: u32,
    old_buyer: Address,
    new_buyer: Address,
) {
    BuyerRoleTransferred {
        escrow_id,
        old_buyer,
        new_buyer,
    }
    .publish(e);
}

// --- Issue #145: Escrow Metadata ---

/// Event: Escrow metadata hash updated
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowMetadataUpdated {
    pub escrow_id: u32,
    pub new_hash: BytesN<32>,
    pub updated_by: Address,
}

pub fn emit_escrow_metadata_updated(
    e: &Env,
    escrow_id: u32,
    new_hash: BytesN<32>,
    updated_by: Address,
) {
    EscrowMetadataUpdated {
        escrow_id,
        new_hash,
        updated_by,
    }
    .publish(e);
}

// --- Issue #148: Multi-Party Escrow ---

/// Event: Multi-party escrow created
#[contractevent]
#[derive(Clone, Debug)]
pub struct MultiPartyEscrowCreated {
    pub escrow_id: u32,
    pub buyer_count: u32,
    pub total_amount: i128,
}

pub fn emit_multi_party_escrow_created(e: &Env, escrow_id: u32, buyer_count: u32, total_amount: i128) {
    MultiPartyEscrowCreated {
        escrow_id,
        buyer_count,
        total_amount,
    }
    .publish(e);
}

// --- Issue #150: Seller-Favored Auto-Release on Prolonged Buyer Inactivity ---

/// Event: Inactivity release claimed by seller after buyer inactivity window
#[contractevent]
#[derive(Clone, Debug)]
pub struct InactivityReleaseTriggered {
    pub escrow_id: u32,
    pub seller: Address,
    pub inactivity_seconds: u64,
}

pub fn emit_inactivity_release_triggered(
    e: &Env,
    escrow_id: u32,
    seller: Address,
    inactivity_seconds: u64,
) {
    InactivityReleaseTriggered {
        escrow_id,
        seller,
        inactivity_seconds,
    }
    .publish(e);
}

// --- Issue #151: Dispute Timeout Mechanism ---

/// Event: Dispute timed out and funds released to default winner
#[contractevent]
#[derive(Clone, Debug)]
pub struct DisputeTimedOut {
    pub escrow_id: u32,
    pub arbiter: Address,
    pub default_winner: u32, // 0 = Buyer, 1 = Seller
    pub elapsed_seconds: u64,
}

/// Event: Arbiter timeout penalty applied
#[contractevent]
#[derive(Clone, Debug)]
pub struct ArbiterTimeoutPenaltyApplied {
    pub arbiter: Address,
    pub total_timeouts: u32,
}

pub fn emit_dispute_timed_out(
    e: &Env,
    escrow_id: u32,
    arbiter: Address,
    default_winner: crate::DisputeDefaultWinner,
    elapsed_seconds: u64,
) {
    DisputeTimedOut {
        escrow_id,
        arbiter,
        default_winner: default_winner as u32,
        elapsed_seconds,
    }
    .publish(e);
}

pub fn emit_arbiter_timeout_penalty_applied(e: &Env, arbiter: Address, total_timeouts: u32) {
    ArbiterTimeoutPenaltyApplied {
        arbiter,
        total_timeouts,
    }
    .publish(e);
}

// #215: Time-locked escrow events
pub fn emit_timelocked_escrow_created(e: &Env, escrow_id: u32, unlock_at: u64, beneficiary: Address) {
    e.events().publish((Symbol::new(e, "TLEscrowCreated"),), (escrow_id, unlock_at, beneficiary));
}
pub fn emit_timelocked_funds_claimed(e: &Env, escrow_id: u32, beneficiary: Address, amount: i128) {
    e.events().publish((Symbol::new(e, "TLFundsClaimed"),), (escrow_id, beneficiary, amount));
}
pub fn emit_timelocked_escrow_cancelled(e: &Env, escrow_id: u32, buyer: Address) {
    e.events().publish((Symbol::new(e, "TLEscrowCancelled"),), (escrow_id, buyer));
}

// #229: Mutual Cancellation events
pub fn emit_cancellation_requested(e: &Env, escrow_id: u32, initiator: Address, expires_at: u64) {
    e.events().publish((Symbol::new(e, "CancelRequested"),), (escrow_id, initiator, expires_at));
}
pub fn emit_cancellation_accepted(e: &Env, escrow_id: u32, buyer: Address, amount_returned: i128, penalty: i128) {
    e.events().publish((Symbol::new(e, "CancelAccepted"),), (escrow_id, buyer, amount_returned, penalty));
}
pub fn emit_cancellation_rejected(e: &Env, escrow_id: u32, rejector: Address) {
    e.events().publish((Symbol::new(e, "CancelRejected"),), (escrow_id, rejector));
}
pub fn emit_cancellation_expired(e: &Env, escrow_id: u32) {
    e.events().publish((Symbol::new(e, "CancelExpired"),), (escrow_id,));
}

// #225: Escrow Top-Up Event

/// Event: Buyer topped up an active escrow with additional funds
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowToppedUp {
    pub escrow_id: u32,
    pub added_amount: i128,
    pub new_total: i128,
    pub buyer: Address,
}

pub fn emit_escrow_topped_up(e: &Env, escrow_id: u32, added_amount: i128, new_total: i128, buyer: Address) {
    EscrowToppedUp { escrow_id, added_amount, new_total, buyer }.publish(e);
}

// --- Issue #237: Seller Performance Collateral ---

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralDeposited {
    pub escrow_id: u32,
    pub seller: Address,
    pub amount: i128,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralForfeited {
    pub escrow_id: u32,
    pub amount: i128,
    pub awarded_to_buyer: Address,
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct CollateralReturned {
    pub escrow_id: u32,
    pub seller: Address,
    pub amount: i128,
}

pub fn emit_collateral_deposited(e: &Env, escrow_id: u32, seller: Address, amount: i128) {
    CollateralDeposited { escrow_id, seller, amount }.publish(e);
}
pub fn emit_collateral_forfeited(e: &Env, escrow_id: u32, amount: i128, awarded_to_buyer: Address) {
    CollateralForfeited { escrow_id, amount, awarded_to_buyer }.publish(e);
}
pub fn emit_collateral_returned(e: &Env, escrow_id: u32, seller: Address, amount: i128) {
    CollateralReturned { escrow_id, seller, amount }.publish(e);
}

// --- Issue #361: Collateral Health Alert ---

pub fn emit_collateral_health_alert(e: &Env, escrow_id: u32, current_ratio_bps: u32, required_ratio_bps: u32) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "CollateralHealthAlert"),),
        (escrow_id, current_ratio_bps, required_ratio_bps),
    );
}

// --- Issue #241: Delivery Proof Hash ---

/// Event: Seller submitted a delivery proof; auto_released indicates if escrow was released.
#[contractevent]
#[derive(Clone, Debug)]
pub struct DeliveryProofSubmitted {
    pub escrow_id: u32,
    pub seller: Address,
    pub proof_hash: BytesN<32>,
    pub auto_released: bool,
}

pub fn emit_delivery_proof_submitted(
    e: &Env,
    escrow_id: u32,
    seller: Address,
    proof_hash: BytesN<32>,
    auto_released: bool,
) {
    DeliveryProofSubmitted {
        escrow_id,
        seller,
        proof_hash,
        auto_released,
    }
    .publish(e);
}

// #244: Seller Role Transfer Veto Events

pub fn emit_seller_transfer_proposed(e: &Env, escrow_id: u32, original_seller: Address, new_seller: Address, veto_deadline: u32) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "SellerTransferProposed"),),
        (escrow_id, original_seller, new_seller, veto_deadline),
    );
}

pub fn emit_seller_transfer_vetoed(e: &Env, escrow_id: u32, buyer: Address, refund_amount: i128) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "SellerTransferVetoed"),),
        (escrow_id, buyer, refund_amount),
    );
}

pub fn emit_seller_transfer_approved(e: &Env, escrow_id: u32, new_seller: Address) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "SellerTransferApproved"),),
        (escrow_id, new_seller),
    );
}

pub fn emit_seller_transfer_expired_approved(e: &Env, escrow_id: u32, new_seller: Address) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "SellerTransferExpired"),),
        (escrow_id, new_seller),
    );
}

// --- Issue #146: Post-Resolution Rating System ---

/// Event: Rating submitted after escrow completion
#[contractevent]
#[derive(Clone, Debug)]
pub struct RatingSubmitted {
    pub escrow_id: u32,
    pub rater: Address,
    pub ratee: Address,
    pub rating: u32,
    pub comment_hash: Option<BytesN<32>>,
}

pub fn emit_rating_submitted(
    e: &Env,
    escrow_id: u32,
    rater: Address,
    ratee: Address,
    rating: u32,
    comment_hash: Option<BytesN<32>>,
) {
    RatingSubmitted {
        escrow_id,
        rater,
        ratee,
        rating,
        comment_hash,
    }
    .publish(e);
}

// --- Resolution Cooling-Off events ---

pub fn emit_resolution_cooling_off(
    e: &Env,
    escrow_id: u32,
    buyer_percent: u32,
    arbiter: Address,
    cooling_off_until: u64,
) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "ResolutionCoolingOff"),),
        (escrow_id, buyer_percent, arbiter, cooling_off_until),
    );
}

pub fn emit_resolution_flagged(e: &Env, escrow_id: u32, caller: Address, reason_hash: soroban_sdk::BytesN<32>) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "ResolutionFlagged"),),
        (escrow_id, caller, reason_hash),
    );
}

pub fn emit_resolution_finalized(e: &Env, escrow_id: u32, buyer_percent: u32, arbiter: Address) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "ResolutionFinalized"),),
        (escrow_id, buyer_percent, arbiter),
    );
}

// --- Mutual Amendment Protocol Events ---

/// Event: Amendment proposed by buyer or seller
#[contractevent]
#[derive(Clone, Debug)]
pub struct AmendmentProposed {
    pub escrow_id: u32,
    pub nonce: u32,
    pub proposer: Address,
    pub expires_at: u64,
}

/// Event: Amendment signed by a party
#[contractevent]
#[derive(Clone, Debug)]
pub struct AmendmentSigned {
    pub escrow_id: u32,
    pub nonce: u32,
    pub signer: Address,
}

/// Event: Amendment applied after both parties signed
#[contractevent]
#[derive(Clone, Debug)]
pub struct AmendmentApplied {
    pub escrow_id: u32,
    pub nonce: u32,
    pub old_amount: i128,
    pub new_amount: i128,
    pub old_deadline: u64,
    pub new_deadline: u64,
}

/// Event: Amendment proposal cancelled
#[contractevent]
#[derive(Clone, Debug)]
pub struct AmendmentCancelled {
    pub escrow_id: u32,
    pub nonce: u32,
    pub cancelled_by: Address,
}

#[allow(clippy::too_many_arguments)]
pub fn emit_amendment_proposed(
    e: &Env,
    escrow_id: u32,
    nonce: u32,
    proposer: Address,
    _new_amount: Option<i128>,
    _new_deadline: Option<u64>,
    _new_metadata_hash: Option<BytesN<32>>,
    expires_at: u64,
) {
    AmendmentProposed {
        escrow_id,
        nonce,
        proposer,
        expires_at,
    }
    .publish(e);
}

pub fn emit_amendment_signed(e: &Env, escrow_id: u32, nonce: u32, signer: Address) {
    AmendmentSigned {
        escrow_id,
        nonce,
        signer,
    }
    .publish(e);
}

#[allow(clippy::too_many_arguments)]
pub fn emit_amendment_applied(
    e: &Env,
    escrow_id: u32,
    nonce: u32,
    old_amount: i128,
    new_amount: i128,
    old_deadline: u64,
    new_deadline: u64,
    _old_metadata_hash: Option<BytesN<32>>,
    _new_metadata_hash: Option<BytesN<32>>,
) {
    AmendmentApplied {
        escrow_id,
        nonce,
        old_amount,
        new_amount,
        old_deadline,
        new_deadline,
    }
    .publish(e);
}

pub fn emit_amendment_cancelled(e: &Env, escrow_id: u32, nonce: u32, cancelled_by: Address) {
    AmendmentCancelled {
        escrow_id,
        nonce,
        cancelled_by,
    }
    .publish(e);
}

pub fn emit_multi_seller_escrow_released(
    env: &Env,
    escrow_id: u32,
    distributions: Vec<(Address, i128)>,
) {
    MultiSellerEscrowReleased {
        escrow_id,
        distributions,
    }
    .publish(env);
}

pub fn emit_conditional_release_triggered(
    env: &Env,
    escrow_id: u32,
    oracle_contract: Address,
    condition_value: i128,
) {
    ConditionalReleaseTriggered {
        escrow_id,
        oracle_contract,
        condition_value,
    }
    .publish(env);
}

pub fn emit_release_condition_waived(env: &Env, escrow_id: u32) {
    ReleaseConditionWaived { escrow_id }.publish(env);
}

// --- #272/#357: Inspector Events ---

/// Event: Seller marked work complete; awaiting inspector (#272)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SellerMarkedComplete {
    pub escrow_id: u32,
    pub seller: Address,
}

/// Event: Inspector replaced by mutual agreement (#272)
#[contractevent]
#[derive(Clone, Debug)]
pub struct InspectorReplaced {
    pub escrow_id: u32,
    pub old_inspector: Address,
    pub new_inspector: Address,
}

/// Event: Inspection report submitted by inspector (#272)
#[contractevent]
#[derive(Clone, Debug)]
pub struct InspectionReportSubmitted {
    pub escrow_id: u32,
    pub inspector: Address,
    pub approved: bool,
    pub report_hash: BytesN<32>,
}

/// Event: Inspector reputation score updated (#357)
#[contractevent]
#[derive(Clone, Debug)]
pub struct InspectorScoreUpdated {
    pub inspector: Address,
    pub total_rulings: u32,
    pub correct_rulings: u32,
    pub accuracy_bps: u32,
}

pub fn emit_seller_marked_complete(e: &Env, escrow_id: u32, seller: Address) {
    SellerMarkedComplete { escrow_id, seller }.publish(e);
}

pub fn emit_inspector_replaced(e: &Env, escrow_id: u32, old_inspector: Address, new_inspector: Address) {
    InspectorReplaced { escrow_id, old_inspector, new_inspector }.publish(e);
}

pub fn emit_inspection_report_submitted(
    e: &Env,
    escrow_id: u32,
    inspector: Address,
    approved: bool,
    report_hash: BytesN<32>,
) {
    InspectionReportSubmitted { escrow_id, inspector, approved, report_hash }.publish(e);
}

pub fn emit_inspector_score_updated(
    e: &Env,
    inspector: Address,
    total_rulings: u32,
    correct_rulings: u32,
    accuracy_bps: u32,
) {
    InspectorScoreUpdated { inspector, total_rulings, correct_rulings, accuracy_bps }.publish(e);
}

// ── #332: Milestone BPS Events ────────────────────────────────────────────────

pub fn emit_milestone_submitted(e: &Env, escrow_id: u32, milestone_index: u32, delivery_hash: BytesN<32>) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "MilestoneSubmitted"),),
        (escrow_id, milestone_index, delivery_hash),
    );
}

pub fn emit_milestone_rejected(e: &Env, escrow_id: u32, milestone_index: u32) {
    e.events().publish(
        (soroban_sdk::Symbol::new(e, "MilestoneRejected"),),
        (escrow_id, milestone_index),
    );
}

/// Event: Seller acknowledged top-up
#[contractevent]
#[derive(Clone, Debug)]
pub struct TopUpAcknowledged {
    pub escrow_id: u32,
    pub seller: Address,
    pub new_total: i128,
}

pub fn emit_top_up_acknowledged(
    e: &Env,
    escrow_id: u32,
    seller: Address,
    new_total: i128,
) {
    TopUpAcknowledged {
        escrow_id,
        seller,
        new_total,
    }
    .publish(e);
}

/// Event: Partial release requested by seller
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialReleaseRequested {
    pub escrow_id: u32,
    pub request_id: u64,
    pub seller: Address,
    pub amount: i128,
}

pub fn emit_partial_release_requested(
    e: &Env,
    escrow_id: u32,
    request_id: u64,
    seller: Address,
    amount: i128,
) {
    PartialReleaseRequested {
        escrow_id,
        request_id,
        seller,
        amount,
    }
    .publish(e);
}

/// Event: Partial release approved by buyer
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialReleaseApproved {
    pub escrow_id: u32,
    pub request_id: u64,
    pub amount: i128,
}

pub fn emit_partial_release_approved(
    e: &Env,
    escrow_id: u32,
    request_id: u64,
    amount: i128,
) {
    PartialReleaseApproved {
        escrow_id,
        request_id,
        amount,
    }
    .publish(e);
}

/// Event: Partial release rejected by buyer
#[contractevent]
#[derive(Clone, Debug)]
pub struct PartialReleaseRejected {
    pub escrow_id: u32,
    pub request_id: u64,
}

pub fn emit_partial_release_rejected(
    e: &Env,
    escrow_id: u32,
    request_id: u64,
) {
    PartialReleaseRejected {
        escrow_id,
        request_id,
    }
    .publish(e);
}

// --- Issue #420: Seller Veto Cooldown ---

/// Event: Seller raised a veto to block fund release.
// ── #319: Bounty Board Events ─────────────────────────────────────────────────

/// Event: Bounty created with open competitive work assignment
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyCreated {
    pub escrow_id: u32,
    pub buyer: Address,
    pub amount: i128,
    pub token: Address,
    pub description_hash: BytesN<32>,
    pub claim_deadline_ledger: u64,
    pub submission_deadline_ledger: u64,
}

/// Event: Bounty claimed by a solver
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyClaimed {
    pub escrow_id: u32,
    pub solver: Address,
}

/// Event: Bounty work submitted by solver
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyWorkSubmitted {
    pub escrow_id: u32,
    pub solver: Address,
    pub submission_hash: BytesN<32>,
}

/// Event: Bounty submission approved and funds awarded to solver
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyAwarded {
    pub escrow_id: u32,
    pub solver: Address,
    pub amount: i128,
}

/// Event: Bounty submission rejected and bounty re-opened
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyRejected {
    pub escrow_id: u32,
    pub solver: Address,
    pub rejection_count: u32,
}

/// Event: Bounty cancelled by buyer (no claims or after max rejections)
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyCancelled {
    pub escrow_id: u32,
    pub buyer: Address,
    pub refund_amount: i128,
}

pub fn emit_bounty_created(
    e: &Env,
    escrow_id: u32,
    buyer: Address,
    amount: i128,
    token: Address,
    description_hash: BytesN<32>,
    claim_deadline_ledger: u64,
    submission_deadline_ledger: u64,
) {
    BountyCreated {
        escrow_id,
        buyer,
        amount,
        token,
        description_hash,
        claim_deadline_ledger,
        submission_deadline_ledger,
    }
    .publish(e);
}

pub fn emit_bounty_claimed(e: &Env, escrow_id: u32, solver: Address) {
    BountyClaimed { escrow_id, solver }.publish(e);
}

pub fn emit_bounty_work_submitted(
    e: &Env,
    escrow_id: u32,
    solver: Address,
    submission_hash: BytesN<32>,
) {
    BountyWorkSubmitted {
        escrow_id,
        solver,
        submission_hash,
    }
    .publish(e);
}

pub fn emit_bounty_awarded(e: &Env, escrow_id: u32, solver: Address, amount: i128) {
    BountyAwarded {
        escrow_id,
        solver,
        amount,
    }
    .publish(e);
}

pub fn emit_bounty_rejected(e: &Env, escrow_id: u32, solver: Address, rejection_count: u32) {
    BountyRejected {
        escrow_id,
        solver,
        rejection_count,
    }
    .publish(e);
}

pub fn emit_bounty_cancelled(e: &Env, escrow_id: u32, buyer: Address, refund_amount: i128) {
    BountyCancelled {
        escrow_id,
        buyer,
        refund_amount,
    }
    .publish(e);
}

// ── #350: Multi-Party N-of-M Release Approval ─────────────────────────────────

/// Event: An approver signed off on releasing escrow funds
#[contractevent]
#[derive(Clone, Debug)]
pub struct MultiPartyApproval {
    pub escrow_id: u32,
    pub approver: Address,
    pub approvals_count: u32,
    pub threshold: u32,
}

pub fn emit_multi_party_approval(
    e: &Env,
    escrow_id: u32,
    approver: Address,
    approvals_count: u32,
    threshold: u32,
) {
    MultiPartyApproval {
        escrow_id,
        approver,
        approvals_count,
        threshold,
    }
    .publish(e);
}

// ── #353: Time-Locked Staged Release Events ───────────────────────────────────

/// Event: A scheduled release tranche was claimed by the beneficiary
#[contractevent]
#[derive(Clone, Debug)]
pub struct ScheduledReleaseExecuted {
    pub escrow_id: u32,
    pub tranche_index: u32,
    pub amount: i128,
    pub remaining_tranches: u32,
}

pub fn emit_scheduled_release_executed(
    e: &Env,
    escrow_id: u32,
    tranche_index: u32,
    amount: i128,
    remaining_tranches: u32,
) {
    ScheduledReleaseExecuted {
        escrow_id,
        tranche_index,
        amount,
        remaining_tranches,
    }
    .publish(e);
}

// ── #366: Seller Veto Override Events ────────────────────────────────────────

/// Event: Seller raised a veto blocking fund release (#366)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SellerVetoRaised {
    pub escrow_id: u32,
    pub seller: Address,
    pub veto_timestamp: u64,
}

/// Event: Seller cancelled their veto before the override window elapsed (#366)
#[contractevent]
#[derive(Clone, Debug)]
pub struct SellerVetoCancelled {
    pub escrow_id: u32,
    pub seller: Address,
}

/// Event: Admin overrode the seller veto after the window elapsed (#366)
#[contractevent]
#[derive(Clone, Debug)]
pub struct VetoOverridden {
    pub escrow_id: u32,
    pub admin: Address,
    pub overridden_at: u64,
}

pub fn emit_seller_veto_raised(e: &Env, escrow_id: u32, seller: Address, veto_timestamp: u64) {
    SellerVetoRaised { escrow_id, seller, veto_timestamp }.publish(e);
}

pub fn emit_seller_veto_cancelled(e: &Env, escrow_id: u32, seller: Address) {
    SellerVetoCancelled { escrow_id, seller }.publish(e);
}

pub fn emit_veto_overridden(e: &Env, escrow_id: u32, admin: Address, overridden_at: u64) {
    VetoOverridden { escrow_id, admin, overridden_at }.publish(e);
}

// ── #376: Bounty Board Milestone Gating Events ───────────────────────────────

/// Event: A milestone-gated bounty was created.
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyMilestoneCreated {
    pub escrow_id: u32,
    pub buyer: Address,
    pub milestone_count: u32,
    pub total_amount: i128,
}

/// Event: A bounty milestone deliverable was submitted by the solver.
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyMilestoneSubmitted {
    pub escrow_id: u32,
    pub index: u32,
    pub solver: Address,
    pub deliverable_hash: BytesN<32>,
}

/// Event: A bounty milestone was verified by its designated verifier and the
/// corresponding tranche released to the solver.
#[contractevent]
#[derive(Clone, Debug)]
pub struct MilestoneVerified {
    pub escrow_id: u32,
    pub index: u32,
    pub amount: i128,
    pub verifier: Address,
}

/// Event: The designated verifier for a still-pending milestone was replaced.
#[contractevent]
#[derive(Clone, Debug)]
pub struct BountyVerifierReplaced {
    pub escrow_id: u32,
    pub index: u32,
    pub old_verifier: Address,
    pub new_verifier: Address,
}

pub fn emit_bounty_milestone_created(
    e: &Env,
    escrow_id: u32,
    buyer: Address,
    milestone_count: u32,
    total_amount: i128,
) {
    BountyMilestoneCreated {
        escrow_id,
        buyer,
        milestone_count,
        total_amount,
    }
    .publish(e);
}

pub fn emit_bounty_milestone_submitted(
    e: &Env,
    escrow_id: u32,
    index: u32,
    solver: Address,
    deliverable_hash: BytesN<32>,
) {
    BountyMilestoneSubmitted {
        escrow_id,
        index,
        solver,
        deliverable_hash,
    }
    .publish(e);
}

pub fn emit_milestone_verified(
    e: &Env,
    escrow_id: u32,
    index: u32,
    amount: i128,
    verifier: Address,
) {
    MilestoneVerified {
        escrow_id,
        index,
        amount,
        verifier,
    }
    .publish(e);
}

pub fn emit_bounty_milestone_verifier_replaced(
    e: &Env,
    escrow_id: u32,
    index: u32,
    old_verifier: Address,
    new_verifier: Address,
) {
    BountyVerifierReplaced {
        escrow_id,
        index,
        old_verifier,
        new_verifier,
    }
    .publish(e);
}

// ── Fee Withdrawal Events ─────────────────────────────────────────────────

/// Event: Admin withdrew accumulated protocol fees
#[contractevent]
#[derive(Clone, Debug)]
pub struct FeesWithdrawn {
    pub amount: i128,
    pub destination: Address,
}

pub fn emit_fees_withdrawn(e: &Env, amount: i128, destination: Address) {
    FeesWithdrawn { amount, destination }.publish(e);
}

// ── #800 Work Authorization ───────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct WorkAuthorized {
    pub escrow_id: u32,
}

pub fn emit_work_authorized(e: &Env, escrow_id: u32) {
    WorkAuthorized { escrow_id }.publish(e);
}

// ── #797 Creation Bond ────────────────────────────────────────────────────────

#[contractevent]
#[derive(Clone, Debug)]
pub struct CreationBondRefunded {
    pub escrow_id: u32,
    pub buyer: Address,
    pub amount: i128,
}

pub fn emit_creation_bond_refunded(e: &Env, escrow_id: u32, buyer: Address, amount: i128) {
    CreationBondRefunded {
        escrow_id,
        buyer,
        amount,
    }
    .publish(e);
}

#[contractevent]
#[derive(Clone, Debug)]
pub struct AbandonmentBondClaimed {
    pub escrow_id: u32,
    pub seller: Address,
    pub amount: i128,
}

pub fn emit_abandonment_bond_claimed(e: &Env, escrow_id: u32, seller: Address, amount: i128) {
    AbandonmentBondClaimed {
        escrow_id,
        seller,
        amount,
    }
    .publish(e);
}

// ─── Third-Party Fee Sponsorship ─────────────────────────────────────────────

/// Event: A third party pre-funded the protocol fee for an escrow
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowFeeSponsored {
    pub escrow_id: u32,
    pub sponsor: Address,
    pub amount: i128,
}

pub fn emit_escrow_fee_sponsored(e: &Env, escrow_id: u32, sponsor: Address, amount: i128) {
    EscrowFeeSponsored {
        escrow_id,
        sponsor,
        amount,
    }
    .publish(e);
}

/// Event: An escrow fee sponsorship was settled — `fee_covered` was paid as
/// protocol fee and `returned` was sent back to the sponsor.
#[contractevent]
#[derive(Clone, Debug)]
pub struct EscrowFeeSponsorshipSettled {
    pub escrow_id: u32,
    pub sponsor: Address,
    pub fee_covered: i128,
    pub returned: i128,
}

pub fn emit_escrow_fee_sponsorship_settled(
    e: &Env,
    escrow_id: u32,
    sponsor: Address,
    fee_covered: i128,
    returned: i128,
) {
    EscrowFeeSponsorshipSettled {
        escrow_id,
        sponsor,
        fee_covered,
        returned,
    }
    .publish(e);
}

/// #977: Settlement offer proposed on a disputed escrow
#[contractevent]
#[derive(Clone, Debug)]
pub struct SettlementProposed {
    pub escrow_id: u32,
    pub proposer: Address,
    pub seller_amount: i128,
}

pub fn emit_settlement_proposed(e: &Env, escrow_id: u32, proposer: Address, seller_amount: i128) {
    SettlementProposed { escrow_id, proposer, seller_amount }.publish(e);
}

/// #977: Settlement offer accepted; dispute closed without arbiter
#[contractevent]
#[derive(Clone, Debug)]
pub struct SettlementAccepted {
    pub escrow_id: u32,
    pub accepter: Address,
    pub seller_amount: i128,
    pub buyer_amount: i128,
}

pub fn emit_settlement_accepted(e: &Env, escrow_id: u32, accepter: Address, seller_amount: i128, buyer_amount: i128) {
    SettlementAccepted { escrow_id, accepter, seller_amount, buyer_amount }.publish(e);
}

/// #978: Arbiter recused and replaced from the pool
#[contractevent]
#[derive(Clone, Debug)]
pub struct ArbiterRecused {
    pub escrow_id: u32,
    pub old_arbiter: Address,
    pub new_arbiter: Address,
    pub reason_hash: BytesN<32>,
}

pub fn emit_arbiter_recused(e: &Env, escrow_id: u32, old_arbiter: Address, new_arbiter: Address, reason_hash: BytesN<32>) {
    ArbiterRecused { escrow_id, old_arbiter, new_arbiter, reason_hash }.publish(e);
}
