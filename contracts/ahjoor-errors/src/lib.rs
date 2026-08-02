//! # ahjoor-errors — Global Error Code Namespace Registry
//!
//! Each Ahjoor protocol contract owns a non-overlapping numeric range so that
//! off-chain parsers can unambiguously decode `InvokeHostFunctionTrapped` errors
//! without per-contract decode tables.
//!
//! ## Range allocation
//!
//! | Contract              | Range       |
//! |----------------------|-------------|
//! | ahjoor-rosca         | 1000 – 1299 |
//! | ahjoor-payments      | 2000 – 2299 |
//! | ahjoor-escrow        | 3000 – 3299 |
//! | ahjoor-refund        | 4000 – 4099 |
//! | ahjoor-token-whitelist | 5000 – 5099 |
//!
//! On-chain contracts continue to use their existing small discriminants (1–118
//! for rosca, 1–56 for payments, etc.) because `#[contracterror]` must produce
//! values that fit in the Soroban XDR `ScError` u32 field and the existing enum
//! variants are already deployed.  This crate provides the *off-chain* namespace
//! that relay nodes and indexers use when decoding errors across contracts.

// ---------------------------------------------------------------------------
// ahjoor-rosca (1000–1299)
// ---------------------------------------------------------------------------

pub mod rosca {
    /// Number of `pub const` codes declared in this module. Kept in sync
    /// manually; `ahjoor-errors`'s test suite cross-checks this against the
    /// number of `ALL_ERRORS` entries tagged `"ahjoor-rosca"` so a code added
    /// here without a matching `ALL_ERRORS` entry (or vice versa) fails CI.
    pub const COUNT: usize = 110;

    // Core Error variants (on-chain discriminant → namespaced code)
    pub const ALREADY_INITIALIZED: u32         = 1001;
    pub const TOKEN_NOT_APPROVED: u32          = 1002;
    pub const CUSTOM_ORDER_LENGTH_MISMATCH: u32 = 1003;
    pub const CUSTOM_ORDER_NON_MEMBER: u32     = 1004;
    pub const AMOUNT_MUST_BE_POSITIVE: u32     = 1005;
    pub const ROUND_DEADLINE_PASSED: u32       = 1006;
    pub const MEMBER_HAS_EXITED: u32           = 1007;
    pub const NOT_A_MEMBER: u32                = 1008;
    pub const ALREADY_CONTRIBUTED: u32         = 1009;
    pub const INVALID_EXCHANGE_RATE: u32       = 1010;
    pub const EXCEEDS_TOKEN_LIMIT: u32         = 1011;
    pub const EXCEEDS_REMAINING_CONTRIBUTION: u32 = 1012;
    pub const DEADLINE_NOT_PASSED: u32         = 1013;
    pub const PENALTY_DISABLED: u32            = 1014;
    pub const NOT_A_DEFAULTER: u32             = 1015;
    pub const CANNOT_CHANGE_MID_ROUND: u32     = 1016;
    pub const ALREADY_A_MEMBER: u32            = 1017;
    pub const NO_REWARDS_TO_CLAIM: u32         = 1018;
    pub const ONLY_MEMBERS_ALLOWED: u32        = 1019;
    pub const PROPOSAL_NOT_FOUND: u32          = 1020;
    pub const VOTING_DEADLINE_PASSED: u32      = 1021;
    pub const PROPOSAL_NOT_PENDING: u32        = 1022;
    pub const ALREADY_VOTED: u32               = 1023;
    pub const VOTING_NOT_ENDED: u32            = 1024;
    pub const CONTRACT_PAUSED: u32             = 1025;
    pub const ALL_MEMBERS_SUSPENDED: u32       = 1026;
    pub const ALREADY_PAUSED: u32             = 1027;
    pub const NOT_PAUSED: u32                  = 1028;
    pub const MEMBER_ALREADY_EXITED: u32       = 1029;
    pub const EXIT_REQUEST_PENDING: u32        = 1030;
    pub const NO_EXIT_REQUEST_FOUND: u32       = 1031;
    pub const EXIT_NOT_ALLOWED_MID_ROUND: u32  = 1032;
    pub const CONTRIBUTION_WINDOW_CLOSED: u32  = 1033;
    pub const FEE_EXCEEDS_MAXIMUM: u32         = 1034;
    pub const INVALID_MAX_DEFAULTS: u32        = 1035;
    pub const GROUP_FULL: u32                  = 1036;
    pub const INVALID_MAX_MEMBERS: u32         = 1037;
    pub const DELEGATION_ALREADY_EXISTS: u32   = 1038;
    pub const NO_DELEGATION_FOUND: u32         = 1039;
    pub const CANNOT_VOTE_WITH_ACTIVE_DELEGATION: u32 = 1040;
    pub const CANNOT_SUB_DELEGATE: u32         = 1041;
    pub const INVITE_NOT_FOUND: u32            = 1042;
    pub const INVITE_ALREADY_REDEEMED: u32     = 1043;
    pub const INVITE_WRONG_RECIPIENT: u32      = 1044;
    pub const ADMIN_ACTION_NOT_FOUND: u32      = 1045;
    pub const ADMIN_ACTION_ALREADY_EXECUTED: u32 = 1046;
    pub const ADMIN_ACTION_EXPIRED: u32        = 1047;
    pub const ADMIN_ALREADY_APPROVED: u32      = 1048;
    pub const INSUFFICIENT_APPROVALS: u32      = 1049;
    pub const NOT_A_CO_ADMIN: u32             = 1050;
    // ExtError variants
    pub const INVALID_TIER: u32               = 1051;
    pub const INSURANCE_POOL_NEGATIVE: u32    = 1052;
    pub const INVALID_INSURANCE_CONTRIBUTION: u32 = 1053;
    pub const SKIP_LIMIT_REACHED: u32         = 1054;
    pub const ALREADY_SKIPPED: u32            = 1055;
    pub const INSUFFICIENT_WEIGHT: u32        = 1056;
    pub const EMERGENCY_PAYOUT_REQUESTED: u32 = 1057;
    pub const EMERGENCY_PAYOUT_QUORUM_NOT_MET: u32 = 1058;
    pub const EMERGENCY_PAYOUT_VOTE_EXPIRED: u32 = 1059;
    pub const EMERGENCY_PAYOUT_ALREADY_EXECUTED: u32 = 1060;
    pub const EMERGENCY_PAYOUT_LIMIT_REACHED: u32 = 1061;
    pub const GROUP_ALREADY_DISSOLVED: u32    = 1062;
    pub const DISSOLUTION_VOTE_IN_PROGRESS: u32 = 1063;
    pub const DISSOLUTION_QUORUM_NOT_MET: u32 = 1064;
    pub const DISSOLUTION_VOTE_EXPIRED: u32   = 1065;
    pub const NO_FUNDS_TO_DISTRIBUTE: u32     = 1066;
    pub const INVALID_EMERGENCY_CONFIG: u32   = 1067;
    pub const INVALID_DISSOLUTION_CONFIG: u32 = 1068;
    pub const GROUP_NOT_YET_ACTIVE: u32       = 1069;
    pub const ONLY_ADMIN_ALLOWED: u32         = 1070;
    pub const INVALID_AMOUNT: u32             = 1071;
    pub const CO_SIGNER_ALREADY_SET: u32      = 1072;
    pub const NO_CO_SIGNER_FOUND: u32         = 1073;
    pub const CO_SIGNER_NOT_ACCEPTED: u32     = 1074;
    pub const NOT_THE_CO_SIGNER: u32          = 1075;
    pub const CO_SIGNER_WINDOW_NOT_OPEN: u32  = 1076;
    pub const CO_SIGNER_WINDOW_EXPIRED: u32   = 1077;
    pub const GROUP_FROZEN: u32               = 1078;
    pub const GROUP_NOT_FROZEN: u32           = 1079;
    pub const SNAPSHOT_TOO_SOON: u32          = 1080;
    pub const TIER_NOT_FOUND: u32             = 1081;
    pub const INVALID_TIER_DEFINITION: u32    = 1082;
    pub const INSUFFICIENT_CREDIT_SCORE: u32  = 1083;
    pub const ROUND_DURATION_OUT_OF_BOUNDS: u32 = 1084;
    pub const DELEGATION_EXPIRED: u32         = 1085;
    pub const NOT_CONTRIB_DELEGATE: u32       = 1086;
    pub const SPLIT_PROPOSAL_NOT_FOUND: u32   = 1087;
    pub const SPLIT_MEMBERS_INVALID: u32      = 1088;
    pub const SPLIT_CONFIRMATION_WINDOW_CLOSED: u32 = 1089;
    pub const SOURCE_GROUP_ALREADY_SPLIT: u32 = 1090;
    pub const SPLIT_ALREADY_CONFIRMED: u32    = 1091;
    pub const SPLIT_NOT_FULLY_CONFIRMED: u32  = 1092;
    // ExtError2 variants
    pub const AUCTION_NOT_ENABLED: u32        = 1101;
    pub const AUCTION_NOT_OPEN: u32           = 1102;
    pub const AUCTION_WINDOW_CLOSED: u32      = 1103;
    pub const INCORRECT_CONTRIBUTION_AMOUNT: u32 = 1104;
    pub const INVALID_SLOT_INDEX: u32         = 1105;
    pub const MIGRATION_ALREADY_EXECUTED: u32 = 1106;
    pub const MIGRATION_ALREADY_PENDING: u32  = 1107;
    pub const MIGRATION_NOT_APPROVED: u32     = 1108;
    pub const MIGRATION_NOT_FOUND: u32        = 1109;
    pub const NO_BID_FOUND: u32              = 1110;
    pub const SLOT_OCCUPIED: u32              = 1111;
    pub const TOKEN_MISMATCH: u32             = 1112;
    pub const OUTSTANDING_LOAN_EXISTS: u32    = 1113;
    pub const NO_COPAYERS_REGISTERED: u32     = 1114;
    pub const COPAYER_AMOUNTS_MISMATCH: u32   = 1115;
    pub const RECEIPT_NOT_FOUND: u32          = 1116;
    pub const COPAYER_SPLITS_ALREADY_SET: u32 = 1117;
    pub const PROXY_ROUNDS_EXHAUSTED: u32     = 1118;
}

// ---------------------------------------------------------------------------
// ahjoor-payments (2000–2299)
// ---------------------------------------------------------------------------

pub mod payments {
    /// See [`crate::rosca::COUNT`] for why this exists.
    pub const COUNT: usize = 47;

    pub const RATE_LIMIT_EXCEEDED: u32              = 2001;
    pub const SUBSCRIPTION_PAUSED: u32              = 2002;
    pub const ORACLE_CONDITION_NOT_MET: u32         = 2003;
    pub const SUBSCRIPTION_IN_TRIAL: u32            = 2004;
    pub const TOKEN_NOT_ALLOWED: u32                = 2005;
    pub const DUPLICATE_EXTERNAL_ID: u32            = 2006;
    pub const MULTISIG_NOT_REQUIRED: u32            = 2007;
    pub const ALREADY_APPROVED: u32                 = 2008;
    pub const NOT_A_SIGNER: u32                     = 2009;
    pub const VOUCHER_EXPIRED: u32                  = 2010;
    pub const VOUCHER_EXHAUSTED: u32                = 2011;
    pub const VOUCHER_REVOKED: u32                  = 2012;
    pub const VOUCHER_NOT_FOUND: u32                = 2013;
    pub const WITHDRAWAL_RATE_LIMIT_EXCEEDED: u32   = 2014;
    pub const REFERRAL_ALREADY_EXISTS: u32          = 2015;
    pub const NO_COMMISSION_TO_CLAIM: u32           = 2016;
    pub const DYNAMIC_PAYMENT_EXPIRED: u32          = 2017;
    pub const TIPPING_NOT_ENABLED: u32              = 2018;
    pub const TIP_EXCEEDS_MAX_BPS: u32              = 2019;
    pub const MERCHANT_VOLUME_CAPPED: u32           = 2020;
    pub const SLIPPAGE_EXCEEDED: u32                = 2021;
    pub const ORACLE_NOT_WHITELISTED: u32           = 2022;
    pub const CUSTOMER_SPEND_LIMIT_EXCEEDED: u32    = 2023;
    pub const CAPTURE_PAST_DEADLINE: u32            = 2024;
    pub const EVIDENCE_WINDOW_CLOSED: u32           = 2025;
    pub const EVIDENCE_LIMIT_REACHED: u32           = 2026;
    pub const COOLING_OFF_EXPIRED: u32              = 2027;
    pub const NOT_IN_COOLING_OFF: u32               = 2028;
    pub const COOLING_OFF_EXCEEDS_MAX: u32          = 2029;
    pub const PAUSE_COUNT_EXCEEDED: u32             = 2030;
    pub const UNAUTHORIZED_PAUSE: u32               = 2031;
    pub const INSUFFICIENT_MERCHANT_RESERVE: u32    = 2032;
    pub const KYB_VERIFICATION_REQUIRED: u32        = 2033;
    pub const RETRY_NOT_DUE: u32                    = 2034;
    pub const DEBIT_RECORD_NOT_FOUND: u32           = 2035;
    pub const DEBIT_ALREADY_ABANDONED: u32          = 2036;
    pub const DEBIT_ALREADY_SUCCEEDED: u32          = 2037;
    pub const INVALID_PAYMENT_STATUS: u32           = 2038;
    pub const MAX_EXTENSIONS_REACHED: u32           = 2039;
    pub const MAX_EXTENSION_LEDGERS_EXCEEDED: u32   = 2040;
    pub const CUSTOMER_BLOCKED: u32                 = 2050;
    pub const DAO_NOT_CONFIGURED: u32               = 2051;
    pub const NOT_A_DAO_MEMBER: u32                 = 2052;
    pub const DAO_ALREADY_ESCALATED: u32            = 2053;
    pub const DAO_VOTE_WINDOW_OPEN: u32             = 2054;
    pub const DAO_VOTE_WINDOW_CLOSED: u32           = 2055;
    pub const DAO_ALREADY_VOTED: u32                = 2056;
}

// ---------------------------------------------------------------------------
// ahjoor-escrow (3000–3299)
// ---------------------------------------------------------------------------

pub mod escrow {
    /// See [`crate::rosca::COUNT`] for why this exists.
    pub const COUNT: usize = 249;

    pub const INVALID_DEADLINE: u32        = 3001;
    pub const INVALID_TRANCHE_INDEX: u32   = 3002;
    pub const TRANCHE_ALREADY_CLAIMED: u32 = 3003;
    pub const ALREADY_INITIALIZED: u32 = 3004;
    pub const AT_LEAST_ONE_BUYER_IS_REQUIRED: u32 = 3005;
    pub const DEADLINE_MUST_BE_FUTURE: u32 = 3006;
    pub const BUYER_CONTRIBUTION_MUST_BE_POSITIVE: u32 = 3007;
    pub const DUPLICATE_BUYER_IN_LIST: u32 = 3008;
    pub const BATCH_MUST_CONTAIN_AT_LEAST_ONE_ESCROW_CONFIG: u32 = 3009;
    pub const BATCH_SIZE_EXCEEDS_MAXIMUM_10_ESCROWS: u32 = 3010;
    pub const ONLY_SELLER_CAN_MARK_COMPLETE: u32 = 3011;
    pub const ESCROW_IS_NOT_ACTIVE: u32 = 3012;
    pub const NO_INSPECTOR_SET_USE_RELEASE_ESCROW_DIRECTLY: u32 = 3013;
    pub const ESCROW_IS_NOT_AWAITING_INSPECTION: u32 = 3014;
    pub const ONLY_ASSIGNED_INSPECTOR_CAN_SUBMIT_REPORT: u32 = 3015;
    pub const ONLY_BUYER_OR_SELLER_CAN_PROPOSE_INSPECTOR_REPLACEMENT: u32 = 3016;
    pub const NO_INSPECTOR_SET_ESCROW: u32 = 3017;
    pub const ONLY_ADMIN_CAN_SET_INSPECTOR_SCORE_THRESHOLD: u32 = 3018;
    pub const MIN_SCORE_BPS_EXCEEDS_MAXIMUM: u32 = 3019;
    pub const ONLY_ADMIN_CAN_APPEAL_INSPECTOR_RULING: u32 = 3020;
    pub const INSPECTOR_RULING_ALREADY_APPEALED_ESCROW: u32 = 3021;
    pub const INSPECTOR_SCORE_BELOW_MINIMUM_THRESHOLD_HIGH_VALUE_ESCROW: u32 = 3022;
    pub const ESCROW_AMOUNT_MUST_BE_POSITIVE: u32 = 3023;
    pub const DEADLINE_MUST_BE_AFTER_MIN_LOCK_UNTIL: u32 = 3024;
    pub const ARBITER_FEE_EXCEEDS_MAXIMUM_1000_BPS: u32 = 3025;
    pub const INCOMPLETE_RELEASE_CONDITION: u32 = 3026;
    pub const RELEASE_CONDITION_THRESHOLD_MUST_BE_POSITIVE: u32 = 3027;
    pub const INVALID_RELEASE_COMPARISON: u32 = 3028;
    pub const MAXIMUM_5_SELLERS_ALLOWED: u32 = 3029;
    pub const SELLER_ALLOCATIONS_MUST_SUM_TO_10000_BPS: u32 = 3030;
    pub const DISPUTE_TIMEOUT_SECONDS_MUST_BE_POSITIVE: u32 = 3031;
    pub const ONLY_CURRENT_HOLDER_CAN_TRANSFER_RECEIPT: u32 = 3032;
    pub const ACTIVE_MILESTONE_IN_PROGRESS: u32 = 3033;
    pub const INSPECTION_PENDING: u32 = 3034;
    pub const ONLY_LISTED_BUYER_CAN_APPROVE_MULTI_BUYER_RELEASE: u32 = 3035;
    pub const BUYER_HAS_ALREADY_APPROVED_RELEASE: u32 = 3036;
    pub const ONLY_BUYER_OR_ARBITER_CAN_RELEASE_ESCROW: u32 = 3037;
    pub const SELLER_VETO_ACTIVE: u32 = 3038;
    pub const SELLER_VETO_ACTIVE_2: u32 = 3039;
    pub const CONDITION_NOT_MET: u32 = 3040;
    pub const ONLY_BUYER_OR_SELLER_CAN_WAIVE_CONDITION: u32 = 3041;
    pub const NO_CONDITIONAL_RELEASE_SET_ESCROW: u32 = 3042;
    pub const ONLY_BUYER_OR_SELLER_CAN_SUBMIT_EVIDENCE: u32 = 3043;
    pub const MAXIMUM_EVIDENCE_ENTRIES_REACHED_PARTY: u32 = 3044;
    pub const ONLY_BUYER_CAN_SET_RENEWAL_ALLOWANCE: u32 = 3045;
    pub const AUTO_RENEW_IS_NOT_ENABLED_ESCROW: u32 = 3046;
    pub const ONLY_BUYER_CAN_CANCEL_AUTO_RENEW: u32 = 3047;
    pub const ONLY_BUYER_CAN_CANCEL_AUTO_RENEWAL: u32 = 3048;
    pub const NO_AUTO_RENEW_CONFIG_SET_ESCROW: u32 = 3049;
    pub const RELEASE_AMOUNT_MUST_BE_POSITIVE: u32 = 3050;
    pub const RELEASE_AMOUNT_EXCEEDS_ESCROW_BALANCE: u32 = 3051;
    pub const AT_LEAST_ONE_MILESTONE_REQUIRED: u32 = 3052;
    pub const TOO_MANY_MILESTONES: u32 = 3053;
    pub const MILESTONE_AMOUNT_MUST_BE_POSITIVE: u32 = 3054;
    pub const NEW_MILESTONES_MUST_START_AS_PENDING: u32 = 3055;
    pub const ESCROW_ALREADY_TERMINAL: u32 = 3056;
    pub const ONLY_BUYER_OR_ARBITER_CAN_APPROVE_MILESTONES: u32 = 3057;
    pub const MILESTONE_INDEX_OUT_RANGE: u32 = 3058;
    pub const MILESTONE_NOT_PENDING: u32 = 3059;
    pub const ONLY_BUYER_OR_SELLER_CAN_DISPUTE_ESCROW: u32 = 3060;
    pub const DISPUTE_AMOUNT_OUT_OF_RANGE: u32 = 3061;
    pub const BUYER_PERCENT_MUST_BE_BETWEEN_0_AND_100: u32 = 3062;
    pub const ESCROW_IS_NOT_DISPUTED: u32 = 3063;
    pub const ONLY_ARBITER_CAN_RESOLVE_DISPUTE: u32 = 3064;
    pub const ESCROW_IS_NOT_COOLING_OFF_STATE: u32 = 3065;
    pub const ONLY_BUYER_OR_SELLER_CAN_FLAG_RESOLUTION_ERROR: u32 = 3066;
    pub const COOLING_OFF_WINDOW_HAS_EXPIRED: u32 = 3067;
    pub const RESOLUTION_ALREADY_FLAGGED: u32 = 3068;
    pub const COOLING_OFF_WINDOW_HAS_NOT_ELAPSED: u32 = 3069;
    pub const RESOLUTION_IS_FLAGGED_ADMIN_MUST_REVIEW_BEFORE_FINALIZATION: u32 = 3070;
    pub const ONLY_ADMIN_CAN_CLEAR_RESOLUTION_FLAGS: u32 = 3071;
    pub const NO_FLAG_TO_CLEAR: u32 = 3072;
    pub const ONLY_ADMIN_CAN_CONFIGURE_COOLING_OFF_PERIOD: u32 = 3073;
    pub const FEE_CONFIGURATION_EXCEEDS_ESCROW_AMOUNT: u32 = 3074;
    pub const TIMEOUT_MUST_BE_POSITIVE: u32 = 3075;
    pub const MULTIPLIER_MUST_BE_POSITIVE: u32 = 3076;
    pub const DEADLINE_MUST_BE_POSITIVE: u32 = 3077;
    pub const DISPUTE_ALREADY_RESOLVED: u32 = 3078;
    pub const DISPUTE_TIMEOUT_DEADLINE_HAS_NOT_PASSED_YET: u32 = 3079;
    pub const MAX_ORACLE_AGE_MUST_BE_POSITIVE: u32 = 3080;
    pub const INSURANCE_TRIGGER_DAYS_MUST_BE_POSITIVE: u32 = 3081;
    pub const INSURANCE_CONTRIBUTION_MUST_BE_POSITIVE: u32 = 3082;
    pub const ONLY_BUYER_OR_SELLER_CAN_CLAIM_INSURANCE: u32 = 3083;
    pub const INSURANCE_ALREADY_CLAIMED: u32 = 3084;
    pub const ADMIN_CONFIRMATION_REQUIRED: u32 = 3085;
    pub const INSURANCE_TRIGGER_PERIOD_NOT_REACHED: u32 = 3086;
    pub const ESCROW_TOKEN_NOT_COVERED_BY_INSURANCE_POOL: u32 = 3087;
    pub const INSURANCE_POOL_HAS_INSUFFICIENT_BALANCE: u32 = 3088;
    pub const FEE_EXCEEDS_MAXIMUM_200_BPS: u32 = 3089;
    pub const WITHDRAWAL_AMOUNT_MUST_BE_POSITIVE: u32 = 3090;
    pub const INSUFFICIENT_ACCRUED_FEES: u32 = 3091;
    pub const RELEASE_CONDITION_NOT_MET: u32 = 3092;
    pub const ESCROW_HAS_NOT_EXPIRED_YET: u32 = 3093;
    pub const ONLY_BUYER_OR_SELLER_CAN_PROPOSE_DEADLINE_EXTENSION: u32 = 3094;
    pub const CANNOT_EXTEND_DEADLINE_WHILE_ESCROW_IS_DISPUTED: u32 = 3095;
    pub const NEW_DEADLINE_MUST_BE_GREATER_THAN_CURRENT_DEADLINE: u32 = 3096;
    pub const ONLY_BUYER_OR_SELLER_CAN_ACCEPT_DEADLINE_EXTENSION: u32 = 3097;
    pub const PROPOSER_CANNOT_ACCEPT_THEIR_OWN_DEADLINE_EXTENSION: u32 = 3098;
    pub const DEADLINE_EXTENSION_PROPOSAL_HAS_EXPIRED: u32 = 3099;
    pub const ONLY_BUYER_OR_SELLER_CAN_PROPOSE_AMENDMENT: u32 = 3100;
    pub const CANNOT_AMEND_TERMINAL_ESCROW: u32 = 3101;
    pub const NEW_AMOUNT_MUST_BE_POSITIVE: u32 = 3102;
    pub const ONLY_BUYER_OR_SELLER_CAN_SIGN_AMENDMENT: u32 = 3103;
    pub const AMENDMENT_NONCE_MISMATCH: u32 = 3104;
    pub const AMENDMENT_PROPOSAL_HAS_EXPIRED: u32 = 3105;
    pub const AMENDMENT_REQUIRES_BUYER_AND_SELLER_SIGNATURES: u32 = 3106;
    pub const ONLY_BUYER_CAN_TOP_UP_ESCROW: u32 = 3107;
    pub const ESCROW_IS_NOT_ACTIVE_OR_AWAITING_INSPECTION: u32 = 3108;
    pub const ADDITIONAL_AMOUNT_MUST_BE_POSITIVE: u32 = 3109;
    pub const TOP_UP_LIMIT_EXCEEDED: u32 = 3110;
    pub const ONLY_SELLER_CAN_ACKNOWLEDGE_TOP_UP: u32 = 3111;
    pub const ONLY_SELLER_CAN_REQUEST_PARTIAL_RELEASE: u32 = 3112;
    pub const PARTIAL_RELEASE_ONLY_ALLOWED_ACTIVE_ESCROW: u32 = 3113;
    pub const PARTIAL_RELEASE_AMOUNT_MUST_BE_POSITIVE: u32 = 3114;
    pub const PARTIAL_RELEASE_AMOUNT_CANNOT_EXCEED_ESCROW_AMOUNT: u32 = 3115;
    pub const REQUEST_ALREADY_PENDING: u32 = 3116;
    pub const ONLY_BUYER_CAN_APPROVE_PARTIAL_RELEASE: u32 = 3117;
    pub const INVALID_REQUEST_ID: u32 = 3118;
    pub const DELEGATE_MUST_BE_DIFFERENT_FROM_SELLER: u32 = 3119;
    pub const SELLER_NOT_PART_OF_ESCROW: u32 = 3120;
    pub const CAN_ONLY_DELEGATE_BEFORE_ESCROW_IS_RELEASED: u32 = 3121;
    pub const ONLY_BUYER_CAN_REJECT_PARTIAL_RELEASE: u32 = 3122;
    pub const ONLY_SELLER_CAN_ESCALATE_PARTIAL_RELEASE: u32 = 3123;
    pub const RESPONSE_DEADLINE_NOT_YET_PASSED: u32 = 3124;
    pub const NEW_BUYER_MUST_BE_DIFFERENT_FROM_CURRENT_BUYER: u32 = 3125;
    pub const BUYER_TRANSFER_ONLY_ALLOWED_ACTIVE_ESCROWS: u32 = 3126;
    pub const ONLY_CURRENT_BUYER_CAN_TRANSFER_BUYER_ROLE: u32 = 3127;
    pub const ONLY_BUYER_OR_SELLER_CAN_UPDATE_METADATA: u32 = 3128;
    pub const ONLY_ADMIN_CAN_UPGRADE_CONTRACT: u32 = 3129;
    pub const ONLY_ADMIN_CAN_MIGRATE_CONTRACT: u32 = 3130;
    pub const MIGRATION_ALREADY_COMPLETED_VERSION: u32 = 3131;
    pub const UNLOCK_AT_MUST_BE_FUTURE: u32 = 3132;
    pub const ALREADY_CLAIMED: u32 = 3133;
    pub const ONLY_BENEFICIARY_CAN_CLAIM: u32 = 3134;
    pub const UNLOCK_TIME_HAS_NOT_PASSED: u32 = 3135;
    pub const ESCROW_NOT_ACTIVE: u32 = 3136;
    pub const PAST_UNLOCK_TIME_USE_CLAIM_TIMELOCKED: u32 = 3137;
    pub const ONLY_BUYER_CAN_CANCEL: u32 = 3138;
    pub const DISPUTE_ACTIVE: u32 = 3139;
    pub const ONLY_ADMIN_CAN_SET_TOKEN_WHITELIST_CONTRACT: u32 = 3140;
    pub const CONTRACT_ALREADY_PAUSED: u32 = 3141;
    pub const CONTRACT_IS_NOT_PAUSED: u32 = 3142;
    pub const TOKEN_NOT_ALLOWED: u32 = 3143;
    pub const DEADLINE_DURATION_MUST_BE_POSITIVE: u32 = 3144;
    pub const TEMPLATE_IS_DEACTIVATED: u32 = 3145;
    pub const ARBITER_ALREADY_POOL: u32 = 3146;
    pub const ARBITER_NOT_POOL: u32 = 3147;
    pub const ARBITER_POOL_IS_EMPTY: u32 = 3148;
    pub const ONLY_TEMPLATE_CREATOR_CAN_UPDATE: u32 = 3149;
    pub const ONLY_TEMPLATE_CREATOR_CAN_DEACTIVATE: u32 = 3150;
    pub const TEMPLATE_ALREADY_DEACTIVATED: u32 = 3151;
    pub const INACTIVITY_RELEASE_IS_NOT_ENABLED_ESCROW: u32 = 3152;
    pub const ONLY_ESCROW_SELLER_CAN_CLAIM_INACTIVITY_RELEASE: u32 = 3153;
    pub const BUYER_INACTIVITY_WINDOW_HAS_NOT_ELAPSED: u32 = 3154;
    pub const PENALTY_CANNOT_EXCEED_10000_BPS: u32 = 3155;
    pub const RESPONSE_WINDOW_MUST_BE_POSITIVE: u32 = 3156;
    pub const ONLY_BUYER_OR_SELLER_CAN_REQUEST_CANCELLATION: u32 = 3157;
    pub const NO_PENDING_CANCELLATION_ESCROW: u32 = 3158;
    pub const INITIATOR_CANNOT_ACCEPT_THEIR_OWN_CANCELLATION_REQUEST: u32 = 3159;
    pub const ONLY_BUYER_OR_SELLER_CAN_ACCEPT_CANCELLATION: u32 = 3160;
    pub const INITIATOR_CANNOT_REJECT_THEIR_OWN_CANCELLATION_REQUEST: u32 = 3161;
    pub const ONLY_BUYER_OR_SELLER_CAN_REJECT_CANCELLATION: u32 = 3162;
    pub const RESPONSE_WINDOW_HAS_NOT_ELAPSED: u32 = 3163;
    pub const BOUNTY_AMOUNT_MUST_BE_POSITIVE: u32 = 3164;
    pub const CLAIM_DEADLINE_MUST_BE_FUTURE: u32 = 3165;
    pub const SUBMISSION_DEADLINE_MUST_BE_AFTER_CLAIM_DEADLINE: u32 = 3166;
    pub const TOKEN_NOT_WHITELISTED: u32 = 3167;
    pub const BOUNTY_IS_NOT_AVAILABLE_CLAIMING: u32 = 3168;
    pub const CLAIM_DEADLINE_HAS_PASSED: u32 = 3169;
    pub const BOUNTY_IS_NOT_CLAIMED_STATUS: u32 = 3170;
    pub const ONLY_ASSIGNED_SOLVER_CAN_SUBMIT_WORK: u32 = 3171;
    pub const SUBMISSION_DEADLINE_HAS_PASSED: u32 = 3172;
    pub const ONLY_BUYER_CAN_APPROVE_SUBMISSION: u32 = 3173;
    pub const NO_SUBMISSION_HAS_BEEN_MADE: u32 = 3174;
    pub const ONLY_BUYER_CAN_REJECT_SUBMISSION: u32 = 3175;
    pub const MAXIMUM_REJECTION_ROUNDS_REACHED: u32 = 3176;
    pub const ONLY_BUYER_CAN_CANCEL_BOUNTY: u32 = 3177;
    pub const CANNOT_CANCEL_BOUNTY_CURRENT_STATE: u32 = 3178;
    pub const ONLY_ADMIN_CAN_SET_MAX_BOUNTY_REJECTION_ROUNDS: u32 = 3179;
    pub const BOUNTY_MUST_HAVE_AT_LEAST_ONE_MILESTONE: u32 = 3180;
    pub const BOUNTY_MUST_BE_CLAIMED_BEFORE_SUBMITTING_MILESTONES: u32 = 3181;
    pub const ONLY_SOLVER_CAN_SUBMIT_MILESTONES: u32 = 3182;
    pub const MILESTONE_INDEX_OUT_BOUNDS: u32 = 3183;
    pub const PREVIOUS_MILESTONE_NOT_YET_VERIFIED: u32 = 3184;
    pub const MILESTONE_IS_NOT_AWAITING_SUBMISSION: u32 = 3185;
    pub const MILESTONE_IS_NOT_AWAITING_VERIFICATION: u32 = 3186;
    pub const ONLY_BOUNTY_CREATOR_CAN_REPLACE_VERIFIER: u32 = 3187;
    pub const VERIFIER_CAN_ONLY_BE_REPLACED_BEFORE_MILESTONE_IS_SUBMITTED: u32 = 3188;
    pub const ONLY_BUYER_CAN_CONFIGURE_COLLATERAL_HEALTH: u32 = 3189;
    pub const MIN_COLLATERAL_RATIO_BPS_OUT_OF_RANGE: u32 = 3190;
    pub const TOP_UP_AMOUNT_MUST_BE_POSITIVE: u32 = 3191;
    pub const ESCROW_IS_NOT_ACTIVE_OR_UNDER_COLLATERALIZED: u32 = 3192;
    pub const ONLY_BUYER_CAN_CONFIGURE_MULTI_PARTY_APPROVAL: u32 = 3193;
    pub const APPROVERS_COUNT_MUST_BE_BETWEEN_2_AND_10: u32 = 3194;
    pub const THRESHOLD_MUST_BE_BETWEEN_1_AND_APPROVERS_COUNT: u32 = 3195;
    pub const CANNOT_RECONFIGURE_APPROVALS_ALREADY_PROGRESS: u32 = 3196;
    pub const CALLER_IS_NOT_AUTHORIZED_APPROVER_ESCROW: u32 = 3197;
    pub const APPROVER_HAS_ALREADY_APPROVED_ESCROW: u32 = 3198;
    pub const RELEASE_SCHEDULE_MUST_CONTAIN_AT_LEAST_ONE_TRANCHE: u32 = 3199;
    pub const EACH_TRANCHE_AMOUNT_MUST_BE_POSITIVE: u32 = 3200;
    pub const EACH_TRANCHE_UNLOCK_AT_MUST_BE_FUTURE: u32 = 3201;
    pub const ONLY_BENEFICIARY_SELLER_CAN_CLAIM_SCHEDULED_RELEASES: u32 = 3202;
    pub const ESCROW_IS_NOT_CLAIMABLE_STATE: u32 = 3203;
    pub const NO_TRANCHES_ARE_CURRENTLY_CLAIMABLE: u32 = 3204;
    pub const CONTRACT_IS_PAUSED: u32 = 3205;
    pub const ONLY_ADMIN_CAN_PAUSE_CONTRACT: u32 = 3206;
    pub const ONLY_ADMIN_CAN_RESUME_CONTRACT: u32 = 3207;
    pub const ESCROW_STILL_LOCKED: u32 = 3208;
    pub const ORACLE_PRICE_IS_STALE: u32 = 3209;
    pub const INVALID_ORACLE_PRICE: u32 = 3210;
    pub const INSUFFICIENT_RENEWAL_ALLOWANCE: u32 = 3211;
    pub const ESCROW_RENEWAL_DURATION_MUST_BE_POSITIVE: u32 = 3212;
    pub const ONLY_ADMIN_CAN_SET_MAX_TOP_UP_BPS: u32 = 3213;
    pub const COLLATERAL_FORFEIT_BPS_CANNOT_EXCEED_10000: u32 = 3214;
    pub const AT_LEAST_ONE_SELLER_REQUIRED: u32 = 3215;
    pub const ONLY_SELLER_CAN_DEPOSIT_COLLATERAL: u32 = 3216;
    pub const ESCROW_IS_NOT_AWAITING_COLLATERAL: u32 = 3217;
    pub const COLLATERAL_DEPOSIT_WINDOW_HAS_EXPIRED: u32 = 3218;
    pub const RATING_MUST_BE_BETWEEN_1_AND_5: u32 = 3219;
    pub const RATING_ONLY_ALLOWED_AFTER_ESCROW_IS_RELEASED_OR_RESOLVED: u32 = 3220;
    pub const ONLY_BUYER_OR_SELLER_CAN_SUBMIT_RATING: u32 = 3221;
    pub const RATING_ALREADY_SUBMITTED_ESCROW: u32 = 3222;
    pub const ONLY_SELLER_CAN_SUBMIT_DELIVERY_PROOF: u32 = 3223;
    pub const PROOF_SUBMISSION_LOCKED_ESCROW_IS_UNDER_DISPUTE: u32 = 3224;
    pub const INVALID_DELIVERY_PROOF: u32 = 3225;
    pub const ONLY_ESCROW_SELLER_CAN_RAISE_VETO: u32 = 3226;
    pub const VETO_COOLDOWN_ACTIVE: u32 = 3227;
    pub const ONLY_BUYER_CAN_APPROVE: u32 = 3228;
    pub const NO_PENDING_SELLER_TRANSFER: u32 = 3229;
    pub const ONLY_ADMIN_CAN_SET_VETO_OVERRIDE_WINDOW: u32 = 3230;
    pub const WINDOW_SECONDS_MUST_BE_POSITIVE: u32 = 3231;
    pub const ONLY_ESCROW_SELLER_CAN_CANCEL_VETO: u32 = 3232;
    pub const VETO_WINDOW_ELAPSED: u32 = 3233;
    pub const ONLY_ADMIN_CAN_OVERRIDE_SELLER_VETO: u32 = 3234;
    pub const ACTIVE_DISPUTE_EXISTS: u32 = 3235;
    pub const VETO_WINDOW_NOT_ELAPSED: u32 = 3236;
    pub const VETO_WINDOW_HAS_NOT_EXPIRED_YET: u32 = 3237;
    pub const ONLY_CURRENT_SELLER_CAN_INITIATE_TRANSFER: u32 = 3238;
    pub const ESCROW_MUST_BE_ACTIVE_TO_TRANSFER_SELLER_ROLE: u32 = 3239;
    pub const ONLY_BUYER_CAN_VETO: u32 = 3240;
    pub const ONLY_ADMIN_CAN_SET_VETO_WINDOW: u32 = 3241;
    pub const RELEASE_BPS_MUST_BE_POSITIVE_EACH_MILESTONE: u32 = 3242;
    pub const MILESTONE_RELEASE_BPS_MUST_SUM_TO_10000: u32 = 3243;
    pub const ONLY_ESCROW_SELLER_MAY_SUBMIT_MILESTONES: u32 = 3244;
    pub const MILESTONE_MUST_BE_PENDING_OR_REJECTED_TO_SUBMIT: u32 = 3245;
    pub const MILESTONE_MUST_BE_SUBMITTED_BEFORE_APPROVAL: u32 = 3246;
    pub const ONLY_ESCROW_BUYER_MAY_REJECT_MILESTONES: u32 = 3247;
    pub const ONLY_SUBMITTED_MILESTONE_CAN_BE_REJECTED: u32 = 3248;
    pub const ONLY_LOSING_PARTY_CAN_FLAG_RESOLUTION_ERROR: u32 = 3249;
}

// ---------------------------------------------------------------------------
// ahjoor-refund (4000–4099)
// ---------------------------------------------------------------------------

pub mod refund {
    /// See [`crate::rosca::COUNT`] for why this exists.
    pub const COUNT: usize = 8;

    // Refund contract uses panic! rather than a contracterror enum;
    // these codes are the off-chain namespace assignments for future migration.
    pub const ALREADY_INITIALIZED: u32             = 4001;
    pub const FEE_EXCEEDS_MAXIMUM: u32             = 4002;
    pub const AMOUNT_MUST_BE_POSITIVE: u32         = 4003;
    pub const INVALID_REASON_CODE: u32             = 4004;
    pub const REFUND_COOLDOWN_ACTIVE: u32          = 4005;
    pub const PAYMENT_NOT_FOUND: u32               = 4006;
    pub const PAYMENT_NOT_COMPLETED: u32           = 4007;
    pub const EXCEEDS_REFUNDABLE_AMOUNT: u32       = 4008;
}

// ---------------------------------------------------------------------------
// ahjoor-token-whitelist (5000–5099)
// ---------------------------------------------------------------------------

pub mod whitelist {
    /// See [`crate::rosca::COUNT`] for why this exists.
    pub const COUNT: usize = 9;

    pub const NOT_INITIALIZED: u32            = 5001;
    pub const ALREADY_INITIALIZED: u32        = 5002;
    pub const UNAUTHORIZED: u32               = 5003;
    pub const TOKEN_ALREADY_WHITELISTED: u32  = 5004;
    pub const TOKEN_NOT_WHITELISTED: u32      = 5005;
    pub const QUOTA_EXCEEDED: u32             = 5006;
    pub const TOKEN_ALREADY_HAS_QUOTA: u32    = 5007;
    pub const TOKEN_HAS_NO_QUOTA: u32         = 5008;
    pub const RISK_TIER_NOT_DEFINED: u32     = 5009;
}

// ---------------------------------------------------------------------------
// Convenience: machine-readable error descriptor
// ---------------------------------------------------------------------------

/// Compact descriptor for one error code entry (used in errors.json generation).
pub struct ErrorEntry {
    pub code: u32,
    pub name: &'static str,
    pub contract: &'static str,
}

pub static ALL_ERRORS: &[ErrorEntry] = &[
    // rosca (110 entries — must match rosca::COUNT)
    ErrorEntry { code: rosca::ALREADY_INITIALIZED, name: "AlreadyInitialized", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::TOKEN_NOT_APPROVED, name: "TokenNotApproved", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CUSTOM_ORDER_LENGTH_MISMATCH, name: "CustomOrderLengthMismatch", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CUSTOM_ORDER_NON_MEMBER, name: "CustomOrderNonMember", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::AMOUNT_MUST_BE_POSITIVE, name: "AmountMustBePositive", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ROUND_DEADLINE_PASSED, name: "RoundDeadlinePassed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MEMBER_HAS_EXITED, name: "MemberHasExited", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NOT_A_MEMBER, name: "NotAMember", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ALREADY_CONTRIBUTED, name: "AlreadyContributed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_EXCHANGE_RATE, name: "InvalidExchangeRate", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EXCEEDS_TOKEN_LIMIT, name: "ExceedsTokenLimit", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EXCEEDS_REMAINING_CONTRIBUTION, name: "ExceedsRemainingContribution", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::DEADLINE_NOT_PASSED, name: "DeadlineNotPassed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::PENALTY_DISABLED, name: "PenaltyDisabled", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NOT_A_DEFAULTER, name: "NotADefaulter", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CANNOT_CHANGE_MID_ROUND, name: "CannotChangeMidRound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ALREADY_A_MEMBER, name: "AlreadyAMember", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NO_REWARDS_TO_CLAIM, name: "NoRewardsToClaim", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ONLY_MEMBERS_ALLOWED, name: "OnlyMembersAllowed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::PROPOSAL_NOT_FOUND, name: "ProposalNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::VOTING_DEADLINE_PASSED, name: "VotingDeadlinePassed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::PROPOSAL_NOT_PENDING, name: "ProposalNotPending", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ALREADY_VOTED, name: "AlreadyVoted", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::VOTING_NOT_ENDED, name: "VotingNotEnded", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CONTRACT_PAUSED, name: "ContractPaused", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ALL_MEMBERS_SUSPENDED, name: "AllMembersSuspended", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ALREADY_PAUSED, name: "AlreadyPaused", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NOT_PAUSED, name: "NotPaused", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MEMBER_ALREADY_EXITED, name: "MemberAlreadyExited", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EXIT_REQUEST_PENDING, name: "ExitRequestPending", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NO_EXIT_REQUEST_FOUND, name: "NoExitRequestFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EXIT_NOT_ALLOWED_MID_ROUND, name: "ExitNotAllowedMidRound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CONTRIBUTION_WINDOW_CLOSED, name: "ContributionWindowClosed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::FEE_EXCEEDS_MAXIMUM, name: "FeeExceedsMaximum", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_MAX_DEFAULTS, name: "InvalidMaxDefaults", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::GROUP_FULL, name: "GroupFull", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_MAX_MEMBERS, name: "InvalidMaxMembers", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::DELEGATION_ALREADY_EXISTS, name: "DelegationAlreadyExists", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NO_DELEGATION_FOUND, name: "NoDelegationFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CANNOT_VOTE_WITH_ACTIVE_DELEGATION, name: "CannotVoteWithActiveDelegation", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CANNOT_SUB_DELEGATE, name: "CannotSubDelegate", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVITE_NOT_FOUND, name: "InviteNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVITE_ALREADY_REDEEMED, name: "InviteAlreadyRedeemed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVITE_WRONG_RECIPIENT, name: "InviteWrongRecipient", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ADMIN_ACTION_NOT_FOUND, name: "AdminActionNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ADMIN_ACTION_ALREADY_EXECUTED, name: "AdminActionAlreadyExecuted", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ADMIN_ACTION_EXPIRED, name: "AdminActionExpired", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ADMIN_ALREADY_APPROVED, name: "AdminAlreadyApproved", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INSUFFICIENT_APPROVALS, name: "InsufficientApprovals", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NOT_A_CO_ADMIN, name: "NotACoAdmin", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_TIER, name: "InvalidTier", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INSURANCE_POOL_NEGATIVE, name: "InsurancePoolNegative", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_INSURANCE_CONTRIBUTION, name: "InvalidInsuranceContribution", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SKIP_LIMIT_REACHED, name: "SkipLimitReached", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ALREADY_SKIPPED, name: "AlreadySkipped", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INSUFFICIENT_WEIGHT, name: "InsufficientWeight", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EMERGENCY_PAYOUT_REQUESTED, name: "EmergencyPayoutRequested", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EMERGENCY_PAYOUT_QUORUM_NOT_MET, name: "EmergencyPayoutQuorumNotMet", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EMERGENCY_PAYOUT_VOTE_EXPIRED, name: "EmergencyPayoutVoteExpired", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EMERGENCY_PAYOUT_ALREADY_EXECUTED, name: "EmergencyPayoutAlreadyExecuted", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::EMERGENCY_PAYOUT_LIMIT_REACHED, name: "EmergencyPayoutLimitReached", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::GROUP_ALREADY_DISSOLVED, name: "GroupAlreadyDissolved", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::DISSOLUTION_VOTE_IN_PROGRESS, name: "DissolutionVoteInProgress", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::DISSOLUTION_QUORUM_NOT_MET, name: "DissolutionQuorumNotMet", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::DISSOLUTION_VOTE_EXPIRED, name: "DissolutionVoteExpired", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NO_FUNDS_TO_DISTRIBUTE, name: "NoFundsToDistribute", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_EMERGENCY_CONFIG, name: "InvalidEmergencyConfig", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_DISSOLUTION_CONFIG, name: "InvalidDissolutionConfig", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::GROUP_NOT_YET_ACTIVE, name: "GroupNotYetActive", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ONLY_ADMIN_ALLOWED, name: "OnlyAdminAllowed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_AMOUNT, name: "InvalidAmount", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CO_SIGNER_ALREADY_SET, name: "CoSignerAlreadySet", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NO_CO_SIGNER_FOUND, name: "NoCoSignerFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CO_SIGNER_NOT_ACCEPTED, name: "CoSignerNotAccepted", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NOT_THE_CO_SIGNER, name: "NotTheCoSigner", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CO_SIGNER_WINDOW_NOT_OPEN, name: "CoSignerWindowNotOpen", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CO_SIGNER_WINDOW_EXPIRED, name: "CoSignerWindowExpired", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::GROUP_FROZEN, name: "GroupFrozen", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::GROUP_NOT_FROZEN, name: "GroupNotFrozen", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SNAPSHOT_TOO_SOON, name: "SnapshotTooSoon", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::TIER_NOT_FOUND, name: "TierNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_TIER_DEFINITION, name: "InvalidTierDefinition", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INSUFFICIENT_CREDIT_SCORE, name: "InsufficientCreditScore", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ROUND_DURATION_OUT_OF_BOUNDS, name: "RoundDurationOutOfBounds", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::DELEGATION_EXPIRED, name: "DelegationExpired", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NOT_CONTRIB_DELEGATE, name: "NotContribDelegate", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SPLIT_PROPOSAL_NOT_FOUND, name: "SplitProposalNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SPLIT_MEMBERS_INVALID, name: "SplitMembersInvalid", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SPLIT_CONFIRMATION_WINDOW_CLOSED, name: "SplitConfirmationWindowClosed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SOURCE_GROUP_ALREADY_SPLIT, name: "SourceGroupAlreadySplit", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SPLIT_ALREADY_CONFIRMED, name: "SplitAlreadyConfirmed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SPLIT_NOT_FULLY_CONFIRMED, name: "SplitNotFullyConfirmed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::AUCTION_NOT_ENABLED, name: "AuctionNotEnabled", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::AUCTION_NOT_OPEN, name: "AuctionNotOpen", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::AUCTION_WINDOW_CLOSED, name: "AuctionWindowClosed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INCORRECT_CONTRIBUTION_AMOUNT, name: "IncorrectContributionAmount", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::INVALID_SLOT_INDEX, name: "InvalidSlotIndex", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MIGRATION_ALREADY_EXECUTED, name: "MigrationAlreadyExecuted", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MIGRATION_ALREADY_PENDING, name: "MigrationAlreadyPending", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MIGRATION_NOT_APPROVED, name: "MigrationNotApproved", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MIGRATION_NOT_FOUND, name: "MigrationNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NO_BID_FOUND, name: "NoBidFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::SLOT_OCCUPIED, name: "SlotOccupied", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::TOKEN_MISMATCH, name: "TokenMismatch", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::OUTSTANDING_LOAN_EXISTS, name: "OutstandingLoanExists", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NO_COPAYERS_REGISTERED, name: "NoCopayersRegistered", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::COPAYER_AMOUNTS_MISMATCH, name: "CopayerAmountsMismatch", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::RECEIPT_NOT_FOUND, name: "ReceiptNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::COPAYER_SPLITS_ALREADY_SET, name: "CopayerSplitsAlreadySet", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::PROXY_ROUNDS_EXHAUSTED, name: "ProxyRoundsExhausted", contract: "ahjoor-rosca" },

    // payments (47 entries — must match payments::COUNT)
    ErrorEntry { code: payments::RATE_LIMIT_EXCEEDED, name: "RateLimitExceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::SUBSCRIPTION_PAUSED, name: "SubscriptionPaused", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::ORACLE_CONDITION_NOT_MET, name: "OracleConditionNotMet", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::SUBSCRIPTION_IN_TRIAL, name: "SubscriptionInTrial", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::TOKEN_NOT_ALLOWED, name: "TokenNotAllowed", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DUPLICATE_EXTERNAL_ID, name: "DuplicateExternalId", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::MULTISIG_NOT_REQUIRED, name: "MultisigNotRequired", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::ALREADY_APPROVED, name: "AlreadyApproved", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::NOT_A_SIGNER, name: "NotASigner", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::VOUCHER_EXPIRED, name: "VoucherExpired", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::VOUCHER_EXHAUSTED, name: "VoucherExhausted", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::VOUCHER_REVOKED, name: "VoucherRevoked", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::VOUCHER_NOT_FOUND, name: "VoucherNotFound", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::WITHDRAWAL_RATE_LIMIT_EXCEEDED, name: "WithdrawalRateLimitExceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::REFERRAL_ALREADY_EXISTS, name: "ReferralAlreadyExists", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::NO_COMMISSION_TO_CLAIM, name: "NoCommissionToClaim", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DYNAMIC_PAYMENT_EXPIRED, name: "DynamicPaymentExpired", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::TIPPING_NOT_ENABLED, name: "TippingNotEnabled", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::TIP_EXCEEDS_MAX_BPS, name: "TipExceedsMaxBps", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::MERCHANT_VOLUME_CAPPED, name: "MerchantVolumeCapped", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::SLIPPAGE_EXCEEDED, name: "SlippageExceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::ORACLE_NOT_WHITELISTED, name: "OracleNotWhitelisted", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::CUSTOMER_SPEND_LIMIT_EXCEEDED, name: "CustomerSpendLimitExceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::CAPTURE_PAST_DEADLINE, name: "CapturePastDeadline", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::EVIDENCE_WINDOW_CLOSED, name: "EvidenceWindowClosed", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::EVIDENCE_LIMIT_REACHED, name: "EvidenceLimitReached", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::COOLING_OFF_EXPIRED, name: "CoolingOffExpired", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::NOT_IN_COOLING_OFF, name: "NotInCoolingOff", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::COOLING_OFF_EXCEEDS_MAX, name: "CoolingOffExceedsMax", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::PAUSE_COUNT_EXCEEDED, name: "PauseCountExceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::UNAUTHORIZED_PAUSE, name: "UnauthorizedPause", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::INSUFFICIENT_MERCHANT_RESERVE, name: "InsufficientMerchantReserve", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::KYB_VERIFICATION_REQUIRED, name: "KYBVerificationRequired", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::RETRY_NOT_DUE, name: "RetryNotDue", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DEBIT_RECORD_NOT_FOUND, name: "DebitRecordNotFound", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DEBIT_ALREADY_ABANDONED, name: "DebitAlreadyAbandoned", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DEBIT_ALREADY_SUCCEEDED, name: "DebitAlreadySucceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::INVALID_PAYMENT_STATUS, name: "InvalidPaymentStatus", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::MAX_EXTENSIONS_REACHED, name: "MaxExtensionsReached", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::MAX_EXTENSION_LEDGERS_EXCEEDED, name: "MaxExtensionLedgersExceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::CUSTOMER_BLOCKED, name: "CustomerBlocked", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DAO_NOT_CONFIGURED, name: "DaoNotConfigured", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::NOT_A_DAO_MEMBER, name: "NotADaoMember", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DAO_ALREADY_ESCALATED, name: "DaoAlreadyEscalated", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DAO_VOTE_WINDOW_OPEN, name: "DaoVoteWindowOpen", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DAO_VOTE_WINDOW_CLOSED, name: "DaoVoteWindowClosed", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DAO_ALREADY_VOTED, name: "DaoAlreadyVoted", contract: "ahjoor-payments" },

    // escrow (248 entries — must match escrow::COUNT)
    ErrorEntry { code: escrow::INVALID_DEADLINE, name: "InvalidDeadline", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INVALID_TRANCHE_INDEX, name: "InvalidTrancheIndex", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TRANCHE_ALREADY_CLAIMED, name: "TrancheAlreadyClaimed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ALREADY_INITIALIZED, name: "AlreadyInitialized", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::AT_LEAST_ONE_BUYER_IS_REQUIRED, name: "AtLeastOneBuyerIsRequired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DEADLINE_MUST_BE_FUTURE, name: "DeadlineMustBeFuture", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BUYER_CONTRIBUTION_MUST_BE_POSITIVE, name: "BuyerContributionMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DUPLICATE_BUYER_IN_LIST, name: "DuplicateBuyerInList", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BATCH_MUST_CONTAIN_AT_LEAST_ONE_ESCROW_CONFIG, name: "BatchMustContainAtLeastOneEscrowConfig", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BATCH_SIZE_EXCEEDS_MAXIMUM_10_ESCROWS, name: "BatchSizeExceedsMaximum10Escrows", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SELLER_CAN_MARK_COMPLETE, name: "OnlySellerCanMarkComplete", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_ACTIVE, name: "EscrowIsNotActive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_INSPECTOR_SET_USE_RELEASE_ESCROW_DIRECTLY, name: "NoInspectorSetUseReleaseEscrowDirectly", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_AWAITING_INSPECTION, name: "EscrowIsNotAwaitingInspection", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ASSIGNED_INSPECTOR_CAN_SUBMIT_REPORT, name: "OnlyAssignedInspectorCanSubmitReport", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_PROPOSE_INSPECTOR_REPLACEMENT, name: "OnlyBuyerOrSellerCanProposeInspectorReplacement", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_INSPECTOR_SET_ESCROW, name: "NoInspectorSetEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_SET_INSPECTOR_SCORE_THRESHOLD, name: "OnlyAdminCanSetInspectorScoreThreshold", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MIN_SCORE_BPS_EXCEEDS_MAXIMUM, name: "MinScoreBpsExceedsMaximum", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_APPEAL_INSPECTOR_RULING, name: "OnlyAdminCanAppealInspectorRuling", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSPECTOR_RULING_ALREADY_APPEALED_ESCROW, name: "InspectorRulingAlreadyAppealedEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSPECTOR_SCORE_BELOW_MINIMUM_THRESHOLD_HIGH_VALUE_ESCROW, name: "InspectorScoreBelowMinimumThresholdHighValueEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_AMOUNT_MUST_BE_POSITIVE, name: "EscrowAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DEADLINE_MUST_BE_AFTER_MIN_LOCK_UNTIL, name: "DeadlineMustBeAfterMinLockUntil", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ARBITER_FEE_EXCEEDS_MAXIMUM_1000_BPS, name: "ArbiterFeeExceedsMaximum1000Bps", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INCOMPLETE_RELEASE_CONDITION, name: "IncompleteReleaseCondition", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RELEASE_CONDITION_THRESHOLD_MUST_BE_POSITIVE, name: "ReleaseConditionThresholdMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INVALID_RELEASE_COMPARISON, name: "InvalidReleaseComparison", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MAXIMUM_5_SELLERS_ALLOWED, name: "Maximum5SellersAllowed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::SELLER_ALLOCATIONS_MUST_SUM_TO_10000_BPS, name: "SellerAllocationsMustSumTo10000Bps", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DISPUTE_TIMEOUT_SECONDS_MUST_BE_POSITIVE, name: "DisputeTimeoutSecondsMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_CURRENT_HOLDER_CAN_TRANSFER_RECEIPT, name: "OnlyCurrentHolderCanTransferReceipt", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ACTIVE_MILESTONE_IN_PROGRESS, name: "ActiveMilestoneInProgress", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSPECTION_PENDING, name: "InspectionPending", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_LISTED_BUYER_CAN_APPROVE_MULTI_BUYER_RELEASE, name: "OnlyListedBuyerCanApproveMultiBuyerRelease", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BUYER_HAS_ALREADY_APPROVED_RELEASE, name: "BuyerHasAlreadyApprovedRelease", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_ARBITER_CAN_RELEASE_ESCROW, name: "OnlyBuyerOrArbiterCanReleaseEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::SELLER_VETO_ACTIVE, name: "SellerVetoActive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::SELLER_VETO_ACTIVE_2, name: "SellerVetoActive2", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CONDITION_NOT_MET, name: "ConditionNotMet", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_WAIVE_CONDITION, name: "OnlyBuyerOrSellerCanWaiveCondition", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_CONDITIONAL_RELEASE_SET_ESCROW, name: "NoConditionalReleaseSetEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_SUBMIT_EVIDENCE, name: "OnlyBuyerOrSellerCanSubmitEvidence", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MAXIMUM_EVIDENCE_ENTRIES_REACHED_PARTY, name: "MaximumEvidenceEntriesReachedParty", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_SET_RENEWAL_ALLOWANCE, name: "OnlyBuyerCanSetRenewalAllowance", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::AUTO_RENEW_IS_NOT_ENABLED_ESCROW, name: "AutoRenewIsNotEnabledEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_CANCEL_AUTO_RENEW, name: "OnlyBuyerCanCancelAutoRenew", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_CANCEL_AUTO_RENEWAL, name: "OnlyBuyerCanCancelAutoRenewal", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_AUTO_RENEW_CONFIG_SET_ESCROW, name: "NoAutoRenewConfigSetEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RELEASE_AMOUNT_MUST_BE_POSITIVE, name: "ReleaseAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RELEASE_AMOUNT_EXCEEDS_ESCROW_BALANCE, name: "ReleaseAmountExceedsEscrowBalance", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::AT_LEAST_ONE_MILESTONE_REQUIRED, name: "AtLeastOneMilestoneRequired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TOO_MANY_MILESTONES, name: "TooManyMilestones", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_AMOUNT_MUST_BE_POSITIVE, name: "MilestoneAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NEW_MILESTONES_MUST_START_AS_PENDING, name: "NewMilestonesMustStartAsPending", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_ALREADY_TERMINAL, name: "EscrowAlreadyTerminal", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_ARBITER_CAN_APPROVE_MILESTONES, name: "OnlyBuyerOrArbiterCanApproveMilestones", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_INDEX_OUT_RANGE, name: "MilestoneIndexOutRange", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_NOT_PENDING, name: "MilestoneNotPending", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_DISPUTE_ESCROW, name: "OnlyBuyerOrSellerCanDisputeEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DISPUTE_AMOUNT_OUT_OF_RANGE, name: "DisputeAmountOutOfRange", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BUYER_PERCENT_MUST_BE_BETWEEN_0_AND_100, name: "BuyerPercentMustBeBetween0And100", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_DISPUTED, name: "EscrowIsNotDisputed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ARBITER_CAN_RESOLVE_DISPUTE, name: "OnlyArbiterCanResolveDispute", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_COOLING_OFF_STATE, name: "EscrowIsNotCoolingOffState", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_FLAG_RESOLUTION_ERROR, name: "OnlyBuyerOrSellerCanFlagResolutionError", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::COOLING_OFF_WINDOW_HAS_EXPIRED, name: "CoolingOffWindowHasExpired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RESOLUTION_ALREADY_FLAGGED, name: "ResolutionAlreadyFlagged", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::COOLING_OFF_WINDOW_HAS_NOT_ELAPSED, name: "CoolingOffWindowHasNotElapsed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RESOLUTION_IS_FLAGGED_ADMIN_MUST_REVIEW_BEFORE_FINALIZATION, name: "ResolutionIsFlaggedAdminMustReviewBeforeFinalization", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_CLEAR_RESOLUTION_FLAGS, name: "OnlyAdminCanClearResolutionFlags", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_FLAG_TO_CLEAR, name: "NoFlagToClear", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_CONFIGURE_COOLING_OFF_PERIOD, name: "OnlyAdminCanConfigureCoolingOffPeriod", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::FEE_CONFIGURATION_EXCEEDS_ESCROW_AMOUNT, name: "FeeConfigurationExceedsEscrowAmount", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TIMEOUT_MUST_BE_POSITIVE, name: "TimeoutMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MULTIPLIER_MUST_BE_POSITIVE, name: "MultiplierMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DEADLINE_MUST_BE_POSITIVE, name: "DeadlineMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DISPUTE_ALREADY_RESOLVED, name: "DisputeAlreadyResolved", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DISPUTE_TIMEOUT_DEADLINE_HAS_NOT_PASSED_YET, name: "DisputeTimeoutDeadlineHasNotPassedYet", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MAX_ORACLE_AGE_MUST_BE_POSITIVE, name: "MaxOracleAgeMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSURANCE_TRIGGER_DAYS_MUST_BE_POSITIVE, name: "InsuranceTriggerDaysMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSURANCE_CONTRIBUTION_MUST_BE_POSITIVE, name: "InsuranceContributionMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_CLAIM_INSURANCE, name: "OnlyBuyerOrSellerCanClaimInsurance", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSURANCE_ALREADY_CLAIMED, name: "InsuranceAlreadyClaimed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ADMIN_CONFIRMATION_REQUIRED, name: "AdminConfirmationRequired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSURANCE_TRIGGER_PERIOD_NOT_REACHED, name: "InsuranceTriggerPeriodNotReached", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_TOKEN_NOT_COVERED_BY_INSURANCE_POOL, name: "EscrowTokenNotCoveredByInsurancePool", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSURANCE_POOL_HAS_INSUFFICIENT_BALANCE, name: "InsurancePoolHasInsufficientBalance", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::FEE_EXCEEDS_MAXIMUM_200_BPS, name: "FeeExceedsMaximum200Bps", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::WITHDRAWAL_AMOUNT_MUST_BE_POSITIVE, name: "WithdrawalAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSUFFICIENT_ACCRUED_FEES, name: "InsufficientAccruedFees", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RELEASE_CONDITION_NOT_MET, name: "ReleaseConditionNotMet", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_HAS_NOT_EXPIRED_YET, name: "EscrowHasNotExpiredYet", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_PROPOSE_DEADLINE_EXTENSION, name: "OnlyBuyerOrSellerCanProposeDeadlineExtension", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CANNOT_EXTEND_DEADLINE_WHILE_ESCROW_IS_DISPUTED, name: "CannotExtendDeadlineWhileEscrowIsDisputed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NEW_DEADLINE_MUST_BE_GREATER_THAN_CURRENT_DEADLINE, name: "NewDeadlineMustBeGreaterThanCurrentDeadline", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_ACCEPT_DEADLINE_EXTENSION, name: "OnlyBuyerOrSellerCanAcceptDeadlineExtension", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PROPOSER_CANNOT_ACCEPT_THEIR_OWN_DEADLINE_EXTENSION, name: "ProposerCannotAcceptTheirOwnDeadlineExtension", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DEADLINE_EXTENSION_PROPOSAL_HAS_EXPIRED, name: "DeadlineExtensionProposalHasExpired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_PROPOSE_AMENDMENT, name: "OnlyBuyerOrSellerCanProposeAmendment", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CANNOT_AMEND_TERMINAL_ESCROW, name: "CannotAmendTerminalEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NEW_AMOUNT_MUST_BE_POSITIVE, name: "NewAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_SIGN_AMENDMENT, name: "OnlyBuyerOrSellerCanSignAmendment", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::AMENDMENT_NONCE_MISMATCH, name: "AmendmentNonceMismatch", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::AMENDMENT_PROPOSAL_HAS_EXPIRED, name: "AmendmentProposalHasExpired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::AMENDMENT_REQUIRES_BUYER_AND_SELLER_SIGNATURES, name: "AmendmentRequiresBuyerAndSellerSignatures", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_TOP_UP_ESCROW, name: "OnlyBuyerCanTopUpEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_ACTIVE_OR_AWAITING_INSPECTION, name: "EscrowIsNotActiveOrAwaitingInspection", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ADDITIONAL_AMOUNT_MUST_BE_POSITIVE, name: "AdditionalAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TOP_UP_LIMIT_EXCEEDED, name: "TopUpLimitExceeded", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SELLER_CAN_ACKNOWLEDGE_TOP_UP, name: "OnlySellerCanAcknowledgeTopUp", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SELLER_CAN_REQUEST_PARTIAL_RELEASE, name: "OnlySellerCanRequestPartialRelease", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PARTIAL_RELEASE_ONLY_ALLOWED_ACTIVE_ESCROW, name: "PartialReleaseOnlyAllowedActiveEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PARTIAL_RELEASE_AMOUNT_MUST_BE_POSITIVE, name: "PartialReleaseAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PARTIAL_RELEASE_AMOUNT_CANNOT_EXCEED_ESCROW_AMOUNT, name: "PartialReleaseAmountCannotExceedEscrowAmount", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::REQUEST_ALREADY_PENDING, name: "RequestAlreadyPending", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_APPROVE_PARTIAL_RELEASE, name: "OnlyBuyerCanApprovePartialRelease", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INVALID_REQUEST_ID, name: "InvalidRequestID", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DELEGATE_MUST_BE_DIFFERENT_FROM_SELLER, name: "DelegateMustBeDifferentFromSeller", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::SELLER_NOT_PART_OF_ESCROW, name: "SellerNotPartOfEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CAN_ONLY_DELEGATE_BEFORE_ESCROW_IS_RELEASED, name: "CanOnlyDelegateBeforeEscrowIsReleased", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_REJECT_PARTIAL_RELEASE, name: "OnlyBuyerCanRejectPartialRelease", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SELLER_CAN_ESCALATE_PARTIAL_RELEASE, name: "OnlySellerCanEscalatePartialRelease", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RESPONSE_DEADLINE_NOT_YET_PASSED, name: "ResponseDeadlineNotYetPassed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NEW_BUYER_MUST_BE_DIFFERENT_FROM_CURRENT_BUYER, name: "NewBuyerMustBeDifferentFromCurrentBuyer", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BUYER_TRANSFER_ONLY_ALLOWED_ACTIVE_ESCROWS, name: "BuyerTransferOnlyAllowedActiveEscrows", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_CURRENT_BUYER_CAN_TRANSFER_BUYER_ROLE, name: "OnlyCurrentBuyerCanTransferBuyerRole", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_UPDATE_METADATA, name: "OnlyBuyerOrSellerCanUpdateMetadata", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_UPGRADE_CONTRACT, name: "OnlyAdminCanUpgradeContract", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_MIGRATE_CONTRACT, name: "OnlyAdminCanMigrateContract", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MIGRATION_ALREADY_COMPLETED_VERSION, name: "MigrationAlreadyCompletedVersion", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::UNLOCK_AT_MUST_BE_FUTURE, name: "UnlockAtMustBeFuture", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ALREADY_CLAIMED, name: "AlreadyClaimed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BENEFICIARY_CAN_CLAIM, name: "OnlyBeneficiaryCanClaim", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::UNLOCK_TIME_HAS_NOT_PASSED, name: "UnlockTimeHasNotPassed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_NOT_ACTIVE, name: "EscrowNotActive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PAST_UNLOCK_TIME_USE_CLAIM_TIMELOCKED, name: "PastUnlockTimeUseClaimTimelocked", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_CANCEL, name: "OnlyBuyerCanCancel", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DISPUTE_ACTIVE, name: "DisputeActive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_SET_TOKEN_WHITELIST_CONTRACT, name: "OnlyAdminCanSetTokenWhitelistContract", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CONTRACT_ALREADY_PAUSED, name: "ContractAlreadyPaused", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CONTRACT_IS_NOT_PAUSED, name: "ContractIsNotPaused", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TOKEN_NOT_ALLOWED, name: "TokenNotAllowed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::DEADLINE_DURATION_MUST_BE_POSITIVE, name: "DeadlineDurationMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TEMPLATE_IS_DEACTIVATED, name: "TemplateIsDeactivated", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ARBITER_ALREADY_POOL, name: "ArbiterAlreadyPool", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ARBITER_NOT_POOL, name: "ArbiterNotPool", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ARBITER_POOL_IS_EMPTY, name: "ArbiterPoolIsEmpty", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_TEMPLATE_CREATOR_CAN_UPDATE, name: "OnlyTemplateCreatorCanUpdate", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_TEMPLATE_CREATOR_CAN_DEACTIVATE, name: "OnlyTemplateCreatorCanDeactivate", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TEMPLATE_ALREADY_DEACTIVATED, name: "TemplateAlreadyDeactivated", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INACTIVITY_RELEASE_IS_NOT_ENABLED_ESCROW, name: "InactivityReleaseIsNotEnabledEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ESCROW_SELLER_CAN_CLAIM_INACTIVITY_RELEASE, name: "OnlyEscrowSellerCanClaimInactivityRelease", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BUYER_INACTIVITY_WINDOW_HAS_NOT_ELAPSED, name: "BuyerInactivityWindowHasNotElapsed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PENALTY_CANNOT_EXCEED_10000_BPS, name: "PenaltyCannotExceed10000Bps", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RESPONSE_WINDOW_MUST_BE_POSITIVE, name: "ResponseWindowMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_REQUEST_CANCELLATION, name: "OnlyBuyerOrSellerCanRequestCancellation", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_PENDING_CANCELLATION_ESCROW, name: "NoPendingCancellationEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INITIATOR_CANNOT_ACCEPT_THEIR_OWN_CANCELLATION_REQUEST, name: "InitiatorCannotAcceptTheirOwnCancellationRequest", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_ACCEPT_CANCELLATION, name: "OnlyBuyerOrSellerCanAcceptCancellation", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INITIATOR_CANNOT_REJECT_THEIR_OWN_CANCELLATION_REQUEST, name: "InitiatorCannotRejectTheirOwnCancellationRequest", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_REJECT_CANCELLATION, name: "OnlyBuyerOrSellerCanRejectCancellation", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RESPONSE_WINDOW_HAS_NOT_ELAPSED, name: "ResponseWindowHasNotElapsed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BOUNTY_AMOUNT_MUST_BE_POSITIVE, name: "BountyAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CLAIM_DEADLINE_MUST_BE_FUTURE, name: "ClaimDeadlineMustBeFuture", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::SUBMISSION_DEADLINE_MUST_BE_AFTER_CLAIM_DEADLINE, name: "SubmissionDeadlineMustBeAfterClaimDeadline", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TOKEN_NOT_WHITELISTED, name: "TokenNotWhitelisted", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BOUNTY_IS_NOT_AVAILABLE_CLAIMING, name: "BountyIsNotAvailableClaiming", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CLAIM_DEADLINE_HAS_PASSED, name: "ClaimDeadlineHasPassed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BOUNTY_IS_NOT_CLAIMED_STATUS, name: "BountyIsNotClaimedStatus", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ASSIGNED_SOLVER_CAN_SUBMIT_WORK, name: "OnlyAssignedSolverCanSubmitWork", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::SUBMISSION_DEADLINE_HAS_PASSED, name: "SubmissionDeadlineHasPassed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_APPROVE_SUBMISSION, name: "OnlyBuyerCanApproveSubmission", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_SUBMISSION_HAS_BEEN_MADE, name: "NoSubmissionHasBeenMade", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_REJECT_SUBMISSION, name: "OnlyBuyerCanRejectSubmission", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MAXIMUM_REJECTION_ROUNDS_REACHED, name: "MaximumRejectionRoundsReached", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_CANCEL_BOUNTY, name: "OnlyBuyerCanCancelBounty", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CANNOT_CANCEL_BOUNTY_CURRENT_STATE, name: "CannotCancelBountyCurrentState", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_SET_MAX_BOUNTY_REJECTION_ROUNDS, name: "OnlyAdminCanSetMaxBountyRejectionRounds", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BOUNTY_MUST_HAVE_AT_LEAST_ONE_MILESTONE, name: "BountyMustHaveAtLeastOneMilestone", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::BOUNTY_MUST_BE_CLAIMED_BEFORE_SUBMITTING_MILESTONES, name: "BountyMustBeClaimedBeforeSubmittingMilestones", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SOLVER_CAN_SUBMIT_MILESTONES, name: "OnlySolverCanSubmitMilestones", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_INDEX_OUT_BOUNDS, name: "MilestoneIndexOutBounds", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PREVIOUS_MILESTONE_NOT_YET_VERIFIED, name: "PreviousMilestoneNotYetVerified", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_IS_NOT_AWAITING_SUBMISSION, name: "MilestoneIsNotAwaitingSubmission", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_IS_NOT_AWAITING_VERIFICATION, name: "MilestoneIsNotAwaitingVerification", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BOUNTY_CREATOR_CAN_REPLACE_VERIFIER, name: "OnlyBountyCreatorCanReplaceVerifier", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::VERIFIER_CAN_ONLY_BE_REPLACED_BEFORE_MILESTONE_IS_SUBMITTED, name: "VerifierCanOnlyBeReplacedBeforeMilestoneIsSubmitted", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_CONFIGURE_COLLATERAL_HEALTH, name: "OnlyBuyerCanConfigureCollateralHealth", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MIN_COLLATERAL_RATIO_BPS_OUT_OF_RANGE, name: "MinCollateralRatioBpsOutOfRange", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TOP_UP_AMOUNT_MUST_BE_POSITIVE, name: "TopUpAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_ACTIVE_OR_UNDER_COLLATERALIZED, name: "EscrowIsNotActiveOrUnderCollateralized", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_CONFIGURE_MULTI_PARTY_APPROVAL, name: "OnlyBuyerCanConfigureMultiPartyApproval", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::APPROVERS_COUNT_MUST_BE_BETWEEN_2_AND_10, name: "ApproversCountMustBeBetween2And10", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::THRESHOLD_MUST_BE_BETWEEN_1_AND_APPROVERS_COUNT, name: "ThresholdMustBeBetween1AndApproversCount", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CANNOT_RECONFIGURE_APPROVALS_ALREADY_PROGRESS, name: "CannotReconfigureApprovalsAlreadyProgress", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CALLER_IS_NOT_AUTHORIZED_APPROVER_ESCROW, name: "CallerIsNotAuthorizedApproverEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::APPROVER_HAS_ALREADY_APPROVED_ESCROW, name: "ApproverHasAlreadyApprovedEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RELEASE_SCHEDULE_MUST_CONTAIN_AT_LEAST_ONE_TRANCHE, name: "ReleaseScheduleMustContainAtLeastOneTranche", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::EACH_TRANCHE_AMOUNT_MUST_BE_POSITIVE, name: "EachTrancheAmountMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::EACH_TRANCHE_UNLOCK_AT_MUST_BE_FUTURE, name: "EachTrancheUnlockAtMustBeFuture", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BENEFICIARY_SELLER_CAN_CLAIM_SCHEDULED_RELEASES, name: "OnlyBeneficiarySellerCanClaimScheduledReleases", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_CLAIMABLE_STATE, name: "EscrowIsNotClaimableState", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_TRANCHES_ARE_CURRENTLY_CLAIMABLE, name: "NoTranchesAreCurrentlyClaimable", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::CONTRACT_IS_PAUSED, name: "ContractIsPaused", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_PAUSE_CONTRACT, name: "OnlyAdminCanPauseContract", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_RESUME_CONTRACT, name: "OnlyAdminCanResumeContract", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_STILL_LOCKED, name: "EscrowStillLocked", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ORACLE_PRICE_IS_STALE, name: "OraclePriceIsStale", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INVALID_ORACLE_PRICE, name: "InvalidOraclePrice", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INSUFFICIENT_RENEWAL_ALLOWANCE, name: "InsufficientRenewalAllowance", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_RENEWAL_DURATION_MUST_BE_POSITIVE, name: "EscrowRenewalDurationMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_SET_MAX_TOP_UP_BPS, name: "OnlyAdminCanSetMaxTopUpBps", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::COLLATERAL_FORFEIT_BPS_CANNOT_EXCEED_10000, name: "CollateralForfeitBpsCannotExceed10000", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::AT_LEAST_ONE_SELLER_REQUIRED, name: "AtLeastOneSellerRequired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SELLER_CAN_DEPOSIT_COLLATERAL, name: "OnlySellerCanDepositCollateral", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_IS_NOT_AWAITING_COLLATERAL, name: "EscrowIsNotAwaitingCollateral", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::COLLATERAL_DEPOSIT_WINDOW_HAS_EXPIRED, name: "CollateralDepositWindowHasExpired", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RATING_MUST_BE_BETWEEN_1_AND_5, name: "RatingMustBeBetween1And5", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RATING_ONLY_ALLOWED_AFTER_ESCROW_IS_RELEASED_OR_RESOLVED, name: "RatingOnlyAllowedAfterEscrowIsReleasedOrResolved", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_OR_SELLER_CAN_SUBMIT_RATING, name: "OnlyBuyerOrSellerCanSubmitRating", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RATING_ALREADY_SUBMITTED_ESCROW, name: "RatingAlreadySubmittedEscrow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SELLER_CAN_SUBMIT_DELIVERY_PROOF, name: "OnlySellerCanSubmitDeliveryProof", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::PROOF_SUBMISSION_LOCKED_ESCROW_IS_UNDER_DISPUTE, name: "ProofSubmissionLockedEscrowIsUnderDispute", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INVALID_DELIVERY_PROOF, name: "InvalidDeliveryProof", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ESCROW_SELLER_CAN_RAISE_VETO, name: "OnlyEscrowSellerCanRaiseVeto", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::VETO_COOLDOWN_ACTIVE, name: "VetoCooldownActive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_APPROVE, name: "OnlyBuyerCanApprove", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::NO_PENDING_SELLER_TRANSFER, name: "NoPendingSellerTransfer", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_SET_VETO_OVERRIDE_WINDOW, name: "OnlyAdminCanSetVetoOverrideWindow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::WINDOW_SECONDS_MUST_BE_POSITIVE, name: "WindowSecondsMustBePositive", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ESCROW_SELLER_CAN_CANCEL_VETO, name: "OnlyEscrowSellerCanCancelVeto", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::VETO_WINDOW_ELAPSED, name: "VetoWindowElapsed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_OVERRIDE_SELLER_VETO, name: "OnlyAdminCanOverrideSellerVeto", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ACTIVE_DISPUTE_EXISTS, name: "ActiveDisputeExists", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::VETO_WINDOW_NOT_ELAPSED, name: "VetoWindowNotElapsed", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::VETO_WINDOW_HAS_NOT_EXPIRED_YET, name: "VetoWindowHasNotExpiredYet", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_CURRENT_SELLER_CAN_INITIATE_TRANSFER, name: "OnlyCurrentSellerCanInitiateTransfer", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ESCROW_MUST_BE_ACTIVE_TO_TRANSFER_SELLER_ROLE, name: "EscrowMustBeActiveToTransferSellerRole", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_BUYER_CAN_VETO, name: "OnlyBuyerCanVeto", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ADMIN_CAN_SET_VETO_WINDOW, name: "OnlyAdminCanSetVetoWindow", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::RELEASE_BPS_MUST_BE_POSITIVE_EACH_MILESTONE, name: "ReleaseBpsMustBePositiveEachMilestone", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_RELEASE_BPS_MUST_SUM_TO_10000, name: "MilestoneReleaseBpsMustSumTo10000", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ESCROW_SELLER_MAY_SUBMIT_MILESTONES, name: "OnlyEscrowSellerMaySubmitMilestones", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_MUST_BE_PENDING_OR_REJECTED_TO_SUBMIT, name: "MilestoneMustBePendingOrRejectedToSubmit", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::MILESTONE_MUST_BE_SUBMITTED_BEFORE_APPROVAL, name: "MilestoneMustBeSubmittedBeforeApproval", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_ESCROW_BUYER_MAY_REJECT_MILESTONES, name: "OnlyEscrowBuyerMayRejectMilestones", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_SUBMITTED_MILESTONE_CAN_BE_REJECTED, name: "OnlySubmittedMilestoneCanBeRejected", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::ONLY_LOSING_PARTY_CAN_FLAG_RESOLUTION_ERROR, name: "OnlyLosingPartyCanFlagResolutionError", contract: "ahjoor-escrow" },

    // refund (8 entries — must match refund::COUNT)
    ErrorEntry { code: refund::ALREADY_INITIALIZED, name: "AlreadyInitialized", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::FEE_EXCEEDS_MAXIMUM, name: "FeeExceedsMaximum", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::AMOUNT_MUST_BE_POSITIVE, name: "AmountMustBePositive", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::INVALID_REASON_CODE, name: "InvalidReasonCode", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::REFUND_COOLDOWN_ACTIVE, name: "RefundCooldownActive", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::PAYMENT_NOT_FOUND, name: "PaymentNotFound", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::PAYMENT_NOT_COMPLETED, name: "PaymentNotCompleted", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::EXCEEDS_REFUNDABLE_AMOUNT, name: "ExceedsRefundableAmount", contract: "ahjoor-refund" },

    // whitelist (8 entries — must match whitelist::COUNT)
    ErrorEntry { code: whitelist::NOT_INITIALIZED, name: "NotInitialized", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::ALREADY_INITIALIZED, name: "AlreadyInitialized", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::UNAUTHORIZED, name: "Unauthorized", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::TOKEN_ALREADY_WHITELISTED, name: "TokenAlreadyWhitelisted", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::TOKEN_NOT_WHITELISTED, name: "TokenNotWhitelisted", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::QUOTA_EXCEEDED, name: "QuotaExceeded", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::TOKEN_ALREADY_HAS_QUOTA, name: "TokenAlreadyHasQuota", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::TOKEN_HAS_NO_QUOTA, name: "TokenHasNoQuota", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::RISK_TIER_NOT_DEFINED, name: "RiskTierNotDefined", contract: "ahjoor-token-whitelist" },
];

/// Look up an `ErrorEntry` by its numeric code.
///
/// Performs the linear search over `ALL_ERRORS` once, for reuse by off-chain
/// tooling and tests instead of every caller writing its own scan.
pub fn error_name_from_code(code: u32) -> Option<&'static ErrorEntry> {
    ALL_ERRORS.iter().find(|entry| entry.code == code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_name_from_code_returns_known_entries() {
        let entry = error_name_from_code(rosca::ALREADY_INITIALIZED).expect("known code");
        assert_eq!(entry.name, "AlreadyInitialized");
        assert_eq!(entry.contract, "ahjoor-rosca");

        let entry = error_name_from_code(payments::RATE_LIMIT_EXCEEDED).expect("known code");
        assert_eq!(entry.name, "RateLimitExceeded");
        assert_eq!(entry.contract, "ahjoor-payments");

        let entry = error_name_from_code(escrow::INVALID_DEADLINE).expect("known code");
        assert_eq!(entry.name, "InvalidDeadline");
        assert_eq!(entry.contract, "ahjoor-escrow");

        let entry = error_name_from_code(refund::ALREADY_INITIALIZED).expect("known code");
        assert_eq!(entry.name, "AlreadyInitialized");
        assert_eq!(entry.contract, "ahjoor-refund");

        let entry = error_name_from_code(whitelist::NOT_INITIALIZED).expect("known code");
        assert_eq!(entry.name, "NotInitialized");
        assert_eq!(entry.contract, "ahjoor-token-whitelist");
    }

    #[test]
    fn error_name_from_code_returns_none_for_unrecognized_code() {
        assert!(error_name_from_code(9_999_999).is_none());
    }

    #[test]
    fn no_duplicate_codes() {
        let mut seen = std::vec::Vec::new();
        for entry in ALL_ERRORS {
            assert!(
                !seen.contains(&entry.code),
                "Duplicate error code {} ({}::{})",
                entry.code,
                entry.contract,
                entry.name,
            );
            seen.push(entry.code);
        }
    }

    /// Guards against the exact failure mode that motivated this registry:
    /// a `pub const` added to a module without a matching `ALL_ERRORS` entry
    /// (or an entry added to the wrong contract/range). Each module exposes
    /// a `COUNT` const alongside its error codes; this test cross-checks
    /// that count against how many `ALL_ERRORS` entries are tagged for that
    /// contract. If someone adds a new error code, they must also bump the
    /// module's `COUNT` and add an `ALL_ERRORS` entry — if either step is
    /// skipped, this test fails.
    #[test]
    fn all_errors_covers_every_module_const() {
        let expected: &[(&str, usize)] = &[
            ("ahjoor-rosca", rosca::COUNT),
            ("ahjoor-payments", payments::COUNT),
            ("ahjoor-escrow", escrow::COUNT),
            ("ahjoor-refund", refund::COUNT),
            ("ahjoor-token-whitelist", whitelist::COUNT),
        ];

        for (contract, count) in expected {
            let actual = ALL_ERRORS.iter().filter(|e| &e.contract == contract).count();
            assert_eq!(
                actual, *count,
                "{contract}: ALL_ERRORS has {actual} entries but the module declares COUNT = {count}. \
                 A const was added/removed without updating the other.",
            );
        }

        let total_expected: usize = expected.iter().map(|(_, c)| c).sum();
        assert_eq!(
            ALL_ERRORS.len(),
            total_expected,
            "ALL_ERRORS contains entries for a contract not covered by this test",
        );
    }

    #[test]
    fn codes_within_contract_ranges() {
        for entry in ALL_ERRORS {
            let in_range = match entry.contract {
                "ahjoor-rosca"            => (1000..=1299).contains(&entry.code),
                "ahjoor-payments"         => (2000..=2299).contains(&entry.code),
                "ahjoor-escrow"           => (3000..=3299).contains(&entry.code),
                "ahjoor-refund"           => (4000..=4099).contains(&entry.code),
                "ahjoor-token-whitelist"  => (5000..=5099).contains(&entry.code),
                _                         => false,
            };
            assert!(
                in_range,
                "Error code {} ({}) is outside the expected range for {}",
                entry.code,
                entry.name,
                entry.contract,
            );
        }
    }
}
