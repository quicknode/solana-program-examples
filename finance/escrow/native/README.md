# Escrow (Native)

This Solana program is an **escrow** written directly against `solana-program`, with no framework. It lets a **maker** swap a specific amount of one token for a desired amount of another token with a **taker**, atomically and without either party having to trust the other.

For example: Alice offers 10 USDC and wants 100 WIF in return. The program holds Alice's USDC in a vault until someone delivers the WIF, then releases both sides in a single transaction.

See also the [Anchor](../anchor/) and [Quasar](../quasar/) variants of the same program.

## Accounts and PDAs

- **Offer**: a PDA with seeds `["offer", maker, id]` storing the offer's `id`, the `maker`, the two mint addresses (`token_mint_a` is what the maker offers, `token_mint_b` is what the maker wants), the `token_b_wanted_amount`, and the PDA `bump`. The `id` lets one maker keep multiple offers open at once.
- **Vault**: the offer PDA's associated token account for mint A. It holds the maker's offered tokens while the offer is open. Only the offer PDA can sign transfers out of it.

The maker pays the rent for the offer account and the vault (and for their own mint B token account if it does not exist yet). Every path that closes those accounts refunds that rent to the maker.

## Lifecycle

A maker opens an offer with the `MakeOffer` instruction, passing the offer `id`, the amount of token A offered, and the amount of token B wanted. The maker signs and pays all rent. The handler derives and creates the offer PDA, creates the vault, creates the maker's mint B token account if needed (so the eventual taker never pays rent for a maker-owned account), and moves the offered token A into the vault with `transfer_checked`. It then verifies the vault holds exactly the offered amount before writing the offer state.

A taker settles the offer with the `TakeOffer` instruction. The taker signs. The handler validates every passed account against the stored offer state (maker, both mints, the vault address, and the offer PDA itself), requires the maker's mint B token account to already exist, and lazily creates the taker's mint A token account (rent paid by the taker, since it is the taker's own account). It then transfers the wanted token B from the taker to the maker, releases the vault's token A to the taker signed by the offer PDA, and verifies conservation with checked arithmetic: the taker gained exactly the vault balance and the maker gained exactly the wanted amount. Finally it closes the vault and the offer account, refunding both rents to the maker.

A maker abandons an offer with the `CancelOffer` instruction. Only the maker can call it; without it, an unwanted offer would lock the maker's tokens in the vault forever. The handler validates the accounts against the offer state, returns the vault's token A to the maker's token account, verifies the maker received exactly the vault balance, and closes the vault and offer accounts back to the maker.

Errors are reported through the named `EscrowError` enum in `program/src/error.rs` (key mismatches, missing maker token account, conservation violations, arithmetic overflow).

## Setup

Prerequisites: the [Agave](https://docs.anza.xyz/) toolchain (`cargo build-sbf`) and Rust.

Build the program into the test fixtures directory:

```bash
cargo build-sbf --manifest-path=./program/Cargo.toml --sbf-out-dir=./tests/fixtures
```

(`npm run build-and-test` in `package.json` runs the same command.)

## Testing

The tests run against [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm), loading the `.so` built above. After building, run:

```bash
cargo test --manifest-path=./program/Cargo.toml
```

The tests cover the make/take flow, the make/cancel flow, rejection of a non-maker cancel, token balances on every leg, and the rent refunds (the maker's lamports recover the offer and vault rent after both take and cancel).
