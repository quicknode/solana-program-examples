# Solana Betting Market (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, on Anchor 1.2.0, the current
> stable Anchor release. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.2.0 && avm use 1.2.0`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

A parimutuel (pooled) betting market on Solana. An admin creates an **event**, adds the possible
**outcomes**, and opens it to bets; bettors then stake a token on the outcome they think will win,
until the event's betting close time. Every stake across
every outcome goes into one pool. When the admin settles the event to the winning outcome, the
losing stakes - minus a program fee - are split among the winners in proportion to their stake.

This is the pooled model used by Solana prediction-market platforms such as Hedgehog Markets,
where odds are set by the crowd's stakes rather than by an order book or a fixed-odds bookmaker.

## Purpose

It solves the core custody problem of pooled betting: collecting stakes from many bettors, holding
them in one place no single bettor controls, and paying winners by a fixed, public formula. Resolution
still requires trusting the admin, who chooses the winning outcome, as described below. The pool is
a token account owned by the event's PDA, so payouts are signed by the program with the event's
seeds - there is no admin key that can move bettors' stakes out of the pool. The admin's only
powers are creating events/outcomes, opening them to bets, choosing the winning outcome (or
cancelling), and closing a finished event's accounts once every bet has been paid out or closed.

## Major Concepts

### Accounts

- **Config** (`seeds = [b"config"]`) - one per deployment. Holds the `admin` (the only key that can
  create events/outcomes, open betting, settle, cancel, and close), the `token_mint` every market
  accepts, the `fee_recipient`, and the `default_fee_bps` each new event copies at creation. The
  config is the admin's long-lived state and stays open for the life of the deployment; no handler
  closes it.
- **Event** (`seeds = [b"event", event_id]`) - one betting market. Tracks `total_pool`, `status`
  (`Draft` / `Open` / `Settled` / `Cancelled`), `betting_closes_at`, and - once settled - the
  `winning_outcome_index`, `winning_pool`, and `distributable_losing_pool` that the payout formula
  reads. The event's `fee_bps` is copied from the config's `default_fee_bps` at creation and is what
  settlement charges. It and `betting_closes_at` are fixed at creation, so later Config changes
  can't alter a market bettors have already joined. Two counters say when the event can close:
  `open_bets`, the number of Bet accounts across every outcome that are still open (`place_bet`
  adds one when it creates a Bet account; a top-up reuses the account and adds nothing;
  `claim_winnings`, `claim_refund` and `close_losing_bet` each subtract one), and `open_outcomes`,
  the number of Outcome accounts still open (`add_outcome` adds one, `close_outcome` subtracts
  one). `close_event` closes the Event account and the vault, returning their rent to the admin.
- **Outcome** (`seeds = [b"outcome", event, index]`) - one possible result. Its `total_amount` is
  the outcome's share of the pool and the denominator for pro-rata payouts when it wins; its
  `bet_count` is the number of Bet accounts ever created on it. `close_outcome` closes it once the
  event is finished and every bet is closed, returning its rent to the admin.
- **Bet** (`seeds = [b"bet", outcome, bettor]`) - a bettor's total stake on one outcome. Re-betting
  the same outcome adds to the existing Bet, so there is exactly one per (outcome, bettor). The
  account exists only while the position is open: it closes (rent back to the bettor) via
  `claim_winnings`, `claim_refund`, or `close_losing_bet`, which is also what makes a second claim
  impossible. `bettor` is the first field after the 8-byte discriminator, so a client lists a
  wallet's open positions with `getProgramAccounts` and a `memcmp` filter on the wallet's address at
  offset 8. The program keeps no per-wallet index, so a wallet can hold any number of open bets.

### The vault

Each event owns a single vault token account - the associated token account of the Event PDA for
`config.token_mint`. `place_bet` moves the stake from the bettor's token account into this vault.
`settle_event`, `claim_winnings`, `claim_refund` and `close_event` move tokens back out, with the
program signing as the Event PDA (`seeds = [b"event", event_id, bump]`), and `close_event` then
closes the vault with the same signature, returning its rent to the admin.

### Payout formula

When an event settles to a winning outcome:

```
losing_pool             = total_pool - winning_pool
fee                     = ceil(losing_pool * fee_bps / 10000)  // charged only on the losing side, rounded up
distributable_losing    = losing_pool - fee                    // what the winners share
```

Each winning bet then claims:

```
payout = stake + stake * distributable_losing / winning_pool
```

A winner always gets their own stake back; the fee is only ever taken from losing stakes. Every
rounding goes the program's way: the fee is the ceiling of its fraction of the losing pool, and
each winner's share is floored, so the payouts and the fee together never exceed what the vault
holds. The flooring leaves at most a few minor units of dust in the vault, which `close_event` pays
to the fee recipient once every bet is closed.

**Example:** Outcome A pool 100, Outcome B pool 50, `fee_bps = 200` (2%). A wins.
`losing_pool = 50`, `fee = 1`, `distributable_losing = 49`. A bettor who staked 40 claims
`40 + 40 * 49 / 100 = 59`. Had the losing pool been 55, the fee would be `ceil(1.1) = 2` and
`distributable_losing = 53`.

### Instruction handlers

- `initialize_config` - anyone (the signer becomes admin). One-time setup: sets admin, stake
  token, default fee, fee recipient.
- `initialize_event` - admin. Creates a market as a `Draft`, fixes its `betting_closes_at` (which
  must be in the future), and creates its vault.
- `add_outcome` - admin. Adds a possible result. Only while the event is a `Draft`.
- `open_betting` - admin. Moves a `Draft` with at least two outcomes to `Open`, which fixes the
  outcome list.
- `place_bet` - bettor. Stakes tokens on one outcome of an `Open` event, before
  `betting_closes_at`; creates or tops up the Bet and updates the pools.
- `settle_event` - admin. Once `betting_closes_at` has passed, resolves to a winning outcome, takes
  the fee, records the payout figures.
- `claim_winnings` - winning bettor. Withdraws stake plus pro-rata share of the losing pool, then
  closes the Bet account.
- `close_losing_bet` - losing bettor. After settlement, closes a worthless Bet to reclaim its rent.
- `cancel_event` - admin. Voids a draft or unresolved market.
- `claim_refund` - bettor. After a cancellation, reclaims the exact stake; the Bet account closes.
- `close_outcome` - admin. Once the event is `Settled` or `Cancelled` (else `EventNotFinished`) and
  `open_bets` is zero (else `BetsStillOpen`), closes one Outcome account and returns its rent to
  the admin.
- `close_event` - admin. Under the same two conditions, and once every Outcome account is closed
  (else `OutcomesStillOpen`), pays whatever the vault still holds to the fee recipient's token
  account, closes the vault, and closes the Event account, returning both rents to the admin.

### Lifecycle

```
Draft --open_betting--> Open --settle_event (at or after betting_closes_at)--> Settled
  |                      |
  +------cancel_event----+----------------------------------------------> Cancelled
```

The outcome list is fixed when the admin opens betting, before any money can arrive: a bet on a
`Draft` is rejected with `EventNotOpen`, and `add_outcome` on anything but a `Draft` with
`EventNotDraft`. So no bettor can freeze a half-built market with an early bet, and no outcome can
be added under someone who has already staked.

Time splits the `Open` state in two. Bets land only while `now < betting_closes_at` (else
`BettingClosed`), and `settle_event` only once `now >= betting_closes_at` (else
`BettingStillOpen`). The two windows never overlap, so nobody can stake after the result could be
known, and the admin cannot end a market before the window bettors were promised.

`settle_event` rejects a winning outcome with no bets - use `cancel_event` to unwind an event that
can't be resolved fairly.

`Settled` and `Cancelled` are the two ways an event ends, and in both every Bet account has a
handler that pays it out or closes it: `claim_winnings` and `close_losing_bet` after a settlement,
`claim_refund` after a cancellation. Once `open_bets` reaches zero the admin closes the accounts the
event created, children first: `close_outcome` for each Outcome account, then `close_event` for the
vault and the Event account. The order matters because each account's address is derived from its
parent's, and an Event's from the admin-supplied `event_id`. An Outcome account left open after its
event closed would sit at the address a later event created with the same `event_id` needs for its
first outcome, so that event's `add_outcome` would fail. A Bet left open is a different case: no
claim, refund or losing-bet close reads the Outcome account, so keeping an Outcome open for as long
as any Bet names its address is an ordering rule rather than something those handlers need. So
`close_outcome` refuses while any bet is open, and `close_event` refuses while any outcome is. After
a settlement the vault holds only the dust the floored payouts left behind, and `close_event` pays
it to the fee recipient with the fee; after a cancellation and its refunds the vault is empty. Every
closed account's rent goes back to whoever paid it: the bettor for a Bet account, the admin for the
outcomes, the vault and the event.

## Setup

Install the [Solana CLI](https://docs.anza.xyz/cli/install) (provides `cargo-build-sbf`) and
[Anchor](https://www.anchor-lang.com/docs/installation). Build the program so the test binary
exists on disk:

```sh
anchor build
```

## Testing

Tests are Rust integration tests running against
[LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) with
[solana-kite](https://crates.io/crates/solana-kite) helpers. They cover the full lifecycle (bet →
settle → claim with exact payout and fee assertions), admin authorization of every admin handler
(`test_only_admin_can_settle_or_cancel_event` covers settling and cancelling), the bet-after-settle
guard, the double-claim guard (the first claim closes the Bet account, so Anchor refuses the second
while loading the accounts, and the test asserts that error), the outcome list locking when betting
opens, the two-outcome minimum, both
edges of the betting close time, settling an outcome with no bets, the cancel/refund path, the
`close_losing_bet` guards, and a wallet holding forty open bets at once, which shows there is no
per-wallet cap.

`test_close_event_pays_dust_to_fee_recipient_and_returns_rent` runs a settlement whose payouts do
not divide evenly, closes the losing bet, both winners' bets, both outcomes and the event, and
checks that the vault's one minor unit of dust reaches the fee recipient, that every closed account
is gone, and that each rent returns to the admin. `test_close_event_refused_while_a_bet_is_open`,
`test_close_event_refused_while_event_is_open` and `test_only_admin_can_close_outcomes_and_event`
check the three refusals, and `test_cancel_and_refund` closes a cancelled event after its refunds.

```sh
anchor test
```

(`Anchor.toml` sets `test = "cargo test"`, so `cargo test` works too.)

## FAQ

### How does a prediction market work on Solana?

This example uses the parimutuel (pooled) model: an admin sets up an event with `initialize_event` and `add_outcome` and opens it with `open_betting`, and bettors stake tokens on an outcome with `place_bet` until betting closes. Every stake goes into one pool; after `settle_event` names the winning outcome, winners call `claim_winnings` to split the losing stakes, minus a program fee, in proportion to their own stake.

### How are the odds set?

By the crowd, not a bookmaker: each winner's payout scales with their share of the winning pool, so the implied odds shift as stakes arrive. This is the model used by Solana prediction-market platforms such as Hedgehog Markets and by racetrack tote boards.

### What happens if an event is cancelled?

The admin calls `cancel_event` and every bettor reclaims their full stake with `claim_refund`. After a settled event, losers reclaim their bet account's rent with `close_losing_bet`.

### What happens to the accounts once an event is over?

Once every bet is paid out or closed, the admin calls `close_outcome` for each outcome and then `close_event`, which pays any rounding dust in the vault to the fee recipient and closes the vault and the Event account. Each rent returns to the admin, who paid it. The Config account stays open; it is the admin's state for the whole deployment.
