# Create Token (Quasar)

Create a token mint and mint tokens to a token account.

The Anchor variant also creates Metaplex metadata; this Quasar variant focuses on the core SPL Token operations. Quasar's metadata crate is demonstrated in the [nft-operations](../../nft-operations/quasar/) example.

See also: [Create Token overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- `create_token` takes a `decimals` instruction argument and initializes the mint with it (create_account + initialize_mint2 CPIs)
- `mint_tokens` takes `amount` in minor units, the raw integer the token program operates on

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
