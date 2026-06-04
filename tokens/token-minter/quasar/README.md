# Token Minter (Quasar)

Mint tokens using the [Classic Token Program](https://solana.com/docs/terminology#token-program).

See also: [Token Minter overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Mint authority
- Token account init

## Setup

From `tokens/token-minter/quasar/`:

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
