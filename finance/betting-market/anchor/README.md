# Betting Market

A parimutuel (pooled) betting market. An admin opens an **event**, adds the possible
**outcomes**, and bettors stake a token on the outcome they think will win. Every stake across
every outcome goes into one pool. When the admin settles the event to the winning outcome, the
losing stakes — minus a protocol fee — are split among the winners in proportion to their stake.

This is the pooled model used by Solana prediction-market platforms such as Hedgehog Markets,
where odds are set by the crowd's stakes rather than by an order book or a fixed-odds bookmaker.

## Purpose

It solves the core problem of trustless betting: collecting stakes from many bettors, holding them
in one place no single bettor controls, and paying winners by a fixed, public formula. The pool is
a token account owned by the event's PDA, so payouts are signed by the program with the event's
seeds — there is no admin key that can move bettors' stakes out of the pool. The admin's only
powers are creating events/outcomes and choosing the winning outcome (or cancelling).

## Major Concepts

### Accounts

- **Config** (`seeds = [b"config"]`) — one per deployment. Holds the `admin` (the only key that can
  create events/outcomes, settle, and cancel), the `token_mint` every market accepts, the
  `fee_recipient`, and the `fee_bps`.
- **Event** (`seeds = [b"event", event_id]`) — one betting market. Tracks `total_pool`, `status`
  (`Open` / `Settled` / `Cancelled`), and — once settled — the `winning_outcome_index`,
  `winning_pool`, and `distributable_losing_pool` that the payout formula reads. The `fee_bps` is
  snapshotted at creation so later Config changes can't alter a market bettors have already joined.
- **Outcome** (`seeds = [b"outcome", event, index]`) — one possible result. Its `total_amount` is
  the outcome's share of the pool and the denominator for pro-rata payouts when it wins.
- **Bet** (`seeds = [b"bet", outcome, bettor]`) — a bettor's total stake on one outcome. Re-betting
  the same outcome adds to the existing Bet, so there is exactly one per (outcome, bettor).
- **User** (`seeds = [b"user", wallet]`) — a per-wallet index listing the bettor's Bet addresses, so
  a client can find someone's positions without scanning every Bet on the program. The list is
  capped (see `MAX_BETS_PER_USER`) to keep the account a fixed size; the Bet accounts are the
  authoritative stake record.

### The vault

Each event owns a single vault token account — the associated token account of the Event PDA for
`config.token_mint`. `place_bet` moves the stake from the bettor's token account into this vault.
`settle_event`, `claim_winnings`, and `claim_refund` move tokens back out, with the program signing
as the Event PDA (`seeds = [b"event", event_id, bump]`).

### Payout formula

When an event settles to a winning outcome:

```
losing_pool             = total_pool - winning_pool
fee                     = losing_pool * fee_bps / 10000      // charged only on the losing side
distributable_losing    = losing_pool - fee
```

Each winning bet then claims:

```
payout = stake + stake * distributable_losing / winning_pool
```

A winner always gets their own stake back; the fee is only ever taken from losing stakes. Integer
division floors each share, leaving at most a few base units of dust in the vault.

**Worked example:** Outcome A pool 100, Outcome B pool 50, `fee_bps = 200` (2%). A wins.
`losing_pool = 50`, `fee = 1`, `distributable_losing = 49`. A bettor who staked 40 claims
`40 + 40 * 49 / 100 = 59`.

### Instruction handlers

| Handler | Who | What it does |
| --- | --- | --- |
| `initialize_config` | anyone (becomes admin) | One-time setup: sets admin, stake token, fee, fee recipient. |
| `create_event` | admin | Opens a market and creates its vault. |
| `add_outcome` | admin | Adds a possible result. Only before any bet is placed. |
| `place_bet` | bettor | Stakes tokens on one outcome; updates the pools and the user's index. |
| `settle_event` | admin | Resolves to a winning outcome, takes the fee, records the payout figures. |
| `claim_winnings` | winning bettor | Withdraws stake plus pro-rata share of the losing pool. |
| `cancel_event` | admin | Voids an unresolved market. |
| `claim_refund` | bettor | After a cancellation, reclaims the exact stake. |

`add_outcome` is locked once betting starts, so the field of choices can't change under existing
bettors. `settle_event` rejects a winning outcome with no bets — use `cancel_event` to unwind an
event that can't be resolved fairly.

## Setup

Install the [Solana CLI](https://docs.anza.xyz/cli/install) (provides `cargo-build-sbf`) and
[Anchor](https://www.anchor-lang.com/docs/installation). Build the program so the test binary
exists on disk:

```sh
anchor build
```

## Testing

Tests are Rust integration tests running against [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm)
with [solana-kite](https://crates.io/crates/solana-kite) helpers. They cover the full lifecycle
(bet → settle → claim with exact payout and fee assertions), admin authorization, the
bet-after-settle and double-claim guards, settling an outcome with no bets, and the cancel/refund
path.

```sh
anchor test
```

(`Anchor.toml` sets `test = "cargo test"`, so `cargo test` works too.)
