# Transaction v1 and this repository

Solana's v1 transaction format ([SIMD-0385](https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0385-transaction-v1.md), sized by [SIMD-0296](https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0296-larger-transactions.md)) raises the maximum transaction size from 1,232 to 4,096 bytes. This page is the repository's view of it: what changes, where it is live, which of the tools these examples build on can send it, and what that means for the examples. The worked example is [`basics/transaction-v1/`](../basics/transaction-v1/anchor/), in Anchor v2, Anchor v1, Pinocchio and native Rust.

Status is as of 3 September 2026.

## What changes

| | Legacy and v0 | v1 |
| --- | --- | --- |
| Maximum size | 1,232 bytes | 4,096 bytes |
| Compute unit limit, heap size, loaded-accounts data size limit | ComputeBudget program instructions | `TransactionConfig` fields in the message header |
| Priority fee | `SetComputeUnitPrice`, in micro-lamports per compute unit | `TransactionConfig::priority_fee`, a total in lamports |
| Unset resource limits | Runtime defaults (200,000 compute units per instruction, 64 MiB of account data) | Zero. Only the heap size falls back, to 32 KiB |
| ComputeBudget instructions present | Applied | Ignored |
| Address lookup tables | v0 only | Not supported |
| Account addresses | Bounded by size (v0: up to 64 with lookup tables) | Up to 64, all inline, no duplicates |
| Instructions | Bounded by size | Up to 64 |
| Signatures | Bounded by size | Up to 12 |
| Wire format | Signature count first | Message version byte first (`0x81`), signatures last |

Two of these bite in practice. A v1 transaction that does not set its compute unit limit fails on its first instruction, and one that does not set its loaded-accounts data size limit fails before any program runs, with `MaxLoadedAccountsDataSizeExceeded`. The example's `unset_config_fields_mean_zero_not_the_default` test shows both.

The other one is the lookup tables. v1 is bigger, not more flexible: a transaction that touches more accounts than fit inline in 4,096 bytes still needs v0 and a lookup table. The vault strategy example's deposit, with five accounts per basket asset, is that case.

## What changes for programs

Nothing. A program sees the same accounts and the same instruction data whether a legacy, v0 or v1 transaction carried them, so no program in this repository, in any framework, needs a change to be called through v1. The one visible difference is to programs that read the transaction's ComputeBudget instructions through the instructions sysvar, for instance to detect a priority fee: a v1 transaction has none, and the config that replaced them is not exposed to programs.

## What changes for tests and clients

Everything that builds a transaction has to know the format. That is the test suites here, and every client.

### Sending a v1 transaction in Rust

`solana-message` 4.2 or later has the `v1` module. The example's tests do exactly this against LiteSVM:

```rust
use solana_message::{v1, VersionedMessage};
use solana_transaction::versioned::VersionedTransaction;

let config = v1::TransactionConfig::empty()
    .with_compute_unit_limit(20_000)
    .with_loaded_accounts_data_size_limit(64 * 1024)
    // A total in lamports, not a price per compute unit.
    .with_priority_fee(5_000);

let message = v1::Message::try_compile_with_config(
    &payer.pubkey(),
    &[instruction],
    recent_blockhash,
    config,
)?;
let transaction = VersionedTransaction::try_new(VersionedMessage::V1(message), &[&payer])?;
```

`wincode::serialize(&transaction)` gives the wire bytes, and `v1::MAX_TRANSACTION_SIZE` (4,096) and `solana_packet::PACKET_DATA_SIZE` (1,232) are the two limits to compare against. LiteSVM enforces neither, so a test that cares about size checks it itself.

### Sending a v1 transaction in TypeScript

`@solana/kit` 8.0.0 or later. The config setter replaces the ComputeBudget instruction helpers:

```typescript
const message = pipe(
  createTransactionMessage({ version: 1 }),
  (m) => setTransactionMessageFeePayerSigner(payer, m),
  (m) => setTransactionMessageLifetimeUsingBlockhash(latestBlockhash, m),
  (m) => appendTransactionMessageInstruction(instruction, m),
  (m) =>
    setTransactionMessageConfig(
      {
        computeUnitLimit: 20_000,
        loadedAccountsDataSizeLimit: 64 * 1024,
        priorityFeeLamports: 5_000n,
      },
      m,
    ),
);
```

Send and simulate with `encoding: "base64"`: base58 encoding stops at 1,232 bytes.

### Reading v1 transactions

Every `getTransaction`, `getBlock` and `blockSubscribe` call needs `maxSupportedTransactionVersion: 1` (the integer, not the string). Without it, `getTransaction` returns error `-32015`, `getBlock` fails for the whole block if it holds one v1 transaction, and `blockSubscribe` emits `block: null`. Indexers that scan for ComputeBudget instructions to read fees see zero for v1 transactions and have to read `transactionConfig` instead.

## Where v1 is live

The feature gate is `enable_tx_v1`, `txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL`, shipped in Agave 4.2.

| Cluster | Status |
| --- | --- |
| `solana-test-validator` (Agave 4.2+) and Surfpool 1.5+ | Active at genesis |
| Testnet | Active since epoch 1025 |
| Devnet | Pending activation (Agave 4.2.2) |
| Mainnet beta | Pending. Anza's stated target is 9 September 2026 |

Check a cluster before sending:

```bash
solana -u <cluster> feature status txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL
```

## Tool support

What the toolchains this repository uses can do with v1, checked against the versions the examples pin.

| Tool | Used here as | v1 |
| --- | --- | --- |
| Agave CLI and `solana-test-validator` | CI installs 3.1.14; the example pins 4.2.2 | 4.2.0 and later |
| `solana-message`, `solana-transaction` (Rust) | 3.x in most tests; 4.2 / 4.1 in the example | `solana-message` 4.2 and later |
| [LiteSVM](https://github.com/LiteSVM/litesvm) | 0.13.1 everywhere but the example, which uses 0.16.0 | 0.16.0 (24 August 2026) and later. Reads the compute budget from the config |
| [Surfpool](https://surfpool.run/) | `anchor test` and `anchor localnet` default validator | 1.5 and later |
| `@solana/kit` | Not used by the tests | 8.0.0 and later |
| `@solana/web3.js` 1.x | Two wallet-adapter demo apps | Reads v1 from 1.99.0. Cannot build one. `@solana/web3.js` 3.0.0-rc.3 and later can |
| Anchor v2 programs (`anchor-lang` 2.0.0-rc.1) | 46 examples | No change needed |
| `anchor-v2-testing` (the Anchor v2 LiteSVM harness) | Every Anchor v2 example but this one | Not yet. Pins LiteSVM 0.13.1, on the pinned revision and on the tip of `anchor-next` as of today |
| Anchor v1 programs (`anchor-lang` 1.1.2) | 37 examples | No change needed |
| `@coral-xyz/anchor` 1.1.2 (Anchor's TypeScript client) | Not used by the tests | Not yet. Depends on `@solana/web3.js` 1.x, which cannot build a v1 transaction |
| Quasar programs (`quasar-lang` 0.1.0) | 53 examples | No change needed |
| `quasar-test` and `quasar-svm` (Quasar's test harness and VM) | Every Quasar example | Not yet. `quasar-svm` 0.1.0 is on `solana-message` 3.x and its `master` branch on 4.1, one release short of the `v1` module |
| Pinocchio, native Rust and sBPF assembly programs | 50 examples | No change needed |
| [`solana-kite`](https://github.com/solanakite/kite-rust) 0.4.0 | 111 test suites | Not yet. Pins LiteSVM 0.13.1 |

## What that means for the examples

Only `basics/transaction-v1/` sends v1 transactions. Every other example's tests still send legacy transactions, and they keep passing, because nothing on the program side changed.

Moving the rest of the repository is blocked on the two test dependencies, not on the examples:

1. `solana-kite` needs a release on LiteSVM 0.16 or later. 111 test suites here depend on it, and LiteSVM 0.13.1 and 0.16.0 cannot share a lockfile (each pins a different `solana-instruction` 3.x patch release), so the root workspace cannot move until it does.
2. `anchor-v2-testing` needs to move off its `=0.13.1` pin. Until then an Anchor v2 example can send v1 transactions only by calling LiteSVM directly, as the example does, and loses `anchor test --profile`, `anchor debugger` and `anchor coverage` for it.
3. Quasar needs `quasar-svm` on `solana-message` 4.2 or later, and a way for a `#[quasar_test]` test to choose the transaction format. Neither exists yet, so there is no Quasar variant of the example.

That is also why the example's four directories are standalone Cargo workspaces, deliberately absent from the root `Cargo.toml` members list.

## Sources

- [Larger transaction sizes](https://solana.com/upgrades/larger-transaction-sizes), the upgrade page.
- [SIMD-0385: Transaction V1](https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0385-transaction-v1.md) and [SIMD-0296: Larger transactions](https://github.com/solana-foundation/solana-improvement-documents/blob/main/proposals/0296-larger-transactions.md).
- [solana-foundation/transaction-v1-examples](https://github.com/solana-foundation/transaction-v1-examples), runnable Rust, TypeScript, Python and Go clients and indexers, with a minimum-version table.
- [The `transactions-v1` reference in solana-dev-skill](https://github.com/solana-foundation/solana-dev-skill/blob/main/skills/solana-dev/references/transactions-v1.md).
- [Feature gate tracker schedule](https://github.com/anza-xyz/agave/wiki/Feature-Gate-Tracker-Schedule), for activation status per cluster.
- [LiteSVM 0.16.0](https://github.com/LiteSVM/litesvm/releases/tag/v0.16.0), the release that added v1 support.
- [Solana v1 transactions explained](https://www.quicknode.com/blog/solana-v1-transactions-explained), on the Quicknode blog.
