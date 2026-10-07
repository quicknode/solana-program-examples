# Solana Options (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, on Anchor 1.2.0, the current
> stable Anchor release. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.2.0 && avm use 1.2.0`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

A Solana options venue is a program that lets one user sell another the right,
but not the obligation, to buy or sell an asset at a fixed price before a fixed
date. This one is **fully collateralized** and **physically settled**: the
writer of a call posts the whole of the underlying, the writer of a put posts
the whole of the strike in the quote token, a buyer pays a premium for the
right, and if the holder exercises before expiry the tokens themselves change
hands at the strike. Because everything a holder could ever claim is in the
vault from the moment the option exists, no position can be under water, so
there is no margin, no liquidator, and no oracle. The venue that took the
other road on Solana, cash settlement with margin and an oracle, is Zeta
Markets.

[⚓ Anchor v2](../anchor) · [⚓ Anchor v1](.) · [💫 Quasar](../quasar) · [Kani model checks](../kani-proofs)

## Programs

- **`options`**: the venue. One admin, one underlying/quote pair, two vaults,
  one account per option, eight instruction handlers.

There is no mock oracle program, because nothing in the venue reads a price.

## Key financial concepts

### A call, a put, a strike, a premium, an expiry

An **option** is a contract with five terms. Its **kind** is a **call** (the
right to buy the underlying) or a **put** (the right to sell it). Its
**strike** is the price the trade happens at if the holder exercises. Its
**expiry** is the last moment the holder can exercise. Its **premium** is what
the buyer pays the writer for the right, up front, and keeps paying nothing
after. The person who sells the right is the **writer**; the person who holds
it is the **holder**.

The two sides have different shapes. The holder's loss is capped at the
premium, whatever happens. The writer keeps the premium whatever happens, and
in exchange takes on an obligation: to sell the underlying at the strike (a
call) or buy it at the strike (a put) if the holder asks.

### Covered and cash-secured: the collateral is the whole obligation

A writer's obligation is bounded and known at write time, so this venue simply
takes all of it into custody. A call writer posts the option's
`underlying_amount` of the underlying: the call is **covered**, and the writer
cannot fail to deliver because the shares are already in the vault. A put
writer posts the option's `strike_amount` of the quote token: the put is
**cash-secured**, and the writer cannot fail to pay. Nothing is ever
undercollateralized, which is why the program has no health check, no
liquidation, and no need to know the price.

### Physical settlement needs no oracle

When a call holder exercises, they pay the strike into the vault and take the
underlying out; a put holder delivers the underlying and takes the strike. The
tokens move; no price is computed. Whether exercising is worth it is the
holder's decision, made against whatever the market is doing offchain, and a
holder who exercises an out-of-the-money option only hurts themselves. The
program enforces the terms and nothing else. A cash-settled venue, which pays
the holder the difference between the market price and the strike, would need
a price feed and every check the *Offchain Truth* material describes.

### The option stores the amounts that change hands

An option is defined by `underlying_amount` and `strike_amount`, the two
minor-unit amounts that change hands on exercise, both chosen by the writer.
`strike_amount` is the strike for the whole option, as an amount rather than a
price. The collateral, the exercise payment and the proceeds are each one of
those two amounts, so settlement does no arithmetic at all: nothing is
multiplied, divided or rounded. The only rounding in the program is in the
venue's fee, which rounds up: the writer receives the premium minus the fee,
so the venue, not the writer, takes the rounding minor unit on a premium that
is not a multiple of the rate.

### Expiry is one comparison and its complement

The holder may exercise while `now < expiry`. The writer may reclaim the
collateral once `now >= expiry`. Those two conditions partition time, so there
is no instant at which both parties can claim the same collateral and none at
which neither can. Expiry is a unix timestamp because an option's expiry is a
calendar date the two parties agreed on, the same reason the fundraiser's
deadline is one.

## Program flow

### Participants

- **Maria** operates the venue and earns 1% of every premium.
- **Alice** holds 5 NVDAx (tokenized NVIDIA stock, 8 decimals) she would be
  happy to sell at $180, and wants to be paid while she waits.
- **Bob** thinks NVIDIA will rally past $180 within the week and wants that
  upside for less than the price of 5 shares.
- **Carol** would like to own NVDAx at $150 and wants to be paid for
  standing ready to buy.
- **Dave** holds 5 NVDAx and wants insurance against a fall below $150.

Everyone starts with the standard wallet of one SOL and 1,000 USDC, plus the
NVDAx the story hands them. NVDAx is trading around $165 offchain.

### Step 1: Maria opens the venue

`initialize_market(fee_bps = 100)` creates the `Market` account (a PDA of the
two mints) and the two vaults. The market account is the token authority of
both vaults: it owns them and signs every transfer out of them with its own
seeds. Maria's key is recorded as `admin`: it can sweep fees and do nothing
else.

### Step 2: Alice writes a covered call on 5 NVDAx

`write_option(id = 1, kind = Call, underlying_amount = 5 NVDAx, strike_amount
= 900 USDC, premium = 25 USDC, expiry = a week out)`, a strike of 180 USDC a
share, moves her 5 NVDAx into the underlying vault and creates the
`OptionContract` account (a PDA of the market, Alice, and her `id`) with
status `Listed`. Nobody has paid anything yet; Alice can `cancel_option` at
any time until someone does.

### Step 3: Bob buys the option

Bob calls `buy_option` with the five terms he read from the option (`kind`,
`underlying_amount`, `strike_amount`, `premium` and `expiry`, as an
`OptionTerms`), and the purchase is refused with `OptionTermsChanged` unless
the option still has exactly those terms. Without that check Alice could
cancel and write a new option at the same address (the same `id`) at a
higher premium, on fewer shares, or with a sooner expiry while Bob's
transaction is on its way, and Bob would pay for an option he never saw. It
is the same switched-offer defense the escrow's `take_offer` has. The
purchase itself takes 25 USDC from Bob: 0.25 USDC (the 1% fee) into the quote
vault, owed to Maria, and 24.75 USDC straight to Alice, as two transfers (a
venue with a zero fee makes only the first). 25 USDC is a multiple of the
rate, so nothing rounds; a premium of 10.000001 USDC would owe a fee of
0.100001 USDC, rounded up from 0.10000001, and pay the writer 9.90 USDC. The 5 NVDAx do not move. The
option's `holder` is now Bob and its status `Held`. Bob's downside is fixed at
the 25 USDC he just paid. Alice could not have bought her own option: the
premium's source and destination are each bound to their party's associated
token account, so with Alice on both sides they would be one account in two
mutable slots, and Anchor refuses the instruction with
`ConstraintDuplicateMutableAccount` before the handler runs.

### Step 4: NVIDIA rallies to $200 and Bob exercises

`exercise_option`, called by Bob before expiry, moves the 900 USDC strike
amount from Bob into the quote vault and 5 NVDAx from the underlying vault to
Bob. He now holds 5 NVDAx worth about $1,000, having spent 925 USDC in total. The status is
`Exercised`, and the 900 USDC sits in the vault owed to Alice.

### Step 5: Alice collects the strike

`collect_proceeds` pays Alice the 900 USDC and closes the option, rent
back to her. She sold her 5 NVDAx for 900 USDC plus the 24.75 USDC premium she
already had, and gave up everything above $180.

### Step 6: Carol writes a cash-secured put on 5 NVDAx, and Dave buys it

Carol's `write_option(id = 2, kind = Put, underlying_amount = 5 NVDAx,
strike_amount = 750 USDC, premium = 20 USDC)`, a strike of 150 USDC a share,
moves 750 USDC into the quote vault. Carol has never held NVDAx, so she has no
NVDAx account; `write_option` creates it at her expense, the account
`collect_proceeds` would pay her shares into had Dave exercised.
`cancel_option` and `reclaim_collateral` create it too if it is missing.
Dave's `buy_option` pays 19.80 USDC to Carol and 0.20 USDC to the vault for
Maria.

### Step 7: The week passes above $150, and Carol reclaims her collateral

Dave never exercises: selling at 150 when the market pays more would be a
gift. After the expiry, Carol's `reclaim_collateral` returns her 750 USDC and
closes the option. Her return is the 19.80 USDC premium; Dave's
insurance cost him 20 USDC and paid nothing, which is what insurance against a
fall that never came should do.

### Step 8: Maria sweeps the fees

`collect_fees` pays Maria the 0.45 USDC of accumulated fees, and only that:
`fees_owed` is the one part of the quote vault the admin can reach. The vaults
are empty: every token that entered has left to the party it was owed to.

Where everyone ended up: Alice earned a premium and sold her shares at her
price; Bob turned 25 USDC of premium into 5 NVDAx at a $20 discount to the
market; Carol was paid to wait for a purchase that never came; Dave bought
insurance he did not need; Maria earned 1% of every premium.

## Custody

The two vaults hold other people's money, so the `Market` account keeps a
ledger of what each vault owes: `underlying_owed` (call collateral, plus put
holders' deliveries awaiting collection), `quote_owed` (put collateral, plus
call holders' strike payments awaiting collection) and `fees_owed`. Every
handler that moves tokens updates the ledger before any transfer and then
asserts that each vault still covers what it owes (`CustodyInvariantViolated`
otherwise). The [Kani model checks](../kani-proofs) walk every path through an option's
life and check that the ledger returns to zero, and check that `collect_fees`
pays the admin exactly `fees_owed` and leaves everything owed to writers and
holders in the vault.

## Design notes and further reading

- Some venues represent each option as two SPL tokens, an option token and a
  writer token, so options can trade on any exchange and one writer's option can
  be exercised in parts by many holders. This example keeps one account per
  option, bought and exercised as a whole, which keeps the custody legible and
  the state machine three states long. Adding secondary trading means
  introducing those tokens.
- Cash-settled venues (Zeta Markets) let a writer post less than the full
  obligation, which is what makes them capital-efficient and also what makes
  them need margin, liquidation, and an oracle. The perpetual-futures example
  in this repository has all three.

## Limitations

- An option is bought and exercised as a whole; there is no partial exercise and
  no secondary sale of a held option.
- The writer sets the premium and a buyer takes it or leaves it. There is no
  order book and no pricing model; a market maker would quote premiums from
  a model offchain and write options at those prices.
- American exercise only. A European option, exercisable only at expiry,
  would add an exercise window after `expiry` and a gap before it.

## Setup

```bash
anchor build
```

## Testing

```bash
anchor build
cargo test
```

The LiteSVM suite (`programs/options/tests/test_options.rs`) walks the call
from write to collected strike and from purchase to expiry and reclaim, and
the put from write to exercise and from purchase to expiry and reclaim
(`test_reclaim_collateral_after_expiry_returns_the_strike_to_the_put_writer`),
pins every balance to the minor unit, checks that the fee on a premium that is
not a multiple of the rate rounds up and the writer receives the rest
(`test_fee_rounds_up_and_the_writer_takes_the_remainder`), follows a put
writer with no NVDAx account from write to reclaim and from write to cancel
(`test_put_writer_without_an_underlying_account_writes_and_reclaims`,
`test_put_writer_without_an_underlying_account_writes_and_cancels`), counts the token
transfers a purchase makes (two, or one on a zero-fee venue), checks the
custody ledger against the vault balances after every lifecycle step, and
checks that every refusal holds
and fails with the expected error code: the expiry boundary from both sides,
cancel after sale, buy after sale or expiry, a buy whose terms the writer
switched by cancelling and rewriting the option
(`test_buy_option_refuses_a_switched_option`, with
`test_buy_option_succeeds_when_the_terms_match` for the matching buy), a
writer buying their own option,
exercise by a non-holder, collection by a non-writer or before exercise,
reclaim after exercise, fee collection by a non-admin, a second sweep with
nothing owed, and the parameter checks at market and write time.
