# Processing Instructions (Quasar)

Pass arguments into an [instruction handler](https://solana.com/docs/terminology#instruction-handler).

See also: [Processing Instructions overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Instruction data
- Handler parameters

## Setup

From `basics/processing-instructions/quasar/`:

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
