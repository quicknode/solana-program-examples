# cNFT Vault (Quasar)

Deposit and withdraw compressed NFTs from a PDA vault.

See also: the [repository catalog](../../../README.md).

## Major concepts

- cNFT transfers
- PDA vault

## Setup

From `compression/cnft-vault/quasar/`:

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
