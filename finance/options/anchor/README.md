# Solana Options (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

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

[⚓ Anchor v2](.) · [⚓ Anchor v1](../anchor-v1) · [💫 Quasar](../quasar) · [Kani proofs](../kani-proofs)

## Programs

- **`options`**: the venue. One admin, one underlying/quote pair, two vaults,
  one account per option lot, eight instruction handlers.

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
takes all of it into custody. A call writer posts `contracts *
underlying_per_contract` of the underlying: the call is **covered**, and the
writer cannot fail to deliver because the shares are already in the vault. A
put writer posts `contracts * strike_per_contract` of the quote token: the put
is **cash-secured**, and the writer cannot fail to pay. Nothing is ever
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

### Every amount is a product of two integers

An option is defined by `contracts`, `underlying_per_contract` and
`strike_per_contract`, all minor-unit integers the writer chooses. The
collateral, the exercise payment and the proceeds are each one checked
multiplication of two of them. There is no division anywhere in settlement, so
there is no rounding to decide a direction for; the only rounding in the
program is the floor in the venue's fee.

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
- **Alice** holds 5 NVDAx (tokenized NVIDIA stock, 6 decimals) she would be
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
two mints), a dataless vault-authority PDA, and the two vaults. Maria's key is
recorded as `admin`: it can sweep fees and do nothing else.

### Step 2: Alice writes 5 covered calls

`write_option(id = 1, kind = Call, contracts = 5, underlying_per_contract =
1 NVDAx, strike_per_contract = 180 USDC, premium = 25 USDC, expiry = a week
out)` moves her 5 NVDAx into the underlying vault and creates the
`OptionContract` account (a PDA of the market, Alice, and her `id`) with
status `Listed`. Nobody has paid anything yet; Alice can `cancel_option` at
any time until someone does.

### Step 3: Bob buys the lot

`buy_option` takes 25 USDC from Bob: 0.25 USDC (the 1% fee) into the quote
vault, owed to Maria, and 24.75 USDC straight to Alice. The 5 NVDAx do not
move. The option's `holder` is now Bob and its status `Held`. Bob's downside
is fixed at the 25 USDC he just paid.

### Step 4: NVIDIA rallies to $200 and Bob exercises

`exercise_option`, called by Bob before expiry, moves 5 × 180 = 900 USDC from
Bob into the quote vault and 5 NVDAx from the underlying vault to Bob. He now
holds 5 NVDAx worth about $1,000, having spent 925 USDC in total. The status is
`Exercised`, and the 900 USDC sits in the vault owed to Alice.

### Step 5: Alice collects the strike

`collect_proceeds` pays Alice the 900 USDC and closes the option account, rent
back to her. She sold her 5 NVDAx for 900 USDC plus the 24.75 USDC premium she
already had, and gave up everything above $180.

### Step 6: Carol writes 5 cash-secured puts, and Dave buys them

Carol's `write_option(id = 2, kind = Put, contracts = 5, underlying_per_contract
= 1 NVDAx, strike_per_contract = 150 USDC, premium = 20 USDC)` moves 5 × 150 =
750 USDC into the quote vault. Dave's `buy_option` pays 19.80 USDC to Carol
and 0.20 USDC to the vault for Maria.

### Step 7: The week passes above $150, and Carol reclaims her collateral

Dave never exercises: selling at 150 when the market pays more would be a
gift. After the expiry, Carol's `reclaim_collateral` returns her 750 USDC and
closes the option account. Her return is the 19.80 USDC premium; Dave's
insurance cost him 20 USDC and paid nothing, which is what insurance against a
fall that never came should do.

### Step 8: Maria sweeps the fees

`collect_fees` pays Maria the 0.45 USDC of accumulated fees. The vaults are
empty: every token that entered has left to the party it was owed to.

Where everyone ended up: Alice earned a premium and sold her shares at her
price; Bob turned 25 USDC of premium into 5 NVDAx at a $20 discount to the
market; Carol was paid to wait for a purchase that never came; Dave bought
insurance he did not need; Maria earned 1% of every premium.

## Custody

The two vaults hold other people's money, so the `Market` account keeps a
ledger of what each vault owes: `underlying_locked` (call collateral, plus put
holders' deliveries awaiting collection), `quote_locked` (put collateral, plus
call holders' strike payments awaiting collection) and `fees_owed`. Every
handler that moves tokens updates the ledger before any transfer and then
asserts that each vault still covers what it owes (`CustodyInvariantViolated`
otherwise). The [Kani proofs](../kani-proofs) walk every path through a lot's
life and show the ledger returns to zero.

## Design notes and further reading

- Some venues represent each option as two SPL tokens, an option token and a
  writer token, so options can trade on any exchange and one writer's lot can
  be exercised in parts by many holders. This example keeps one account per
  lot, bought and exercised as a whole, which keeps the custody legible and
  the state machine three states long. Adding secondary trading means
  introducing those tokens.
- Cash-settled venues (Zeta Markets) let a writer post less than the full
  obligation, which is what makes them capital-efficient and also what makes
  them need margin, liquidation, and an oracle. The perpetual-futures example
  in this repository has all three.

## Limitations

- A lot is bought and exercised as a whole; there is no partial exercise and
  no secondary sale of a held option.
- The writer sets the premium and a buyer takes it or leaves it. There is no
  order book and no pricing model; a market maker would quote premiums from
  a model offchain and write lots at those prices.
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
from write to collected strike and the put from write to exercise and to
expiry, pins every balance to the minor unit, checks the custody ledger
against the vault balances after every step, and proves every gate shuts:
the expiry boundary from both sides, cancel after sale, buy after sale or
expiry, exercise by a non-holder, collection by a non-writer or before
exercise, reclaim after exercise, fee collection by a non-admin, and the
parameter checks at write time.

## FAQ

### How do options work on Solana?

A writer calls `write_option`, posting the full collateral (the underlying for
a call, the strike in USDC for a put) and naming a premium. A buyer calls
`buy_option`, pays the premium, and becomes the holder. Before expiry the
holder may call `exercise_option` to trade at the strike; after expiry the
writer calls `reclaim_collateral` to take back what was not exercised.

### Why does this options program need no oracle?

Because it settles physically. Exercising moves the underlying one way and
the strike the other; the program never has to know what the underlying is
worth, only that the holder chose to trade. A cash-settled option pays the
difference between the market price and the strike, and that difference is a
number only a price feed can supply.

### What stops a writer from defaulting?

They cannot: `write_option` takes the entire obligation into the vault up
front, so a call is covered and a put is cash-secured. The writer can get the
collateral back only through `cancel_option` (before anyone buys) or
`reclaim_collateral` (after expiry), and never while a holder could still
exercise.

### How does the venue make money?

`buy_option` takes `fee_bps` of every premium into the quote vault, and the
admin sweeps it with `collect_fees`. The fee is the admin's only reach into
the vault; collateral and strike payments are locked to their writers and
holders.
