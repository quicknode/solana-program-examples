# PDA Rent Payer (Quasar)

A [PDA](https://solana.com/docs/terminology#program-derived-address-pda) pays [rent](https://solana.com/docs/terminology#rent) when creating another account.

See also: [Pda Rent Payer overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- PDA signer
- Rent payer pattern

## Setup

From `basics/pda-rent-payer/quasar/`:

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
