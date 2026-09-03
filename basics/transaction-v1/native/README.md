# Solana Transaction v1 (Native)

A Solana transaction v1 example: a program that stores a 3,000 byte document in a single [instruction](https://solana.com/docs/references/terminology#instruction), which only fits inside the v1 [transaction](https://solana.com/docs/references/terminology#transaction) format (4,096 bytes, up from the 1,232 bytes of legacy and v0 transactions). The program is written directly against `solana-program`, with no framework. Nothing in the program is specific to v1: it reads its accounts and instruction data the same way whichever format carried them. The tests are where v1 shows up, because the client builds the transaction.

See also the [Anchor v2](../anchor/), [Anchor v1](../anchor-v1/) and [Pinocchio](../pinocchio/) variants of the same program, and [docs/transaction-v1.md](../../../docs/transaction-v1.md) for what v1 changes, its activation status, and which tools support it.

## Major concepts

**What v1 changes for the transaction.** The size limit rises from 1,232 to 4,096 bytes, which is what makes this example possible. The resource limits that legacy and v0 transactions set with ComputeBudget program instructions (compute unit limit, priority fee, loaded-accounts data size limit, heap size) move into a `TransactionConfig` in the message header, and any ComputeBudget instructions in a v1 transaction are ignored. The priority fee becomes a total in lamports rather than a price per compute unit. Unset config fields mean zero, not "use the default", so every v1 transaction has to set its compute unit limit and its loaded-accounts data size limit explicitly. The signatures move to the end of the wire format, so the first byte is the message version (`0x81` for v1). Address lookup tables are not supported: a transaction that needs them stays on v0.

**What v1 changes for the program.** Nothing. The 3,000 byte document arrives as instruction data exactly as a 300 byte one would.

**The document account.** A [PDA](https://solana.com/docs/references/terminology#program-derived-account-pda) with seeds `["document", payer]`, one per payer, created by the program with a size equal to the document and owned by the program. The payer pays the [rent](https://solana.com/docs/references/terminology#rent).

## Lifecycle

A payer signs a transaction carrying one instruction: the `STORE_DOCUMENT` discriminator byte followed by the document. The `store_document` handler checks that the payer signed, that the document is not empty, that the third account is the System Program, and that the second account is the payer's document PDA. It then creates the PDA through a signed CPI to the System Program, sized to the document, and copies the document in.

## Setup

Prerequisites: the [Agave](https://docs.anza.xyz/) toolchain (`cargo build-sbf`) and Rust.

From this directory (`basics/transaction-v1/native/`):

```bash
cargo build-sbf --manifest-path=./program/Cargo.toml
```

This writes the program binary to `target/deploy/transaction_v1_native_program.so`.

This example is its own Cargo workspace rather than a member of the repository root workspace: the root pins LiteSVM 0.13.1, which predates v1, and the two LiteSVM versions cannot share a lockfile. `Cargo.toml` at this level explains the details.

## Testing

The Rust + [LiteSVM](https://www.litesvm.com/) tests load the `.so` built above. The binary is embedded at test-compile time, so rebuild after every program change or a stale `.so` silently tests old code. After building, run:

```bash
cargo test --manifest-path=./program/Cargo.toml
```

LiteSVM 0.16.0 is the first release that executes v1 transactions. The suite covers:

- `stores_a_document_too_large_for_a_legacy_transaction`: a 3,000 byte document in a v1 transaction. The test measures the transaction on the wire (over 1,232 bytes, at most 4,096) and reads the document back from the PDA.
- `a_legacy_transaction_still_works_for_a_small_document`: the same instruction with a 500 byte document, sent as a legacy transaction. The program did not change.
- `the_transaction_config_replaces_compute_budget_instructions`: a v1 transaction with a priority fee in the config. The payer is charged exactly rent plus the base fee plus that priority fee, and the wire format starts with the v1 version byte.
- `unset_config_fields_mean_zero_not_the_default`: an empty config fails before the program runs (zero bytes of account data may be loaded), and a config with only the data size limit fails on the program's first instruction (zero compute units).

LiteSVM does not enforce either size limit (a validator's packet layer does), which is why the size tests check the serialized transaction themselves.

## Sending v1 transactions to a real cluster

The v1 format is behind the `enable_tx_v1` feature gate (`txv1aq4pp281K9um3tnPgkfX8UqtFT6wcVW3hNezGLL`). It needs Agave 4.2 or later and, on the RPC side, `maxSupportedTransactionVersion: 1` on every `getTransaction`, `getBlock` and `blockSubscribe` call, plus `encoding: "base64"` when sending, since base58 encoding stops at 1,232 bytes. See [docs/transaction-v1.md](../../../docs/transaction-v1.md) for the activation status per cluster and the client libraries that can build v1 transactions.
