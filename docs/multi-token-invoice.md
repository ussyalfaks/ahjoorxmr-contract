# Multi-Token Invoice

This document explains the multi-token invoice functionality in the Ahjoor Payments contract, which allows merchants to create invoices that can be paid in multiple different tokens.

## Overview

A `MultiTokenInvoice` is an invoice that:
- Is denominated in a **base currency** (e.g., USD)
- Accepts payments in **multiple tokens** (e.g., USDC, EURC, XLM)
- Converts received payments to a **preferred settlement token** for merchant payouts
- Optionally uses an **oracle contract** for real-time cross-token price discovery

## Key Concepts

### Base Currency
The currency in which the invoice total is denominated. All payments are converted to this base currency for accounting purposes.

### Accepted Tokens
A list of token addresses that the payer can use to settle the invoice. The payer must pay using one of these tokens.

### Preferred Settlement Token
The token that the merchant ultimately receives after settlement. All payments are converted from base currency to this token.

### Conversion Rates
- **Token-to-base rate**: Rate for converting each accepted token to the base currency (scaled by 1,000,000)
- **Settlement conversion rate**: Rate for converting base currency to the preferred settlement token (scaled by 1,000,000)

### Oracle Contract (Optional)
When configured, the invoice can use an oracle for dynamic price discovery instead of pre-set conversion rates. The oracle provides real-time exchange rates between tokens.

## Creating a Multi-Token Invoice

### Basic Creation (Without Oracle)

```rust
use soroban_sdk::{Address, Env, Map, String, Vec};
use crate::multi_token_invoice::{MultiTokenInvoiceImpl, InvoiceLineItem};

let merchant = Address::generate(&env);
let customer = Address::generate(&env);
let base_currency = usdc_address;  // Invoice denominated in USDC
let accepted_tokens = vec![&env, usdc_address, eurc_address, xlm_address];
let preferred_settlement_token = usdc_address;  // Merchant receives USDC

let line_items = vec![&env, InvoiceLineItem {
    description: String::from_str(&env, "Service A"),
    quantity: 2,
    unit_price: 50_000_000,  // 50.00 (scaled by 1M)
    amount: 100_000_000,     // 100.00 total
    tax_rate_bps: 0,
}];

let invoice_id = MultiTokenInvoiceImpl::create_invoice(
    &env,
    merchant,
    customer,
    100_000_000,              // Total amount in base currency
    base_currency,
    accepted_tokens,
    preferred_settlement_token,
    line_items,
    due_date,
    Map::new(&env),           // Metadata
);
```

### Creation With Oracle (Dynamic Pricing)

```rust
let oracle_address = Some(oracle_contract_address);

let invoice_id = MultiTokenInvoiceImpl::create_invoice_with_oracle(
    &env,
    merchant,
    customer,
    100_000_000,
    base_currency,
    accepted_tokens,
    preferred_settlement_token,
    line_items,
    due_date,
    Map::new(&env),
    oracle_address,  // Enable oracle-based pricing
);
```

## Setting Conversion Rates

Before payments can be accepted, conversion rates must be set for each accepted token.

### Setting Token-to-Base Conversion Rate

```rust
// Set rate for EURC to USDC: 1 EURC = 1.08 USDC (scaled by 1M)
MultiTokenInvoiceImpl::set_conversion_rate(
    &env,
    merchant,
    eurc_address,
    1_080_000,  // 1.08 * 1,000,000
);
```

### Setting Settlement Conversion Rate

```rust
// Set rate for base to settlement: 1:1 by default, can be changed
MultiTokenInvoiceImpl::set_settlement_conversion_rate(
    &env,
    merchant,
    invoice_id,
    1_000_000,  // 1:1 ratio
);
```

## Payer Token Selection and Payment

The payer selects which accepted token to use when making a payment. The contract validates that the token is in the `accepted_tokens` list.

### Standard Payment (Pre-set Conversion Rates)

```rust
let payment = MultiTokenInvoiceImpl::accept_payment(
    &env,
    invoice_id,
    payer_address,
    eurc_address,  // Payer chooses to pay in EURC
    92_592_593,    // Amount in EURC (≈100 USDC at 1.08 rate)
);
```

**Conversion calculation:**
- Payment amount in EURC: 92,592,593
- Conversion rate (EURC→USDC): 1,080,000
- Amount in base (USDC): (92,592,593 × 1,080,000) / 1,000,000 = 100,000,000

### Oracle-Based Payment (Dynamic Pricing)

When an oracle is configured, payers can use `pay_invoice_cross_token` with slippage protection:

```rust
let payment = MultiTokenInvoiceImpl::pay_invoice_cross_token(
    &env,
    invoice_id,
    payer_address,
    payment_token_address,
    payment_amount,
    50,  // max_slippage_bps: 0.5% tolerance
);
```

**Slippage validation:**
- The oracle provides the current price between `payment_token` and `base_currency`
- If a stored conversion rate exists, the oracle price is compared against it
- Payment is rejected if the deviation exceeds `max_slippage_bps` (basis points)
- Example: With `max_slippage_bps = 50`, a 0.5% deviation is allowed

## Conversion and Pricing Rules

### Rate Scaling
All conversion rates are scaled by 1,000,000 (6 decimal places) to maintain precision:
- Rate of 1.5 = 1,500,000
- Rate of 0.98 = 980,000

### Payment Conversion Flow

1. **Token → Base Currency**
   ```
   amount_in_base = (payment_amount × token_to_base_rate) / 1_000_000
   ```

2. **Base → Settlement Token**
   ```
   amount_in_settlement = (amount_in_base × settlement_conversion_rate) / 1_000_000
   ```

### Oracle Price Discovery

When using an oracle:
- The oracle returns price as `base / quote` scaled by 1,000,000
- For `get_price(payment_token, base_currency)`, returns how much base_currency 1 payment_token is worth
- Oracle prices must be positive and available
- Slippage checks protect against price manipulation

### Payment Limits

- **Line items**: Maximum 20 per invoice
- **Batch settlement**: Maximum 50 invoices per batch
- **Amounts**: Must be positive values
- **Overflow protection**: All arithmetic uses checked operations

## Invoice Status Lifecycle

Invoices transition through these statuses:

- `Draft`: Initial state (not currently used in creation flow)
- `Issued`: Invoice created and ready for payment
- `PartiallyPaid`: Some payments received but not full amount
- `FullyPaid`: Total amount received in base currency
- `Overdue`: Past due date (not automatically set, can be updated externally)
- `Cancelled`: Invoice cancelled by merchant

## Public Functions

### Invoice Creation

#### `create_invoice`
Creates a new multi-token invoice without oracle support.

**Parameters:**
- `env: &Env` - Soroban environment
- `merchant: Address` - Merchant address (requires auth)
- `customer: Address` - Customer address
- `total_amount: i128` - Total invoice amount in base currency
- `base_currency: Address` - Base currency token address
- `accepted_tokens: Vec<Address>` - List of payable token addresses
- `preferred_settlement_token: Address` - Token for merchant settlement
- `line_items: Vec<InvoiceLineItem>` - Invoice line items
- `due_date: u64` - Unix timestamp for due date
- `metadata: Map<String, String>` - Optional metadata

**Returns:** `u32` - The new invoice ID

#### `create_invoice_with_oracle`
Creates a new multi-token invoice with oracle support for dynamic pricing.

**Additional Parameter:**
- `oracle_contract: Option<Address>` - Oracle contract address for price discovery

### Payment Processing

#### `accept_payment`
Accepts a payment in an accepted token using pre-set conversion rates.

**Parameters:**
- `env: &Env` - Soroban environment
- `invoice_id: u32` - Invoice ID
- `payer: Address` - Payer address (requires auth)
- `token: Address` - Token address for payment
- `amount: i128` - Payment amount in the token

**Returns:** `InvoicePayment` - Payment record with conversion details

#### `pay_invoice_cross_token`
Accepts a payment using oracle-based price discovery with slippage protection.

**Additional Parameters:**
- `max_slippage_bps: u32` - Maximum acceptable slippage in basis points (e.g., 50 = 0.5%)

### Conversion Rate Management

#### `set_conversion_rate`
Sets the conversion rate for a specific token to base currency.

**Parameters:**
- `env: &Env` - Soroban environment
- `merchant: Address` - Merchant address (requires auth)
- `token: Address` - Token address
- `rate_to_base: i128` - Conversion rate scaled by 1,000,000

#### `set_settlement_conversion_rate`
Sets the conversion rate from base currency to settlement token.

**Parameters:**
- `env: &Env` - Soroban environment
- `merchant: Address` - Merchant address (requires auth)
- `invoice_id: u32` - Invoice ID
- `rate: i128` - Conversion rate scaled by 1,000,000

### Invoice Queries

#### `get_invoice`
Retrieves invoice details.

**Parameters:**
- `env: &Env` - Soroban environment
- `invoice_id: u32` - Invoice ID

**Returns:** `Option<MultiTokenInvoice>` - Invoice data or None if not found

#### `get_invoice_status`
Retrieves the current status of an invoice.

**Parameters:**
- `env: &Env` - Soroban environment
- `invoice_id: u32` - Invoice ID

**Returns:** `InvoiceStatus` - Current invoice status

#### `get_invoice_balance`
Retrieves the remaining unpaid balance in base currency.

**Parameters:**
- `env: &Env` - Soroban environment
- `invoice_id: u32` - Invoice ID

**Returns:** `i128` - Remaining balance in base currency

#### `get_invoice_payments`
Retrieves payment history for an invoice.

**Parameters:**
- `env: &Env` - Soroban environment
- `invoice_id: u32` - Invoice ID

**Returns:** `Vec<InvoicePayment>` - List of payments (currently returns empty - needs indexing)

### Settlement

#### `settle_invoices`
Batch settles fully paid invoices for a merchant.

**Parameters:**
- `env: &Env` - Soroban environment
- `merchant: Address` - Merchant address (requires auth)
- `invoice_ids: Vec<u32>` - List of invoice IDs to settle

**Returns:** `SettlementBatch` - Settlement batch record

#### `get_settlement_batch`
Retrieves settlement batch details.

**Parameters:**
- `env: &Env` - Soroban environment
- `batch_id: u32` - Batch ID

**Returns:** `Option<SettlementBatch>` - Batch data or None if not found

### Invoice Management

#### `cancel_invoice`
Cancels an invoice (merchant only).

**Parameters:**
- `env: &Env` - Soroban environment
- `invoice_id: u32` - Invoice ID

## Data Structures

### `MultiTokenInvoice`
```rust
pub struct MultiTokenInvoice {
    pub invoice_id: u32,
    pub merchant: Address,
    pub customer: Address,
    pub created_at: u64,
    pub due_date: u64,
    pub total_amount: i128,              // In base currency
    pub base_currency: Address,
    pub accepted_tokens: Vec<Address>,
    pub preferred_settlement_token: Address,
    pub line_items: Vec<InvoiceLineItem>,
    pub status: InvoiceStatus,
    pub payments_received: Map<Address, i128>,  // token -> amount in base
    pub conversion_rates: Map<Address, i128>,  // token -> rate to base
    pub settlement_conversion_rate: i128,      // base -> settlement
    pub oracle_contract: Option<Address>,
    pub metadata: Map<String, String>,
}
```

### `InvoicePayment`
```rust
pub struct InvoicePayment {
    pub payment_id: u32,
    pub invoice_id: u32,
    pub token: Address,              // Token used for payment
    pub amount: i128,                // Amount in payment token
    pub amount_in_base: i128,        // Converted to base currency
    pub amount_in_settlement: i128,  // Converted to settlement token
    pub paid_at: u64,
    pub payer: Address,
    pub tx_hash: BytesN<32>,
}
```

### `CrossTokenSettlementRecord`
```rust
pub struct CrossTokenSettlementRecord {
    pub invoice_id: u32,
    pub paid_token: Address,
    pub paid_amount: i128,
    pub invoiced_token: Address,     // Base currency
    pub invoiced_amount: i128,       // Oracle-derived equivalent
    pub oracle_price: i128,          // Price used (scaled by 1M)
    pub max_slippage_bps: u32,       // Slippage tolerance checked
}
```

## Error Handling

Common errors from `MultiTokenInvoiceError`:

- `InvoiceNotFound` - Invoice ID does not exist
- `InvalidInvoiceStatus` - Operation not allowed in current status
- `PaymentExceedsInvoiceAmount` - Payment would exceed total
- `TokenNotAccepted` - Payment token not in accepted list
- `ConversionRateNotSet` - No conversion rate configured for token
- `InvalidConversionRate` - Rate is zero or causes overflow
- `SlippageExceeded` - Oracle price deviation exceeds tolerance
- `OracleNotConfigured` - Oracle required but not set
- `OraclePriceUnavailable` - Oracle returned None or invalid price
- `UnauthorizedAccess` - Caller not authorized for operation

## Test Examples

See the following test files for behavior examples:

- `contracts/ahjoor-payments/src/test_dynamic_settlement.rs` - Multi-token payment with oracle integration
- `contracts/ahjoor-payments/src/test_oracle_staleness.rs` - Oracle staleness validation
- `contracts/ahjoor-payments/src/test.rs` - General payment and invoice tests

### Example Test Case

From `test_dynamic_settlement.rs`:

```rust
#[test]
fn test_create_payment_multi_token_success() {
    let (env, client, _admin, merchant, usdc_addr, pay_token_addr, 
         _usdc_client, usdc_admin_client, pay_token_client, 
         pay_token_admin_client) = setup_multi_token();

    let customer = Address::generate(&env);
    pay_token_admin_client.mint(&customer, &10_000_000);
    usdc_admin_client.mint(&client.address, &5_000_000);

    let pid = client.create_payment_multi_token(
        &customer,
        &merchant,
        &1_000_000,
        &pay_token_addr,
        &Some(50u32),  // 0.5% max slippage
    );

    client.complete_payment(&pid);
    let payment = client.get_payment(&pid);
    assert_eq!(payment.status, PaymentStatus::Completed);
    assert_eq!(payment.token, usdc_addr);  // Merchant received USDC
}
```

## Oracle Interface

The invoice oracle must implement the `InvoiceOracleInterface`:

```rust
#[contractclient(name = "InvoiceOracleClient")]
pub trait InvoiceOracleInterface {
    /// Returns the price of `base` in terms of `quote`, scaled by 1_000_000.
    /// Returns `None` if the price feed is unavailable.
    fn get_price(env: soroban_sdk::Env, base: Address, quote: Address) -> Option<i128>;
}
```

For `get_price(payment_token, base_currency)`:
- Returns how much `base_currency` 1 unit of `payment_token` is worth
- Scaled by 1,000,000 for precision
- Returns `None` if price is unavailable
