## Reserve Fund

This document describes the  Reserve Fund implementation in the Ahjoor Refund contract. It explains the purpose of the reserve fund, who funds it, when reserve balances are used based on the current implementation, and how reserve balances are funded, maintained, and updated.

### Overview

The Reserve Fund is a merchant-specific balance maintained by the refund contract. It allows merchants to deposit funds into a reserve account and enforces a minimum reserve requirement based on the merchant's recorded payment volume.

Each merchant has an independent reserve balance that can be:

* Deposited into
* Withdrawn from (subject to reserve requirements)
* Checked for compliance with the configured reserve policy

### Reserve funding

The merchant funds the Reserve Fund. Tokens are first minted to the merchant's balance who then uses part of it to fund the reserve by calling `deposit_reserve()`.
The reserve balance can be viewed using `get_merchant_reserve()`.

The required reserve is calculated from the configured reserve ratio and the merchant's recorded payment volume. The administrator configures the required reserve percentage in basis points (bps) using `set_reserve_ratio_bps()`, while merchants record payment volume through `record_payment_volume()`. 


### When the Reserve Fund is drawn on vs. using merchant direct funds

Direct merchant funds include a merchant's wallet balance outside the Ahjoor Refund contract while the reserve fund equals tokens held inside the contract under the merchant's reserve account.

In the current implementation, there is still no logic showing the reserve being used to satisfy a refund nor does it indicate  using the merchant's direct funds.

Instead, the tests highlight `withdraw_reserve()`, which allows a merchant to withdraw from their reserve balance, provided the withdrawal does not go below the required minimum reserve.

The contract prevents withdrawals that reduce the reserve below the required minimum; if a withdrawal breaches this, it panics with `WithdrawalWouldBreachMinimum`. 

Compliance checks use the reserve balance to decide if a merchant is compliant; if a merchant lacks required reserve, they are non-compliant. (`check_reserve_compliance` called by admin).

### Reserve Funds Balance Replenishment

The reserve balance replenishes when a merchant deposits funds using either `deposit_reserve()` (legacy reserve) or `deposit_merchant_reserve()` (canonical reserve).

The contract also provides `migrate_merchant_reserve()`, which transfers any existing legacy reserve balance into the canonical reserve storage. 
Migration preserves the merchant's total reserve balance by consolidating funds into the canonical reserve rather than creating additional funds. 

The tests also show the `record_payment_volume` call changes the required minimum, but does not automatically replenish the reserve.

## References
* `contracts\ahjoor-refund\src\test_reserve_fund.rs`