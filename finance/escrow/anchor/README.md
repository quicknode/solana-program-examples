# Solana Escrow (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

This Solana [program](https://solana.com/docs/terminology#program) is an **escrow** - it lets a **maker** swap a specific amount of one token for a desired amount of another token with a **taker**, atomically and without either party having to trust the other.

For example: Alice offers 10 USDC and wants 100 WIF in return. The program holds Alice's USDC in a vault until someone delivers the WIF, then releases both sides in a single transaction. Neither party can take the other's tokens and run, and there is no spread or middleman fee on the swap.

See also the [native](../native/) and [Quasar](../quasar/) variants of the same program.

## Accounts and PDAs

- **Offer**: a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) with seeds `["offer", maker, id]` storing the offer `id`, the `maker`, the two mints (`token_mint_a` is what the maker offers, `token_mint_b` is what the maker wants), the `token_b_wanted_amount`, and the PDA `bump`. The `id` lets one maker keep multiple offers open at once.
- **Vault**: the offer PDA's associated token account for token A. It holds the maker's offered tokens while the offer is open; only the offer PDA can sign transfers out of it.

The maker pays the rent for the offer account and the vault, and every path that closes them (`take_offer`, `cancel_offer`) refunds that rent to the maker.

## Lifecycle

A maker opens an offer with `make_offer`, passing the `id`, `token_a_offered_amount`, and `token_b_wanted_amount`. The maker signs and pays all rent. The handler creates the offer PDA and the vault, creates the maker's token-B associated token account if needed (paid by the maker, so the eventual taker never funds a maker-owned account), moves the offered token A into the vault with `transfer_checked`, and records the offer state.

A taker settles the offer with `take_offer`. The taker signs. Anchor's constraints bind every account to the stored offer state (`address = offer.maker` on the maker and `address = offer.token_mint_a` / `address = offer.token_mint_b` on the mints, associated-token constraints on the vault and all token accounts, and the PDA seeds on the offer itself). The handler sends the wanted token B from the taker to the maker, releases the vault's token A to the taker signed by the offer PDA, and closes both the vault and the offer account back to the maker, who paid their rent. The taker's own token-A account is created on the fly if needed, paid by the taker.

A maker abandons an offer with `cancel_offer`. Only the maker can call it; without it, an unwanted offer would lock the maker's tokens in the vault forever. The handler returns the vault's token A to the maker and closes the vault and offer accounts, refunding both rents to the maker.

## Setup

Prerequisites: Rust, the [Agave](https://docs.anza.xyz/) toolchain, and the Anchor v2 CLI. Build the program with:

```bash
anchor build
```

(or `cargo build-sbf` from `programs/escrow/`). The tests load the resulting `target/deploy/escrow.so`.

## Testing

The tests are Rust integration tests running against [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) (with [solana-kite](https://crates.io/crates/solana-kite) helpers). After building, run:

```bash
cargo test
```

(`anchor test` runs the same command, per `Anchor.toml`.) The tests cover the make/take flow, the make/cancel flow, rejection of a non-maker cancel, token balances on every leg, and the rent refunds (the maker's lamports recover the offer and vault rent after both take and cancel).

## FAQ

### How does an escrow work on Solana?

A Solana escrow is a program that holds a maker's tokens in a program-controlled vault until a taker delivers the tokens the maker asked for, then releases both sides in one atomic transaction. This example implements the whole lifecycle in three instruction handlers: `make_offer`, `take_offer`, and `cancel_offer`.

### Is this a good first Solana finance program to learn?

Yes. Escrow is the smallest complete finance program: one state PDA, one vault, three instruction handlers, and the atomic swap idea that underlies every onchain exchange. Start here before the [AMM](../../token-swap/anchor/), [order book](../../order-book/anchor/), and [lending](../../lending/anchor/) examples.

### How do I run and test this escrow example?

Build with `anchor build`, then run `cargo test`. The tests are Rust integration tests against [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm), so no local validator is needed.

### How is this escrow program verified?

Two ways: LiteSVM integration tests covering the make, take, and cancel flows, and [Kani](https://github.com/model-checking/kani) proofs in [`../kani-proofs/`](../kani-proofs/) that check the money-math invariants over all possible inputs, not just test cases.

## Credit

Based on [Dean Little's Anchor Escrow](https://github.com/deanmlittle/anchor-escrow-2024), restructured for teaching.
