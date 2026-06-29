# Transfer Hook - Allow/Block List (Quasar)

Allow/block list enforced by a transfer hook program.

See also: [Allow Block List Token overview](../README.md) and the [repository catalog](../../../../../README.md).

## Major concepts

- Transfer hook
- List authority

## Setup

From `tokens/token-extensions/transfer-hook/allow-block-list-token/quasar/`:

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
