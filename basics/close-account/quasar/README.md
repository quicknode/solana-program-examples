# Close Account (Quasar)

Create a PDA [account](https://solana.com/docs/terminology#account), then close it and return [rent](https://solana.com/docs/terminology#rent) to the user.

See also: the [repository catalog](../../../README.md).

## Major concepts

- PDA init and close
- Rent reclamation
- `close_user` binds the user account to the signer's own PDA, so only the account's owner can close it

## Setup

From `basics/close-account/quasar/`:

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
