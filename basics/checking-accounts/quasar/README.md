# Checking Accounts (Quasar)

Validate signers, owners, and addresses on incoming [instructions](https://solana.com/docs/terminology#instruction).

See also: [Checking Accounts overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Compile-time account checks
- Signer and mut constraints

## Setup

From `basics/checking-accounts/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

In-process tests via **Quasar SVM** (`quasar-svm` in `Quasar.toml`):

```bash
cargo test
```

Tests invoke instruction handlers and assert onchain state. No local validator.

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant in the same example where present.
