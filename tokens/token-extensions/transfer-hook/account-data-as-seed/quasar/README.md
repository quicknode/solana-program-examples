# Transfer Hook — Account Data as Seed (Quasar)

Derive extra accounts from token account data in a transfer hook.

See also: the [repository catalog](../../../../../README.md).

## Major concepts

- Transfer hook
- Extra account metas

## Setup

From `tokens/token-extensions/transfer-hook/account-data-as-seed/quasar/`:

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
