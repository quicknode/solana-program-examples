# Token Swap (AMM)

A Constant Product Automated Market Maker (AMM) in [Anchor](https://solana.com/docs/terminology#anchor) — the model popularized by Uniswap V2.

The pool keeps `x * y = K` invariant: if `x` is the reserve of token A and `y` is the reserve of token B, then `x * y` stays constant for a given liquidity quantity.

## Why a CPAMM

Other bonding-curve designs exist:

- **Constant Sum AMM (CSAMM):** `x + y = K`. Constant price but reserves can be drained.
- **Curve Stableswap:** a mix of CSAMM and CPAMM, tuned for like-priced assets.
- **Uniswap V3 Concentrated Liquidity AMM (CLAMM):** splits the curve into buckets; LPs supply liquidity to specific price ranges.
- **Trader Joe CLAMM:** like Uniswap V3, but each bucket is a CSAMM.

A CPAMM is the simplest and the cheapest to keep in [account](https://solana.com/docs/terminology#account) state — one pool, one [mint](https://solana.com/docs/terminology#token-mint), easy to reason about. That's what this example implements.

## Design

Requirements:

- **Fee distribution.** Every pool charges a trading fee, paid in the traded token, that rewards LPs. To stay consistent across pools, the fee is shared.
- **Single pool per asset pair.** Avoids liquidity fragmentation.
- **LP accounting.** The [program](https://solana.com/docs/terminology#program) tracks each LP's deposits.

Implementation choices:

- **Shared parameters.** A single AMM account stores the shared trading-fee config and admin. Each pool then has its own account.
- **Unique pools.** Each pool is a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) seeded from the AMM, `mint_a`, and `mint_b` (in that order, with `mint_a < mint_b`).
- **LP accounting via tokens.** LP positions are tracked as tokens (the `mint_liquidity` mint), so they're composable with any wallet or downstream protocol.

## Onchain-design principles applied here

- **Store keys in the account.** Even for PDAs, storing the parent keys in the account state makes lookups easier (you can rebuild the PDA without consulting external data) and works well with Anchor's `has_one` constraint.
- **Keep seeds simple.** Start with the parent's seeds, then the current object's identifiers in alphabetical order. For the pool, that means `[amm, mint_a, mint_b]`.
- **Keep [instruction](https://solana.com/docs/terminology#instruction) scope small.** Smaller instructions touch fewer accounts, leaving room in the transaction and improving composability and security.

## File structure

```text
programs/token-swap/src/
├── constants.rs
├── errors.rs
├── instructions
│   ├── create_amm.rs
│   ├── create_pool.rs
│   ├── deposit_liquidity.rs
│   ├── mod.rs
│   ├── swap_exact_tokens_for_tokens.rs
│   └── withdraw_liquidity.rs
├── lib.rs
└── state.rs
```

## State

### `Amm`

- `id: Pubkey` — the primary key of the AMM (used as a seed).
- `admin: Pubkey` — the admin authority.
- `fee: u16` — LP fee in basis points (must be < 10000).

### `Pool`

- `amm: Pubkey` — the parent AMM.
- `mint_a: Pubkey` — mint of token A.
- `mint_b: Pubkey` — mint of token B.

`Pool` PDA seeds: `[amm, mint_a, mint_b]` with `mint_a < mint_b`.

## Instruction handlers

### `create_amm`

Initializes an `Amm` account with the supplied `id`, `admin`, and `fee`. Enforces `fee < 10000`.

### `create_pool`

Initializes a `Pool` account, an LP mint (`mint_liquidity`), and the two pool [ATAs](https://solana.com/docs/terminology#associated-token-account-ata) (`pool_account_a`, `pool_account_b`). Enforces `mint_a < mint_b` for canonical pool addressing.

### `deposit_liquidity`

Transfers `amount_a` and `amount_b` from the depositor to the pool, then mints LP tokens to the depositor.

- For the first deposit, the LP amount is `sqrt(amount_a * amount_b)`, with `MINIMUM_LIQUIDITY` locked away forever (to prevent the empty-pool edge case).
- For later deposits, the amounts are scaled to match the current pool ratio.

### `swap_exact_tokens_for_tokens`

Swaps a fixed `input_amount` of one token for as much of the other as possible (subject to `min_output_amount`).

- The trading fee is taken off the input first (`taxed_input = input * (10_000 - fee) / 10_000`).
- The output is computed against the current `pool_a` and `pool_b` balances.
- After the swap, the invariant `pool_a * pool_b` is checked to ensure it has not decreased.

### `withdraw_liquidity`

Burns LP tokens and returns the proportional share of `pool_a` and `pool_b` to the LP. The proportion is `amount / (mint_liquidity.supply + MINIMUM_LIQUIDITY)`.

## Tests

Run `pnpm test` from the example directory.
