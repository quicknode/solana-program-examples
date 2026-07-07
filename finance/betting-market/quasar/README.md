# Betting Market, Quasar port

A parimutuel betting market. An admin opens events (markets), adds the possible
outcomes, and later settles or cancels each one. Bettors stake a fixed token on
the outcome they think will happen; when the event is settled, the winners split
the losing side's stakes in proportion to their own, after a protocol fee. This
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

- The **admin** (whoever ran `initialize_config`) opens an event with
  `create_event`, then lists each possible result with `add_outcome`. Outcomes
  can only be added before the first bet, so the field of choices can't change
  under bettors who have already staked.
- A **bettor** stakes the market's token on one outcome with `place_bet`. The
  stake joins the event's single pool vault. Re-betting the same outcome tops up
  the existing position rather than opening a second one.
- The admin resolves the market with `settle_event`, naming the winning outcome.
  The protocol fee is charged only on the losing pool, so a winner can never
  receive less than they staked. The fee moves to the fee recipient immediately;
  the figures winners need are recorded on the event.
- A winner calls `claim_winnings` to withdraw their stake plus their share of
  the losing pool (their stake divided by the total winning stake, times the
  distributable losing pool). A loser calls `close_losing_bet` to reclaim their
  Bet account's rent and free a slot in their position index.
- If a market cannot be resolved, the admin calls `cancel_event`, and every
  bettor reclaims their exact stake with `claim_refund`. No fee is taken.

Closing the Bet account is what ends a position and prevents a double claim: a
second `claim_winnings` or `claim_refund` fails because the account no longer
exists.

## Accounts and PDAs

- **Config**, PDA `["config"]`. The single global account. Its `admin` is the
  only key allowed to create, settle, and cancel events; `token_mint` fixes the
  one stake asset; `fee_recipient` and `fee_bps` set the settlement fee.
- **Event**, PDA `["event", event_id]`. One market. Holds the running
  `total_pool`, the status (Open, Settled, Cancelled), a fee snapshot taken at
  creation, and the winning figures written at settlement. Its PDA is the token
  authority of the pool vault.
- **Outcome**, PDA `["outcome", event, index]`. One possible result.
  `total_amount` is this outcome's share of the pool and the denominator for
  pro-rata payouts when it wins.
- **Bet**, PDA `["bet", outcome, bettor]`. One bettor's total stake on one
  outcome. Exactly one per (outcome, bettor); it closes on claim, refund, or
  loser-close.
- **User**, PDA `["user", bettor]`. A per-wallet index of a bettor's open Bet
  accounts, so a client can list a wallet's positions without scanning every Bet
  on the program. Capped at 32 concurrent positions.
- **Pool vault**, PDA `["vault", event]`. One token account per event, holding
  every stake across all outcomes, with the Event PDA as its authority.

## Safety and custody

- Stakes sit in the program-owned pool vault from `place_bet` until a claim or
  refund. Every transfer out is signed by the Event PDA with `invoke_signed`, so
  only the deployed program can move pooled funds. There is no admin path to
  withdraw stakes, only to settle or cancel.
- Payouts credit and close before transferring (effects before interactions),
  and the fee uses integer division that floors in the pool's favor, leaving at
  most a few minor units of dust rather than ever overpaying.
- Admin-gated instructions bind the signer to `config.admin` with `has_one`, and
  the winning outcome is tied to its index through the account's PDA derivation,
  so a mismatched outcome can't be settled to.

## What the Quasar port does differently

The mechanics, fee model, and payout math are identical to the Anchor build. The
differences follow from Quasar being zero-copy and fixed-layout:

- **Variable-length text and the position index are fixed-capacity.** The Anchor
  build stores `Event.description` and `Outcome.label` as borsh `String`s and
  `User.bets` as a `Vec<Pubkey>`. This port stores them as fixed byte buffers
  plus a length (`[u8; 200]`, `[u8; 64]`, and a packed `[u8; 1024]` of up to 32
  addresses). Keeping every account fixed-size makes each mutation a plain
  in-place write, with no reallocation and no read-your-own-buffer aliasing when
  an account is updated after creation.
- **The pool vault is a program-derived token account** (`["vault", event]`)
  rather than an associated token account, matching how the other Quasar finance
  examples (lending, perpetual-futures) hold pool funds.
- **Enums are stored as `u8`** (zero-copy accounts hold POD scalars). `side` and
  status values match the Anchor build's byte encodings.

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
suite in `src/tests.rs` drives the full lifecycle (open a market, add outcomes,
place opposing bets, settle, claim the winnings, close the losing bet) and the
cancel-and-refund path, asserting on-chain state, token balances, and fee
accounting at each step, plus an admin-authorization rejection.

## Extending

- Per-market stake tokens instead of one deployment-wide mint.
- A minimum settlement delay so an event can't be settled the instant it opens.
- Partial cash-out of a position before settlement.
- Oracle-driven settlement instead of an admin call.
