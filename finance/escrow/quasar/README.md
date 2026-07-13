# Solana Escrow (Quasar)

An atomic token swap escrow on Solana, written with Quasar: the program holds a maker's tokens in a vault until a taker delivers the tokens the maker asked for, then releases both sides in one transaction.

See also: the [repository catalog](../../../README.md).

## Major concepts

- **Offer**: a PDA with seeds `["offer", maker, id]` (the same seeds as the Anchor variant, so clients work against either build). It stores the maker, both mints, the maker's token B account, the vault address, the wanted `receive` amount, and the bump. `take_offer` and `cancel_offer` validate every passed account against this stored state via `has_one` bindings.
- **Vault**: a token account owned by the offer PDA holding the maker's offered token A while the offer is open.
- The maker pays the rent for the offer account and the vault in `make_offer`; both `take_offer` and `cancel_offer` close those accounts back to the maker.
- See the [Anchor variant](../anchor/README.md) for the full walkthrough.

## Setup

From `finance/escrow/quasar/`:

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
