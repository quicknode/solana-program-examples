# Anchor Escrow

## Introduction

This Solana program is an **escrow** — it lets a user swap a specific amount of one token for a desired amount of another token.

For example: Alice offers 10 USDC and wants 100 WIF in return.

Without an escrow, users would have to swap tokens manually and trust each other. The escrow program acts as a trusted third party that only releases tokens to both sides when the swap can complete atomically. Neither party can take the other's tokens and run.

Alice and Bob transact directly with each other through the program, so there's no spread or middleman fee taken on the swap.

## Usage

Run the tests with `pnpm test` (as configured in `Anchor.toml`).

## Credit

Based on [Dean Little's Anchor Escrow](https://github.com/deanmlittle/anchor-escrow-2024), with a few changes to make it easier to discuss in class.

### Changes from the original

One challenge when teaching is avoiding ambiguity — names have to be clear and not confused with anything else.

- Several custom handler functions were replaced by helpers from `@solana-developers/helpers` to reduce file size.
- Shared token-transfer logic now lives in `instructions/shared.rs`.
- The upstream project uses a custom file layout. This version uses the 'multiple files' Anchor layout.
- Contexts are separate data structures from the functions that use them. There's no need for OO-style `impl` patterns here — no mutable state is stored in the context, and the methods don't mutate it.
- The name 'deposit' was overloaded. `deposit` is both a verb and a noun, which made the code hard to read:
  - deposit #1 → `token_a_offered_amount`
  - deposit #2 (in `make()`) → `send_offered_tokens_to_vault`
  - deposit #3 (in `take()`) → `send_wanted_tokens_to_maker`
- `seed` was renamed to `id`, because it conflicted with the `seeds` used for PDA derivation.
- `Escrow` was used for both the program name and the account that records an offer. People kept confusing the offer account with the vault.
  - `Escrow` (the program) → still `Escrow`.
  - `Escrow` (the offer) → `Offer`.
- `receive` was renamed to `token_b_wanted_amount`, since `receive` is a verb and not a good name for an integer.
- `mint_a` → `token_mint_a` (what the maker offered and what the taker wants).
- `mint_b` → `token_mint_b` (what the maker wants and what the taker must offer).
- `makerAtaA` → `makerTokenAccountA`
- `makerAtaB` → `makerTokenAccountB`
- `takerAtaA` → `takerTokenAccountA`
- `takerAtaB` → `takerTokenAccountB`
