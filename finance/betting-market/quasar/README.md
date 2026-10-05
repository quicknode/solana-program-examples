# Solana Betting Market (Quasar)

A parimutuel betting market on Solana, written with Quasar. An admin creates events (markets), adds the possible
outcomes, opens them to bets, and later settles or cancels each one. Bettors stake a fixed token on
the outcome they think will happen; when the event is settled, the winners split
the losing side's stakes in proportion to their own, after a program fee. This
is the same mechanism a racetrack tote board or a prediction market runs on.

This is a [Quasar](https://github.com/blueshift-gg/quasar) port of the Anchor
example in [`../anchor`](../anchor). Quasar is a zero-copy, `no_std`,
zero-allocation Solana framework with Anchor-like syntax. Both builds use the
same program ID (`7LyqAeLR3mK9dfj9LqxWzfKH61VVHzuNpkgW5Y32De74`), so clients and
PDA derivations work against either unchanged.

## How a market plays out

A market pays out parimutuel-style: there is no fixed odds and no house taking
the other side of your bet. Everyone's stake goes into one pool, and when the
result is known the winners divide the pool.

- The **admin** (whoever ran `initialize_config`) creates an event as a draft
  with `initialize_event`, fixing when betting closes, then lists each possible
  result with `add_outcome`, and opens it with `open_betting`, which needs at
  least two outcomes. Outcomes can only be added to a draft and nobody can bet on
  one, so the field of choices is final before the first bet lands.
- A **bettor** stakes the market's token on one outcome with `place_bet`, any
  time before the close. The stake joins the event's single pool vault.
  Re-betting the same outcome tops up the existing position rather than opening a
  second one.
- Once betting has closed, the admin resolves the market with `settle_event`,
  naming the winning outcome. Bets are accepted only while `now <
  betting_closes_at` and settlement only once `now >= betting_closes_at`, so
  nobody can stake after the result could be known.
  The program fee is charged only on the losing pool, so a winner can never
  receive less than they staked, and it rounds up:
  `ceil(losing_pool * fee_bps / 10_000)`. The fee moves to the fee recipient
  immediately; the figures winners need are recorded on the event.
- A winner calls `claim_winnings` to withdraw their stake plus their share of
  the losing pool (their stake divided by the total winning stake, times the
  distributable losing pool). A loser calls `close_losing_bet` to reclaim their
  Bet account's rent.
- If a market cannot be resolved, the admin calls `cancel_event` (on a draft or
  an open event), and every
  bettor reclaims their exact stake with `claim_refund`. No fee is taken.
- Once every bet is paid out or closed, the admin closes the accounts the event
  created: `close_outcome` for each Outcome account (once the event is `Settled`
  or `Cancelled`, else `EventNotFinished`, and `open_bets` is zero, else
  `BetsStillOpen`), then `close_event` (under the same two conditions, and once
  every Outcome account is closed, else `OutcomesStillOpen`), which pays
  whatever the vault still holds to the fee recipient's token account, closes
  the vault, and closes the Event account, returning both rents to the admin.

Closing the Bet account is what ends a position and prevents a double claim: a
second `claim_winnings` or `claim_refund` fails because the account no longer
exists.

`Settled` and `Cancelled` are the two ways an event ends, and in both every Bet
account has a handler that pays it out or closes it: `claim_winnings` and
`close_losing_bet` after a settlement, `claim_refund` after a cancellation. Once
`open_bets` reaches zero the admin closes the accounts the event created,
children first: `close_outcome` for each Outcome account, then `close_event` for
the vault and the Event account. The order matters because each account's
address is derived from its parent's. An Outcome account left open after its
event closed would be found again, with its old `total_amount`, by a later event
created with the same `event_id`, and a Bet account left open after its outcome
closed would have nothing to claim against. So `close_outcome` refuses while any
bet is open, and `close_event` refuses while any outcome is. After a settlement
the vault holds only the dust the floored payouts left behind, and `close_event`
pays it to the fee recipient with the fee; after a cancellation and its refunds
the vault is empty. Every closed account's rent goes back to whoever paid it:
the bettor for a Bet account, the admin for the outcomes, the vault and the
event. The Config account stays open; it is the admin's state for the whole
deployment.

## Accounts and PDAs

- **Config**, PDA `["config"]`. The single global account. Its `admin` is the
  only key allowed to create, open, settle, cancel, and close events;
  `token_mint` fixes the one stake asset; `fee_recipient` receives the settlement
  fee, and `default_fee_bps` is the fee each new event copies at creation. The
  config is the admin's long-lived state and stays open for the life of the
  deployment; no handler closes it.
- **Event**, PDA `["event", event_id]`. One market. Holds the running
  `total_pool`, the status (Draft, Open, Settled, Cancelled), the
  `betting_closes_at` timestamp and a fee snapshot both fixed at creation, and the
  winning figures written at settlement. Its PDA is the token
  authority of the pool vault. Two counters say when the event can close:
  `open_bets`, the number of Bet accounts across every outcome that are still
  open (`place_bet` adds one when it creates a Bet account; a top-up reuses the
  account and adds nothing; `claim_winnings`, `claim_refund` and
  `close_losing_bet` each subtract one), and `open_outcomes`, the number of
  Outcome accounts still open (`add_outcome` adds one, `close_outcome` subtracts
  one). `close_event` closes the Event account and the vault, returning their
  rent to the admin.
- **Outcome**, PDA `["outcome", event, index]`. One possible result.
  `total_amount` is this outcome's share of the pool and the denominator for
  pro-rata payouts when it wins; its `bet_count` is the number of Bet accounts
  ever created on it. `close_outcome` closes it once the event is finished and
  every bet is closed, returning its rent to the admin.
- **Bet**, PDA `["bet", outcome, bettor]`. One bettor's total stake on one
  outcome. Exactly one per (outcome, bettor); it closes on claim, refund, or
  loser-close. `bettor` is the first field after the 1-byte discriminator, so a
  client lists a wallet's open positions with `getProgramAccounts` and a
  `memcmp` filter at offset 1; there is no per-wallet index and no cap.
- **Pool vault**, PDA `["vault", event]`. One token account per event, holding
  every stake across all outcomes, with the Event PDA as its authority.
  `settle_event`, `claim_winnings`, `claim_refund` and `close_event` move tokens
  out of it, signed by the Event PDA, and `close_event` then closes it with the
  same signature, returning its rent to the admin.

## Safety and custody

- Stakes sit in the program-owned pool vault from `place_bet` until a claim or
  refund. Every transfer out is signed by the Event PDA with `invoke_signed`, so
  only the deployed program can move pooled funds. There is no admin path to
  withdraw stakes, only to settle or cancel.
- Payouts credit and close before transferring (effects before interactions),
  and every rounding goes the pool's way: the fee is the ceiling of its fraction
  of the losing pool and each winner's share is floored, leaving at most a few
  minor units of dust rather than ever overpaying. `close_event` pays that dust
  to the fee recipient once every bet is closed.
- Admin-gated instructions bind the signer to `config.admin` with `has_one`, and
  the winning outcome is tied to its index through the account's PDA derivation,
  so a mismatched outcome can't be settled to.

## What the Quasar port does differently

The mechanics, fee model, and payout math are identical to the Anchor build. The
differences follow from Quasar being zero-copy and fixed-layout:

- **Variable-length text is fixed-capacity.** The Anchor build stores
  `Event.description` and `Outcome.label` as borsh `String`s. This port stores
  them as fixed byte buffers plus a length (`[u8; 200]` and `[u8; 64]`). Keeping every account fixed-size makes each mutation a plain
  in-place write, with no reallocation and no read-your-own-buffer aliasing when
  an account is updated after creation.
- **The pool vault is a program-derived token account** (`["vault", event]`)
  rather than an associated token account, matching how the other Quasar finance
  examples (lending, perpetual-futures) hold pool funds.
- **Enums are stored as `u8`** (zero-copy accounts hold POD scalars). The
  `EventStatus` values match the Anchor build's byte encodings.

## Building and testing

Requires the [Solana toolchain](https://docs.anza.xyz/cli/install) and the
[Quasar CLI](https://github.com/blueshift-gg/quasar):

```sh
cargo install --git https://github.com/blueshift-gg/quasar quasar-cli --locked
quasar build          # compiles the program to target/deploy/quasar_betting_market.so
cargo test            # QuasarSVM integration tests (they load the compiled .so)
```

`quasar build` must run before `cargo test`, which loads the compiled `.so` into
[QuasarSVM](https://github.com/blueshift-gg/quasar-svm), an in-process SVM. The
suite in `src/tests.rs` drives the full lifecycle (create a market, add outcomes,
open betting, place opposing bets, settle after the close, claim the winnings,
close the losing bet) and the cancel-and-refund path, asserting onchain state,
token balances, and fee accounting at each step. It also checks the admin
authorization of every admin handler
(`settle_and_cancel_reject_a_non_admin_signer` covers settling and cancelling),
that the outcome list locks when betting opens, the two-outcome minimum, both
edges of the betting close time, and that a close time in the past is refused.

`close_event_pays_dust_to_fee_recipient_and_returns_rent` runs a settlement
whose fee (1% of 250, rounded up to 3) and payouts do not divide evenly, closes
the losing bet, both winners' bets,
both outcomes and the event, and checks that the vault's one minor unit of dust
reaches the fee recipient, that every closed account is gone, and that each rent
returns to the admin. `close_event_refused_while_a_bet_is_open`,
`close_event_refused_while_event_is_open` and
`only_admin_can_close_outcomes_and_event` check the three refusals, and
`cancelled_event_refunds_the_exact_stake` closes a cancelled event after its
refunds.

## Extending

- Per-market stake tokens instead of one deployment-wide mint.
- A minimum settlement delay so an event can't be settled the instant it opens.
- Partial cash-out of a position before settlement.
- Oracle-driven settlement instead of an admin call.
