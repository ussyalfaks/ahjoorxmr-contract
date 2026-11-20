# Ahjoor

**Ahjoor** brings community savings circles (ROSCAs) on-chain with Soroban smart contracts on Stellar. Members pool a fixed amount each round and take turns receiving the pot. Every contribution and payout can be checked on-chain, and no middleman holds the money.

ROSCAs go by many names: *Ajo* and *Esusu* in West Africa, *Susu* in the Caribbean, *Tanda* in Latin America, and *Chit Funds* in South Asia.

## Contracts

| Contract | Role |
| --- | --- |
| `ahjoor-rosca` | Savings groups: rounds, contributions, payouts, governance |
| `ahjoor-escrow` | Escrow with arbiters, disputes, and timeouts |
| `ahjoor-payments` | Merchant payments with two-step authorize and capture |
| `ahjoor-refund` | Refund requests, approvals, and abuse scoring |
| `ahjoor-token-whitelist` | Admin-controlled list of tokens the other contracts accept |

Shared code lives in `ahjoor-common` and `ahjoor-errors`. Cross-contract tests are in `integration-tests`.

## Quick Start

You need [Rust](https://www.rust-lang.org/tools/install) (stable) and the [Stellar CLI](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup).

```bash
git clone https://github.com/Ahjoor/ahjoor-contract.git
cd ahjoor-contract

make build      # stellar contract build
make test       # cargo test
make fmt        # cargo fmt --all
make coverage   # needs cargo-llvm-cov; fails under 90% lines / 85% regions
```

### Deploy to testnet

```bash
stellar keys generate --global alice --network testnet
stellar keys fund alice --network testnet

stellar contract deploy \
  --wasm target/wasm32v1-none/release/ahjoor_rosca.wasm \
  --source alice \
  --network testnet
```

Save the `CONTRACT_ID` that `deploy` prints. Every later `stellar contract invoke` call needs it.

## Security at a Glance

- Every function that changes state calls `require_auth()`, and each one checks the caller's role (buyer, seller, arbiter and so on).
- Only the contract itself holds funds, and money moves only through explicit entry points.
- Only whitelisted SEP-41 tokens are accepted.
- Dispute timeouts stop an inactive arbiter from locking funds forever.
- `overflow-checks = true` makes arithmetic overflow panic instead of wrap.
- Each write extends the storage TTL. If a group goes quiet for a while, call `bump_storage()` about every 30 days.

## Documentation

You can find the full guides for each feature in the **[Documentation Index](docs/README.md)**. To contribute, read [CONTRIBUTING.md](CONTRIBUTING.md).

## Community

- [Telegram](https://t.me/ahjoor)
- [Stellar Docs](https://developers.stellar.org/)
