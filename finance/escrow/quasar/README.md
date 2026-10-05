# Solana Escrow (Quasar)

An atomic token swap escrow on Solana, written with Quasar: the program holds a maker's tokens in a vault until a taker delivers the tokens the maker asked for, then releases both sides in one transaction.

See also: the [repository catalog](../../../README.md).

## Major concepts

- **Offer**: a PDA with seeds `["offer", maker, id]` (the same seeds as the Anchor variant, so clients work against either build). It stores the maker, both mints, the maker's token B account, the vault address, the wanted `receive` amount, and the bump. `take_offer` and `cancel_offer` validate every passed account against this stored state via `has_one` bindings.
- **Vault**: a token account owned by the offer PDA holding the maker's offered token A while the offer is open.
- The maker pays the rent for the offer account and the vault in `make_offer`; both `take_offer` and `cancel_offer` close those accounts back to the maker.
- `take_offer` takes `minimum_token_a_out` (the least token A the taker accepts from the vault) and `maximum_token_b_in` (the most token B the taker will pay), signed by the taker. An offer's address comes from its maker and `id`, so while a taker's transaction is in flight the maker could cancel the offer and make it again under the same `id` at worse terms, and the transaction would land on the new offer. Before any token moves, `take_offer` refuses the take with `OfferTermsChanged` if the vault holds less token A than `minimum_token_a_out` or the offer's `receive` is more than `maximum_token_b_in`. `test_take_offer_rejects_switched_offer` and `test_take_offer_rejects_switched_offer_wanting_more_token_b` in `src/tests.rs` run that switch and check the take fails with the taker's tokens untouched.
- See the [Anchor variant](../anchor/README.md) for the full walkthrough.

## Setup

From `finance/escrow/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

In-process tests with the `quasar-test` harness (`cargo test`, the command `Quasar.toml` names):

```bash
quasar build
cargo test
```

Tests invoke instruction handlers and assert onchain state. No local validator. They tell one story, the book's: token A is USDC, minted at 6 decimals, token B is TSLAx at 8, and the maker offers 250 USDC (`USDC_OFFERED`, 250,000,000 minor units) for 1 TSLAx (`TSLAX_WANTED`, 100,000,000 minor units), which the taker takes. Both start with the standard 1,000 USDC, and the taker also holds 1 TSLAx, so the take leaves the taker at 1,250 USDC and no TSLAx. They cover the make, take and cancel flows with their rent refunds, a take that lands on an offer the maker cancelled and re-made at worse terms (`OfferTermsChanged`), offers with zero on either side (`ZeroAmount`) or one token on both (`AccountBorrowFailed`), and a substituted mint, a substituted vault and a signer who is not the maker (each `HasOneMismatch`). Every refusal test asserts the error code it expects.

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant in the same example where present.
