# Create Token (Quasar)

Create a mint with metadata using Token and Metaplex programs.

See also: [Create Token overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Mint + metadata CPI
- See [tokens/create-token/README.md](../create-token/README.md)

## Setup

From `tokens/create-token/quasar/`:

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
