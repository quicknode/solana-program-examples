# Counter (Quasar)

Global counter in a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) with initialize and increment handlers.

See also: [Counter overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- PDA state
- Handler dispatch

## Setup

From `basics/counter/quasar/`:

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
