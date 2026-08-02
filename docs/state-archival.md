# State Archival Troubleshooting

Stellar/Soroban uses **State Archival** to manage network storage. Contracts and data entries that remain idle for long periods can be archived. This is a realistic scenario for ROSCA savings groups that go dormant for months.

Ahjoor extends TTL automatically on write paths (for example `init` or `contribute`), but inactive groups should still bump storage periodically. If archival does occur, it is fully reversible — no group data is lost.

## 1. Check if a contract is archived

Use the Stellar CLI to inspect contract status and TTL:

```bash
stellar contract info --id <CONTRACT_ID> --network testnet
```

Review the output for archival indicators (for example archived instance storage or expired TTL). Replace `testnet` with `mainnet` or `--local` as appropriate for your deployment.

## 2. Restore an archived contract

If the contract or its persistent entries were archived, restore them before invoking contract functions:

```bash
stellar contract restore \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet
```

Use an account that can pay the restore fee (typically a group admin or any funded participant). After restore completes, all prior contract invocations work again without redeploying.

## 3. Prevent archival

Call `bump_storage()` about every **30 days** during periods of inactivity to extend instance storage TTL:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source alice \
  --network testnet \
  -- bump_storage
```

Any member can call this function; it does not mutate ROSCA group state — it only extends storage lifetime.

## 4. What data is preserved

After a restore, **all ROSCA group state is preserved as-is**, including:

- Group configuration and membership
- Round/cycle progress and schedules
- Contribution records and payout history
- Penalties, suspensions, and related audit data

State archival on Stellar/Soroban does not delete contract logic or persistent storage contents; it moves idle entries to archived storage until restored.

## 5. What is lost

**Nothing.** State archival on Stellar/Soroban is fully reversible. Restoring brings back the same on-chain state that existed before archival — no redeployment or data migration is required.

## Related

- [README — State Archival & TTL](../README.md#state-archival--ttl)
- [Stellar State Archival documentation](http://web.archive.org/web/20240612170450/https://developers.stellar.org/docs/learn/smart-contract-internals/state-archival)
