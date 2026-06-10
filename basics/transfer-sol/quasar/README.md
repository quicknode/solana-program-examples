# Transfer SOL (Quasar)

Transfer native SOL via the System Program.

See also: [Transfer Sol overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- System transfer CPI
- Signer-funded lamports
- Direct lamport moves (`transfer_sol_with_program`) require the payer to be owned by this program, enforced by an account constraint, with checked balance math

## Setup

From `basics/transfer-sol/quasar/`:

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
