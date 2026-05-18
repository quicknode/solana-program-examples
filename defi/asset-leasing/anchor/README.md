# Asset Leasing

**Directional token lending.** **Holders** rent out token inventory
to **short sellers**. The short seller posts collateral in a stable
asset (e.g. USDC) and borrows the asset they want to short (e.g.
xNVDA). They immediately sell the borrowed xNVDA on the open market
for more USDC, pay a second-by-second lending fee while the position
is open, and later buy equivalent xNVDA back to return to the
holder. If xNVDA's price falls between the sell and the re-buy, the
short seller pockets the difference in USDC; if xNVDA rallies far
enough that their collateral no longer covers the cost of buying it
back, keepers liquidate the position.

This is the same primitive that underpins traditional securities
lending in TradFi: holders earn yield on inventory they would hold
anyway (think exchange-traded funds, pension funds, or any passive
allocator), and short sellers and arbitrageurs get the tokens they
need to sell short. The program is written in
[Anchor](https://solana.com/docs/terminology); a parallel
[Quasar port](#quasar-port) implements the same onchain behaviour.

---

## Table of contents

1. [What does this program do?](#what-does-this-program-do)
2. [Bilateral versus pooled lending](#bilateral-versus-pooled-lending)
3. [Lifecycle](#lifecycle)
4. [Safety and edge cases](#safety-and-edge-cases)
5. [Running the tests](#running-the-tests)
6. [Quasar port](#quasar-port)
7. [Extending the program](#extending-the-program)

---

## What does this program do?

A **holder** offers some quantity of **token A** - the leased token -
for a fixed term. A **short seller** posts collateral in a different
**token B** - the collateral token - to take delivery of the A
tokens.

The short seller's full lifecycle is:

1. **Open the position** by calling `take_lease`. This borrows A from
   the holder and locks B as collateral. From this point on, a
   per-second lending fee accrues against the locked collateral. The
   fee is computed on demand: the program tracks
   `last_paid_timestamp` and `lease_fee_per_second` on the lease
   account, multiplies by elapsed seconds whenever any handler runs,
   and debits the result from the collateral. Nothing happens onchain
   each second - the fee is just a number that grows until someone
   pokes the lease.
2. **Sell A immediately** on a market like Jupiter, receiving more B
   in return. The short seller now has more B and owes A. The
   asset-leasing program does not perform this swap itself; that is
   the DEX's job, and keeping the two concerns separate keeps each
   program narrow and composable. In practice a frontend bundles
   `take_lease` and the Jupiter swap into a single transaction so
   the short seller signs once and the open-short flow is atomic
   (Solana's transaction atomicity guarantees both succeed or both
   revert).
3. **Wait.** They are betting A's price (denominated in B) will fall.
   The short seller doesn't have to call anything while they wait -
   accrued fees auto-settle at close. They can optionally call
   `pay_lease_fee` to settle the running balance early (so the fee
   doesn't eat into their collateral cushion), and `top_up_collateral`
   to add more collateral if A's price moves against them.
4. **Close the position** by calling `return_lease`. They buy A back
   on the open market - hopefully at a lower price than they sold it
   for - and return the same quantity of A to the holder. The B they
   paid to re-acquire A is less than the B they got for selling it,
   and the difference is the short seller's profit.

If A's price *rises* instead, buying it back costs more B than they
got for selling it - that's a loss. If it rises far enough that their
locked collateral is no longer worth more than the A they owe, anyone
can call `liquidate` to close the position out, paying the keeper a
bounty from the collateral. If the lease term ends without the short
seller calling `return_lease`, the holder calls `close_expired` to
seize the collateral and recover.

The holder's full lifecycle is shorter:

1. **List the tokens** by calling `create_lease`. This locks the A
   tokens in a program-owned vault and publishes the terms (collateral
   required, lease fee, duration, maintenance margin, liquidation
   bounty, oracle feed). The lease starts in `Listed` status.
2. **Wait for a taker.** If a short seller takes the offer (calling
   `take_lease`), the lease moves to `Active` status and the holder
   starts earning the per-second lending fee. If no-one takes it, the
   holder can cancel at any time.
3. **Earn fees while the lease is `Active`.** The holder doesn't have
   to call anything; the fee accrues against the short seller's
   collateral and settles whenever any handler runs against the lease.
4. **Get paid out at close.** Whichever path the lease takes (clean
   return, liquidation, or expiry), the holder ends up with their A
   tokens back (or, on liquidation/expiry default, the equivalent
   value in B as compensation) plus all the lease fees that accrued.

The holder can call `close_expired` to terminate the lease in two
situations: (a) the lease is `Listed` and they want to cancel it
before any short seller takes it, or (b) the lease is `Active`, the
deadline has passed, and the short seller hasn't returned the tokens -
in which case the holder seizes the entire collateral as compensation
for the missing tokens.

The program acts as the escrow agent. Both the leased tokens and
the collateral sit in program-owned vaults during the lease, and the
program-derived address signs all the transfers in and out. There is
no admin key and no off-program logic that can move funds: every
transfer is dictated by the rules below, and those rules are the
deployed bytecode. Specifically:

1. Takes the holder's A tokens and locks them in a program-owned
   vault until a short seller shows up.
2. When a short seller calls `take_lease`, the program locks the
   short seller's B tokens as collateral and hands the A tokens to
   the short seller.
3. While the loan is live, a second-by-second **lending fee stream**
   pays the holder out of the collateral vault.
4. If the price of A (measured in B) rises far enough that the locked
   collateral is no longer enough to cover the cost of re-acquiring
   the borrowed tokens, anyone can call `liquidate` - the collateral
   is seized, most of it goes to the holder, and a small percentage
   (the **liquidation bounty**) goes to whoever called `liquidate`.
   Such a caller is known as a **keeper** - a bot or anyone else who
   watches the chain for positions that have gone underwater and
   earns the bounty by cleaning them up.
5. If the short seller returns the full A amount before the deadline,
   the short seller gets back whatever collateral is left after
   lending fees.
6. If the short seller ghosts past the deadline without returning
   anything, the holder calls `close_expired` and sweeps the
   collateral as compensation.

The trigger for step 4 is the **maintenance margin**: a ratio,
expressed in basis points (1 basis point = 1/100 of a percent), of
required collateral value to debt value.
`maintenance_margin_basis_points = 12_000` is 120%, meaning the
collateral must stay worth at least 1.2× the borrowed tokens. Drop
below and the position becomes liquidatable.

The program is a pair of vaults, a small piece of state that tracks
how much has been paid, and an oracle check.

### Example: shorting xNVDA via the lending market

Concrete numbers using assets that already trade on Solana -
[xNVDA](https://www.backed.fi/) (a Backed Finance / xStocks tokenised
NVIDIA share) and USDC. xNVDA has its own Pyth feed; the program
takes the feed id verbatim at `create_lease`.

Alice holds 100 xNVDA at ~$180 / share, ~$18 000 notional. She wants
yield on inventory she would hold anyway.

Bob wants short exposure to NVIDIA without using a perpetual future.

Alice lists the lease (assume USDC is 6-decimal, xNVDA is also
6-decimal for round numbers):

- **`leased_amount`**: `100_000_000` (100 xNVDA)
- **`required_collateral_amount`**: `22_000_000_000` (22 000 USDC) - ~122% LTV at the spot price
- **`lease_fee_per_second`**: `456` (USDC base units / s) - ≈ 8% APR on 18 000 USDC notional
- **`duration_seconds`**: `2_592_000` - 30 days
- **`maintenance_margin_basis_points`**: `11_000` - 110%
- **`liquidation_bounty_basis_points`**: `100` - 1% of post-fee collateral
- **`feed_id`**: Pyth xNVDA/USD feed id ([Pyth feed registry](https://www.pyth.network/price-feeds))

Bob calls `take_lease`, posts 22 000 USDC, receives 100 xNVDA, and
sells the 100 xNVDA on Jupiter for ~18 000 USDC at the spot price.

#### If NVIDIA rallies to $200

- Bob's debt to repurchase the 100 xNVDA is now `100 × $200 = $20 000`.
- Collateral ratio: `22 000 / 20 000 = 110%` - exactly at the
  maintenance margin.
- One more upward tick and a keeper can call `liquidate` with a fresh
  Pyth update. Of the 22 000 USDC vault: a small portion has
  already streamed out as lease fees (Bob's incentive to keep paying
  was to keep the position alive); of what's left, 1% goes to the
  keeper as the bounty (~220 USDC), the rest to Alice.
- Bob can avoid liquidation by:
  - Calling `top_up_collateral` to push the ratio back above 110%, or
  - Buying 100 xNVDA on the open market and calling `return_lease` to
    close out cleanly.

#### If NVIDIA falls to $160

- Bob's debt drops to `100 × $160 = $16 000`.
- Collateral ratio: `22 000 / 16 000 = 137.5%` - well above the 110%
  maintenance margin. No liquidation pressure.
- Bob buys back 100 xNVDA on Jupiter for ~16 000 USDC and calls
  `return_lease`. Alice receives the 100 xNVDA back plus the
  accrued lease fee. The remaining ~22 000 USDC (minus fees paid)
  refunds to Bob.
- Bob's profit ≈ `$18 000 − $16 000 − fees − trading costs ≈ $2 000`
  minus carry. This is a 30-day short on NVIDIA, expressed onchain.

The asymmetry: liquidation only ever fires when the *borrowed* asset
rallies against the collateral. A drop in the borrowed asset price is
purely beneficial to the short seller. The streaming lending fee is
the position's only ongoing cost in either direction.

The [lifecycle](#lifecycle) section walks each instruction handler
with concrete numbers that match the LiteSVM tests; the xNVDA example
above is the same machinery applied to a real asset pair.

### Production deviations to know

- **Pyth integration is hand-rolled, not via the SDK.** The LiteSVM
  tests install a `PriceUpdateV2` account whose layout is decoded
  inline in `liquidate.rs`. Production code would depend on the
  `pyth-solana-receiver-sdk` crate so layout changes are caught at
  compile time.
- See [safety and edge cases](#safety-and-edge-cases) for the rest of the deliberate simplifications.

---

## Bilateral versus pooled lending

Our program could be redesigned to use pooled lending, like Kamino,
MarginFi, and other programs where many depositors share one
liquidity pool that borrowers draw from against collateral, with
rates set automatically by a utilisation curve. That design works
well for some assets and poorly for others. We chose bilateral
lending - direct deals between one holder and one short seller - and
it's worth explaining why.

Pooled lending already supports shorting tokens. A short seller
deposits collateral, borrows the asset, sells it, buys it back later,
and repays the loan. So the question isn't whether pooled lending
*can* facilitate shorts. It can. The question is which structure is
the right tool for which market.

For deep, liquid assets such as SOL, USDC, and the majors, pooled
lending is the right tool. Capital is efficient, fills are instant,
and pricing adjusts automatically as borrowing demand changes.

Bilateral lending wins where pooled lending breaks down:

- **Bilateral terms.** Holder and short seller agree on a fixed
  duration, fixed lease fee, and a custom collateral schedule.
  Pooled lending forces every borrower onto one rate model and
  offers no end date. Borrowers face open-ended exposure to rate
  spikes and margin calls. Bilateral lending gives both sides
  predictability.
- **Pool rates spike when supply is thin.** Pool interest rates rise
  gently as borrowing demand grows, then spike sharply once most of
  the supply is in use. For lightly supplied assets, this makes
  shorting punitive and unstable. Bilateral lending prices through
  direct negotiation, so the rate is whatever holder and short
  seller agree on.
- **Holder control over supply.** In a pool, the holder is one of
  many depositors; the program commingles deposits and decides how
  they get used. In bilateral lending, the holder chooses which
  short seller borrows their tokens and on what terms. They can
  refuse, charge a premium, or restrict to specific counterparties.
- **Long-tail and new tokens.** A token with no pooled-lending
  market cannot be shorted through pooled lending. Bilateral lending
  works on day one with one holder and one short seller, in markets
  of size n=1.

If your target asset is a major liquid token with deep existing
pooled-lending markets, redesigning around a pool is reasonable. If
your target is anything else - a thinly-supplied token, a token
where holders care who borrows from them, or a token where both
sides want fixed terms - bilateral lending is the better fit.

---

## Lifecycle

### What the short seller really gets

When a short seller takes a lease, they walk away with two things:

- **At open: today's market value of the leased tokens, in stables.**
  They borrow the leased tokens from a holder, sell them on the open
  market immediately, and pocket the stables.
- **At close: an obligation to return the same number of tokens,
  regardless of what those tokens are worth then.** The obligation
  is fixed in *units of the leased token*, not in *units of value*.
  If the price falls - say from $190 to $160 per token - the cost of
  acquiring the same number of tokens to return drops, and the short
  seller keeps the difference.

The asymmetry is the trade: cash received today is fixed in stables;
the cost of fulfilling the obligation later is fixed in tokens whose
price is unknown. Bet correctly on the direction and that asymmetry
prints money. Bet wrong and the cost of buying the tokens back can
exceed the cash plus the collateral, at which point the keepers
arrive (see [branch: position underwater - `liquidate`](#branch-position-underwater---liquidate)).

### The holder lists the tokens - `create_lease`

The holder calls `create_lease`, naming the leased mint, the
collateral mint, the amount of leased tokens to offer, the
collateral the short seller will have to post, the per-second lease
fee, the duration, the maintenance-margin and liquidation-bounty
ratios, and the Pyth `feed_id` the lease will be priced against.
This is where every account the rest of the lifecycle uses gets
created. The handler initialises three
[program-derived addresses](https://solana.com/docs/terminology):

- **`Lease`** - the state account, owned by the program, holding all
  the lease parameters and the current lifecycle status. Seeds:
  `[b"lease", holder, lease_id.to_le_bytes()]` - keying on
  `lease_id` lets one holder run many leases in parallel.
- **`leased_vault`** - a token account for the leased mint whose
  authority is itself (the program signs as the vault using the
  vault's own seeds). Seeds: `[b"leased_vault", lease]`. Holds
  `leased_amount` while `Listed`; `0` while `Active` (the short
  seller has the tokens); the full amount again briefly inside
  `return_lease`.
- **`collateral_vault`** - a token account for the collateral mint,
  also self-authoritative. Seeds: `[b"collateral_vault", lease]`.
  Created empty here; filled by `take_lease`, drained over time as
  lease fees stream out, and topped up by `top_up_collateral`.

The handler then moves the leased tokens out of the holder's wallet
into the leased vault. Locking the leased tokens up front means a
short seller calling `take_lease` later cannot fail because the
holder spent the inventory in the meantime - the atomicity guarantee
transfers to the program the moment the lease is listed.

- **Signers:** `holder` (the user wallet listing the tokens; receives
  the lease fee and the final recovery).
- **Accounts:**
  - `holder` (signer, mut - pays account rent)
  - `leased_mint`, `collateral_mint` (read-only)
  - `holder_leased_account` (mut, holder's [associated token account](https://solana.com/docs/terminology) for the leased mint - source)
  - `lease` (program-derived address, **init**) - created here, seeds `[b"lease", holder, lease_id.to_le_bytes()]`
  - `leased_vault` (program-derived address, **init**, token account) - created here, seeds `[b"leased_vault", lease]`, authority = itself
  - `collateral_vault` (program-derived address, **init**, token account) - created here, seeds `[b"collateral_vault", lease]`, authority = itself
  - `token_program`, `system_program`
- **What happens:**
  - Single token movement: `leased_amount` of the leased mint
    transfers from `holder_leased_account` to `leased_vault`.
  - The `Lease` account is written with `status = Listed`,
    `short_seller = Pubkey::default()`, `collateral_amount = 0`,
    `start_timestamp = 0`, `end_timestamp = 0`,
    `last_paid_timestamp = 0`, and the supplied parameters including
    `feed_id`. All three bumps are stored.
- **Errors:**
  - `LeasedMintEqualsCollateralMint` if `leased_mint == collateral_mint`
  - `InvalidLeasedAmount` if `leased_amount == 0`
  - `InvalidCollateralAmount` if `required_collateral_amount == 0`
  - `InvalidLeaseFeePerSecond` if `lease_fee_per_second == 0`
  - `InvalidDuration` if `duration_seconds <= 0`
  - `InvalidMaintenanceMargin` if `maintenance_margin_basis_points` is `0` or `> 50_000`
  - `InvalidLiquidationBounty` if `liquidation_bounty_basis_points > 2_000`

#### What's on the lease account

The `Lease` account written above carries the full set of fields
referenced by the rest of the lifecycle. From [`state/lease.rs`](programs/asset-leasing/src/state/lease.rs):

```rust
pub struct Lease {
    pub lease_id: u64,             // caller-supplied id so one holder can run many leases
    pub holder: Pubkey,            // who listed it, gets paid the lease fee
    pub short_seller: Pubkey,      // who took the lease; Pubkey::default() while Listed

    pub leased_mint: Pubkey,
    pub leased_amount: u64,        // locked at creation, unchanging

    pub collateral_mint: Pubkey,
    pub collateral_amount: u64,    // increases on top_up, decreases as lease fees pay out
    pub required_collateral_amount: u64, // what the short seller must post on take_lease

    pub lease_fee_per_second: u64,      // denominated in collateral units
    pub duration_seconds: i64,
    pub start_timestamp: i64,             // 0 while Listed
    pub end_timestamp: i64,               // 0 while Listed; start_timestamp + duration once Active
    pub last_paid_timestamp: i64,    // Lease fee accrues from here to min(now, end_timestamp)

    pub maintenance_margin_basis_points: u16,   // e.g. 12_000 = 120%
    pub liquidation_bounty_basis_points: u16,   // e.g. 500 = 5%

    pub feed_id: [u8; 32],         // Pyth feed_id this lease is pinned to

    pub status: LeaseStatus,       // Listed | Active | Liquidated | Closed

    pub bump: u8,
    pub leased_vault_bump: u8,
    pub collateral_vault_bump: u8,
}
```

### The short seller takes the offer - `take_lease`

A short seller who has spotted the `Lease` account onchain (via an
indexer or a direct lookup) calls `take_lease` to take delivery. The
program deposits the short seller's collateral into `collateral_vault`
first - the vault was created empty by `create_lease` and this is
the call that fills it - then hands over the leased tokens.
Depositing collateral first means that if the leased-token payout
fails for any reason the whole transaction reverts and the short
seller gets their collateral back. The lease moves from `Listed` to
`Active`.

- **Signers:** `short_seller` (the user wallet borrowing the tokens
  and posting collateral).
- **Accounts:**
  - `short_seller` (signer, mut)
  - `holder` (UncheckedAccount - read for program-derived address seed derivation only, no signature required)
  - `lease` (mut, `has_one = holder`, `has_one = leased_mint`, `has_one = collateral_mint`, must be `Listed`)
  - `leased_mint`, `collateral_mint`
  - `leased_vault`, `collateral_vault` (both mut, both program-derived addresses)
  - `short_seller_collateral_account` (mut, short seller's associated token account for the collateral mint - source)
  - `short_seller_leased_account` (mut, **init_if_needed** - short seller's associated token account for the leased mint, destination)
  - `token_program`, `associated_token_program`, `system_program`
- **What happens:**
  - Two token movements, in order:
    1. `required_collateral_amount` of the collateral mint moves
       from `short_seller_collateral_account` into `collateral_vault`.
    2. `leased_amount` of the leased mint moves from `leased_vault`
       to `short_seller_leased_account`.
  - State changes on `lease`:
    - `short_seller = short_seller.key()`
    - `collateral_amount = required_collateral_amount`
    - `start_timestamp = now`
    - `end_timestamp = now + duration_seconds` (checked add)
    - `last_paid_timestamp = now` (nothing has accrued yet)
    - `status = Active`
- **Errors:**
  - `InvalidLeaseStatus` if the lease is not `Listed`
  - Anchor `has_one` mismatch errors if `holder`, `leased_mint`, or
    `collateral_mint` do not match the values stored on the lease
  - `MathOverflow` if `now + duration_seconds` overflows `i64`

### The lease fee streams - `pay_lease_fee`

The lease fee accrues second by second out of the collateral vault.
Anyone can call `pay_lease_fee` to settle whatever has accrued since
the last settlement: the short seller has the obvious incentive (keep
the position out of liquidation), and a keeper bot may push a payment
before checking margins so healthy leases stay healthy. The fee
formula is `(min(now, end_timestamp) - last_paid_timestamp) *
lease_fee_per_second`, capped at the collateral actually sitting in
the vault. Fees do not accrue past `end_timestamp` - once the
deadline hits, the short seller is either returning the tokens,
being liquidated, or defaulting; no further lease fees are owed.

- **Signers:** `payer` (any user wallet - the short seller, a
  keeper bot, or anyone else willing to pay the transaction fee).
- **Accounts:**
  - `payer` (signer, mut - pays for `init_if_needed` of the holder associated token account)
  - `holder` (UncheckedAccount, read-only - used for `has_one` check)
  - `lease` (mut, must be `Active`)
  - `collateral_mint`, `collateral_vault`
  - `holder_collateral_account` (mut, **init_if_needed** - holder's [associated token account](https://solana.com/docs/terminology) for the collateral mint, destination for the lease fee)
  - `token_program`, `associated_token_program`, `system_program`
- **What happens:**
  - Compute `lease_fee_due = (min(now, end_timestamp) - last_paid_timestamp) * lease_fee_per_second`.
  - Compute `payable = min(lease_fee_due, lease.collateral_amount)`.
  - If `payable > 0`, transfer `payable` of the collateral mint from
    `collateral_vault` to `holder_collateral_account`.
  - State changes: `lease.collateral_amount -= payable`,
    `lease.last_paid_timestamp = min(now, end_timestamp)`.
  - If the vault did not have enough collateral to cover the full
    `lease_fee_due`, the residual is silently left as a debt the next
    `liquidate` or `close_expired` call cleans up. (See
    [safety and edge cases](#safety-and-edge-cases) for
    the rationale on this trade-off.)
- **Errors:**
  - `InvalidLeaseStatus` if the lease is not `Active`
  - `MathOverflow` if `elapsed * lease_fee_per_second` overflows `u64`

### The short seller defends the position - `top_up_collateral`

If the price moves against the short seller and the position drifts
toward the maintenance-margin floor, the short seller can add more
collateral to push the ratio back up. They call `top_up_collateral`
with an `amount` of the collateral mint, which the program transfers
straight into `collateral_vault` and adds to `lease.collateral_amount`.
The short seller can call this any number of times while the lease
is `Active`.

- **Signers:** `short_seller`.
- **Parameter:** `amount: u64` - how much collateral to add.
- **Accounts:**
  - `short_seller` (signer)
  - `holder` (UncheckedAccount, read-only)
  - `lease` (mut, `has_one = holder`, `has_one = collateral_mint`, must be `Active`, must be the same `short_seller`)
  - `collateral_mint`, `collateral_vault`
  - `short_seller_collateral_account` (mut, source)
  - `token_program`
- **What happens:**
  - Transfer `amount` of the collateral mint from
    `short_seller_collateral_account` into `collateral_vault`.
  - `lease.collateral_amount += amount` (checked add).
- **Errors:**
  - `InvalidCollateralAmount` if `amount == 0`
  - `Unauthorised` if `lease.short_seller != short_seller.key()`
  - `InvalidLeaseStatus` if the lease is not `Active`
  - `MathOverflow` if the addition overflows `u64`

### The short seller closes - `return_lease`

To close the position, the short seller buys back the leased tokens
on the open market and calls `return_lease`. The program runs the
full settlement in a single transaction: leased tokens move from the
short seller back to the holder, accrued lease fees move from the
collateral vault to the holder, the leftover collateral refunds to
the short seller, and both vaults plus the `Lease` account close.
The handler accepts a return at any time while `status == Active` -
returning before `end_timestamp` just means lease fees stop accruing
the moment the call lands; returning after `end_timestamp` does not
pile on extra charges because the fee formula already caps elapsed
time at `end_timestamp`.

- **Signers:** `short_seller`.
- **Accounts:**
  - `short_seller` (signer, mut)
  - `holder` (UncheckedAccount, mut - receives `Lease` and vault rent-exempt lamports via `close = holder`)
  - `lease` (mut, `close = holder`, must be `Active`, must be the same `short_seller`)
  - `leased_mint`, `collateral_mint`
  - `leased_vault`, `collateral_vault` (both mut)
  - `short_seller_leased_account` (mut, source for the return)
  - `short_seller_collateral_account` (mut, destination for the collateral refund)
  - `holder_leased_account` (mut, **init_if_needed**)
  - `holder_collateral_account` (mut, **init_if_needed**)
  - `token_program`, `associated_token_program`, `system_program`
- **What happens:**
  - Four token movements, in order:
    1. `leased_amount` of the leased mint moves from
       `short_seller_leased_account` into `leased_vault`.
    2. The same `leased_amount` moves out of `leased_vault` into
       `holder_leased_account`. The leased tokens hop through the
       vault rather than going direct from short seller to holder so
       the program can reuse the vault's program-derived-address
       signing path; the atomic round-trip leaves the vault empty
       and ready to close.
    3. `lease_fee_payable = min(lease_fee_due, lease.collateral_amount)`
       of the collateral mint moves from `collateral_vault` to
       `holder_collateral_account`.
    4. The remaining `lease.collateral_amount - lease_fee_payable`
       refunds from `collateral_vault` to `short_seller_collateral_account`.
  - Both vaults close via `close_account` [cross-program invocations](https://solana.com/docs/terminology);
    their rent-exempt lamports go to the holder. The `Lease` account
    closes via Anchor's `close = holder` constraint, with its
    rent-exempt lamports going to the holder too.
  - State changes before close:
    `lease.last_paid_timestamp = min(now, end_timestamp)`,
    `lease.collateral_amount = 0`, `lease.status = Closed`.
- **Errors:**
  - `InvalidLeaseStatus` if the lease is not `Active`
  - `Unauthorised` if `lease.short_seller != short_seller.key()`
  - `MathOverflow` if the lease-fee or collateral subtraction overflows

`return_lease` is the first place an account-close happens; the same
mechanism runs in `liquidate` and `close_expired`. The `Closed` and
`Liquidated` states are not directly observable onchain: all three
of `return_lease`, `liquidate` and `close_expired` close the `Lease`
account in the same transaction (`close = holder`), returning the
rent-exempt lamports to the holder. The in-memory `status` field is
set *before* the close so the transaction logs record the terminal
state, but the account disappears at the end.

### Branch: position underwater - `liquidate`

If the leased asset rallies far enough that the locked collateral is
no longer worth more than the debt times the maintenance margin,
anyone - typically a keeper bot - can call `liquidate` with a fresh
Pyth price update. The program decodes the update by hand
(production code would use `pyth-solana-receiver-sdk`; the LiteSVM
tests install a `PriceUpdateV2` account whose layout is parsed
inline), checks the position is genuinely underwater, settles the
accrued lease fee to the holder, pays the keeper a bounty out of
what remains, and sends the rest to the holder. The leased tokens
stay with the short seller - the collateral is the holder's
compensation for the lost asset.

The underwater check, in integers, is:

`collateral_value * 10_000 < debt_value * maintenance_margin_basis_points`

where `debt_value = leased_amount * price * 10^exponent`, with the
Pyth exponent folded into whichever side of the inequality keeps the
math non-negative (see [`is_underwater`](programs/asset-leasing/src/instructions/liquidate.rs)).

- **Signers:** `keeper` (any user wallet - typically a bot watching
  for underwater positions; receives the bounty as payment for
  cleaning up).
- **Accounts:**
  - `keeper` (signer, mut - pays `init_if_needed` cost for both associated token accounts)
  - `holder` (UncheckedAccount, mut - receives lease fee, holder share, and the rent-exempt lamports from the three closed accounts)
  - `lease` (mut, `close = holder`, must be `Active`)
  - `leased_mint`, `collateral_mint`
  - `leased_vault`, `collateral_vault` (both mut)
  - `holder_collateral_account` (mut, **init_if_needed**)
  - `keeper_collateral_account` (mut, **init_if_needed** - keeper's [associated token account](https://solana.com/docs/terminology) for the collateral mint, destination for the bounty)
  - `price_update` (UncheckedAccount, constrained to `owner = PYTH_RECEIVER_PROGRAM_ID`) - a `PriceUpdateV2` account owned by the Pyth Receiver program for the feed the lease was pinned to at creation. This is the first handler that requires the oracle account itself; `create_lease` only stores the `feed_id` it expects to see here.
  - `token_program`, `associated_token_program`, `system_program`
- **What happens:**
  - Decode `price_update`: discriminator must match
    `PRICE_UPDATE_V2_DISCRIMINATOR`, account length ≥ 89 bytes,
    `feed_id` must equal `lease.feed_id`,
    `0 < now - publish_time <= 60 seconds`, `price > 0`. The
    decoded `feed_id` check is the **feed-pinning** guard - without
    it a keeper could pass any feed the Pyth Receiver program owns
    (a wildly volatile pair that happens to be dipping, say) to
    force a spurious liquidation.
  - Confirm `is_underwater` returns true.
  - Three collateral movements, in order:
    1. `lease_fee_payable = min(lease_fee_due, lease.collateral_amount)`
       of the collateral mint moves from `collateral_vault` to
       `holder_collateral_account`.
    2. `bounty = (remaining * liquidation_bounty_basis_points) / 10_000`
       moves from `collateral_vault` to `keeper_collateral_account`,
       where `remaining = lease.collateral_amount - lease_fee_payable`.
    3. `remaining - bounty` moves from `collateral_vault` to
       `holder_collateral_account`.
  - Both vaults close - `leased_vault` is already empty because the
    short seller kept the leased tokens - and their rent-exempt
    lamports go to the holder. The `Lease` account closes the same
    way via Anchor's `close = holder`.
  - State changes before close:
    `lease.collateral_amount = 0`,
    `lease.last_paid_timestamp = min(now, end_timestamp)`,
    `lease.status = Liquidated`.
- **Errors:**
  - `StalePrice` if the discriminator does not match, the account is
    too short, `publish_time > now`, or `now - publish_time > 60`
  - `PriceFeedMismatch` if `decoded.feed_id != lease.feed_id`
  - `NonPositivePrice` if `price <= 0`
  - `PositionHealthy` if the underwater check fails
  - `InvalidLeaseStatus` if the lease is not `Active`
  - `MathOverflow` on any of the integer-multiplication steps

### Branch: cancel or default - `close_expired`

The holder has a single recovery handler that covers two unrelated
situations:

- The lease sat in `Listed` and the holder wants to cancel it -
  no-one ever took the offer. Allowed any time.
- The lease was `Active`, `end_timestamp` has passed, and the short
  seller never called `return_lease`. The holder takes the entire
  collateral vault as compensation.

In both cases the program drains whichever vault is non-empty, closes
both vaults, and closes the `Lease` account, with all three
rent-exempt-lamport refunds going to the holder.

- **Signers:** `holder`.
- **Accounts:**
  - `holder` (signer, mut - also the rent destination for all three closes)
  - `lease` (mut, `close = holder`, status ∈ `{Listed, Active}`)
  - `leased_mint`, `collateral_mint`
  - `leased_vault`, `collateral_vault` (both mut)
  - `holder_leased_account` (mut, **init_if_needed**)
  - `holder_collateral_account` (mut, **init_if_needed**)
  - `token_program`, `associated_token_program`, `system_program`
- **What happens:**
  - On a `Listed` cancel: `leased_vault` holds `leased_amount` -
    drain it back to `holder_leased_account`. `collateral_vault` is
    empty, no transfer.
  - On an `Active` default (after `end_timestamp`):
    `leased_vault` is empty (the short seller kept the tokens),
    `collateral_vault` holds `lease.collateral_amount` - drain all
    of it to `holder_collateral_account`.
  - Both vaults close; the `Lease` account closes via Anchor's
    `close = holder`.
  - State changes before close:
    - On the `Active` branch only,
      `lease.last_paid_timestamp = min(now, end_timestamp)` - settles
      the timestamp invariant so a future program version that wants
      to split the default pot differently (pro-rata lease fees,
      partial refund) has a correct anchor to start from.
    - `lease.collateral_amount = 0`
    - `lease.status = Closed`
- **Errors:**
  - `InvalidLeaseStatus` if `status` is not `Listed` or `Active`
  - `LeaseNotExpired` if `status == Active` and `now < end_timestamp`

### Branch scenarios

The handlers above cover the happy path. The branch scenarios below
walk the same machinery through liquidation, a falling-price profit,
and the two `close_expired` situations using concrete numbers that
match the LiteSVM tests one-to-one. All four scenarios share the
same starting parameters; both mints are 6-decimal tokens, so 1 token
= 1 000 000 base units. "Leased units" means base units of the leased
mint and "collateral units" means base units of the collateral mint -
descriptive labels, not real tickers.

Shared starting parameters:

- `leased_amount = 100_000_000` (100 leased tokens)
- `required_collateral_amount = 200_000_000` (200 collateral tokens)
- `lease_fee_per_second = 10` collateral units
- `duration_seconds = 86_400` (24 hours)
- `maintenance_margin_basis_points = 12_000` (120%)
- `liquidation_bounty_basis_points = 500` (5% of post-lease-fee collateral)
- `feed_id = [0xAB; 32]` (arbitrary, consistent across all calls)

The holder starts with 1 000 000 000 leased units; the short seller
starts with 1 000 000 000 collateral units. Each scenario opens with
`create_lease` and (where relevant) `take_lease` running as described
in [the holder lists the tokens - `create_lease`](#the-holder-lists-the-tokens---create_lease) and [the short seller takes the offer - `take_lease`](#the-short-seller-takes-the-offer---take_lease). Lease fees use the formula in [the lease fee streams - `pay_lease_fee`](#the-lease-fee-streams---pay_lease_fee).

#### Liquidation - leased asset rallies

`create_lease` and `take_lease` run as standard, leaving
`collateral_vault = 200_000_000`, `leased_vault = 0`, and the short
seller holding 100 leased tokens. Time jumps to `T + 300`.

A keeper observes a fresh Pyth price update: the leased-in-collateral
price has spiked to 4.0 (exponent = 0, raw price = 4). Debt value is
`100_000_000 × 4 = 400_000_000` collateral units against a collateral
pot of ~200 000 000 - maintenance ratio is `200/400 = 50%`, far below
the required 120%. The keeper does not need to call `pay_lease_fee`
first; `liquidate` settles accrued fees itself.

The keeper calls `liquidate` (mechanics in [branch: position underwater - `liquidate`](#branch-position-underwater---liquidate)). At `T + 300`:

- Accrued lease fee: `300 × 10 = 3_000` collateral units. The vault
  has 200 000 000, so `lease_fee_payable = 3_000` flows to the holder.
- Remaining: `200_000_000 − 3_000 = 199_997_000` collateral units.
- Bounty: `199_997_000 × 500 / 10_000 = 9_999_850` collateral units to
  the keeper.
- Holder share: `199_997_000 − 9_999_850 = 189_997_150` collateral
  units to the holder.
- Both vaults close, the `Lease` account closes; status recorded as
  `Liquidated`.

Final balances:

- **Holder:** 900 000 000 leased units (the 100 never came back - the
  short seller kept them), `3_000 + 189_997_150 = 190_000_150`
  collateral units, plus rent-exempt lamports from three closes.
- **Short seller:** still holds 100 000 000 leased units, lost the
  full 200 000 000 collateral.
- **Keeper:** 9 999 850 collateral units.

The asymmetry to remember: liquidation does *not* reclaim the leased
tokens. The collateral pays the holder for the lost asset; the short
seller has effectively bought the leased tokens at the forfeit price.

#### Falling price - short seller profits

`create_lease` and `take_lease` run as standard. Time jumps to
`T + 300`. The leased-in-collateral price has fallen sharply: take
exponent = −1, raw price = 5, so debt value is
`100_000_000 × 5 / 10 = 50_000_000` collateral units. The collateral
pot is ~200 000 000 - maintenance ratio is `200_000_000 / 50_000_000
= 400%`, far above the required 120%. A keeper calling `liquidate`
here would fail with `PositionHealthy`; the program refuses to seize
a healthy position.

At `T + 600` (10 minutes in) the short seller buys 100 leased tokens
on the open market at the new price (about 50 collateral tokens
total - far less than the 200 they posted) and calls `return_lease`
(mechanics in [the short seller closes - `return_lease`](#the-short-seller-closes---return_lease)). Accrued lease fees are `600 × 10 = 6_000`
collateral units. The settlement:

- 100 000 000 leased units flow short seller → leased vault → holder.
- 6 000 collateral units flow from the collateral vault to the holder.
- The remaining `200_000_000 − 6_000 = 199_994_000` collateral units
  refund to the short seller.
- Both vaults close, the `Lease` account closes.

Final balances:

- **Holder:** 1 000 000 000 leased units (full return), 6 000
  collateral units in lease fees.
- **Short seller:** received 100 leased tokens, sold them at the
  original price, bought 100 leased tokens back at the lower price,
  returned them. Net cost is the lending fee (6 000 collateral units)
  plus open-market trading costs; gain is the difference between the
  original sale price and the buy-back price. The standard short
  payoff.

The short seller can defend a borderline position with
`top_up_collateral` ([the short seller defends the position - `top_up_collateral`](#the-short-seller-defends-the-position---top_up_collateral)) or close it early via `return_lease`
([the short seller closes - `return_lease`](#the-short-seller-closes---return_lease)). Only adverse price moves trigger liquidation.

#### Default - `close_expired` on an `Active` lease

`create_lease` and `take_lease` run as standard. The short seller
takes the tokens, posts collateral, then disappears. `pay_lease_fee`
is never called. The clock advances past
`end_timestamp = T + 86_400`.

At `T + 100_000` the holder calls `close_expired` (mechanics in
[branch: cancel or default - `close_expired`](#branch-cancel-or-default---close_expired)). Because `status == Active` and `now >= end_timestamp`, the
default branch runs:

- `leased_vault` is empty (the short seller kept the tokens) - no
  transfer.
- `collateral_vault` holds 200 000 000 collateral units; all of it
  flows to `holder_collateral_account`.
- Both vaults close, the `Lease` account closes;
  `last_paid_timestamp` settles at `end_timestamp`.

Final balances:

- **Holder:** 900 000 000 leased units, 200 000 000 collateral units
  (the entire collateral pot as compensation), plus three
  account-close refunds.
- **Short seller:** 100 000 000 leased units, paid the full
  collateral and kept the leased tokens.

#### Cancel - `close_expired` on a `Listed` lease

The cheap cancel path. `create_lease` runs; no short seller ever
calls `take_lease`. The holder calls `close_expired` immediately
(mechanics in [branch: cancel or default - `close_expired`](#branch-cancel-or-default---close_expired)). Because `status == Listed`, no expiry check
applies:

- `leased_vault` holds 100 000 000 leased units; all of it drains
  back to `holder_leased_account`.
- `collateral_vault` is empty - no transfer.
- Both vaults close, the `Lease` account closes.

Final balances: the holder is back to 1 000 000 000 leased units;
nothing else moved.

---

## Safety and edge cases

### What the program refuses to do

All of the following come from [`errors.rs`](programs/asset-leasing/src/errors.rs)
and are enforced by either an Anchor constraint or a `require!` in the
handler:

- **`InvalidLeaseStatus`** - action tried against a lease in the wrong state (e.g. `take_lease` on a lease that is already `Active`).
- **`InvalidDuration`** - `duration_seconds <= 0` on `create_lease`.
- **`InvalidLeasedAmount`** - `leased_amount == 0` on `create_lease`.
- **`InvalidCollateralAmount`** - `required_collateral_amount == 0` on `create_lease`; `amount == 0` on `top_up_collateral`.
- **`InvalidLeaseFeePerSecond`** - `lease_fee_per_second == 0` on `create_lease`.
- **`InvalidMaintenanceMargin`** - `maintenance_margin_basis_points == 0` or `> 50_000` on `create_lease`.
- **`InvalidLiquidationBounty`** - `liquidation_bounty_basis_points > 2_000` on `create_lease`.
- **`LeaseExpired`** - reserved; not currently used (lease fee accrual naturally caps at `end_timestamp`).
- **`LeaseNotExpired`** - `close_expired` called on an `Active` lease before `end_timestamp`.
- **`PositionHealthy`** - `liquidate` called on a lease that passes the maintenance-margin check.
- **`StalePrice`** - Pyth price update older than 60 s, or has a future `publish_time`, or fails discriminator / length check.
- **`NonPositivePrice`** - Pyth price is `<= 0`.
- **`MathOverflow`** - any of the `checked_*` arithmetic returned `None`.
- **`Unauthorised`** - lease-modifying handler called by someone who is not the registered short seller (`top_up_collateral`, `return_lease`).
- **`LeasedMintEqualsCollateralMint`** - `create_lease` called with the same mint for both sides.
- **`PriceFeedMismatch`** - `liquidate` called with a Pyth update whose `feed_id` does not match `lease.feed_id`.

### Guarded design choices worth knowing

- **Leased tokens are locked up-front.** `create_lease` moves the tokens
  into the `leased_vault` immediately, so a short seller calling
  `take_lease` cannot fail because the holder spent the funds
  elsewhere in the meantime.

- **Leased mint ≠ collateral mint.** If both sides used the same
  mint, the two vaults would hold the same asset and the
  "what-do-I-owe-vs-what-do-I-hold" accounting would collapse. The
  guard is cheap and the error message is explicit.

- **Feed pinning.** The Pyth `feed_id` is stored on the `Lease` at
  creation and enforced on every `liquidate`. A keeper cannot pass in a
  random unrelated price feed (like a volatile pair that happens to be
  dipping) to force a spurious liquidation.

- **Staleness window.** Pyth `publish_time` older than 60 seconds is
  rejected, and `publish_time > now` is rejected too (keepers must not
  front-run the validator clock).

- **Integer-only math.** Every percentage and price calculation folds
  into a `checked_mul` / `checked_div` of `u128` - no floats, no
  surprising NaN. `BASIS_POINTS_DENOMINATOR = 10 000` is the only
  "percentage denominator" anywhere; cross-check against `constants.rs`
  if you're porting the math.

- **Authority-is-self vaults.** `leased_vault.authority ==
  leased_vault.key()` (and likewise for `collateral_vault`). The
  program signs as the vault using its own seeds, which means the
  `Lease` account is not involved in signing any of the token moves.
  Authority-is-self keeps the signer-seed array small (one seed list,
  not two).

- **Max maintenance margin = 500%.** Without an upper bound a holder
  could set a margin that is unreachable on day one and liquidate the
  short seller instantly. 50 000 basis points is generous - enough
  for truly speculative leases - while still blocking the pathological
  10 000× trap.

- **Max liquidation bounty = 20%.** Higher than 20% and the keeper's
  cut would dwarf the holder's recovery on default. The cap keeps
  liquidation economics roughly in line with holder-first semantics.

### Things the program does *not* guard against

A production version of the program would want more:

- **Price feed correctness.** The program verifies the owner
  (`PYTH_RECEIVER_PROGRAM_ID`), the discriminator, the layout and the
  feed id, but the program cannot know whether the feed the holder
  pinned quotes the right pair. Supplying the wrong feed at creation
  is the holder's problem - the wrong feed won't cause a liquidation
  to succeed against a truly healthy position (the feed id check
  would fail), but it will mean *no* liquidation can succeed, so a
  short seller could drain the collateral via lease fees and walk
  away. A production version would cross-check the price feed's
  `feed_id` against a program-maintained registry.

- **Lease-fee dust accumulation.** Lease fees are paid in whole base
  units per second of `lease_fee_per_second`. Choose a small
  `lease_fee_per_second` and short-lived leases can settle 0 lease
  fees if no-one calls `pay_lease_fee` for a very short period. Not a
  security issue - the accrual timestamp only moves forward when the
  lease fee is actually settled - but worth knowing.

- **Griefing on `init_if_needed`.** `take_lease`, `pay_lease_fee`,
  `liquidate`, `return_lease` and `close_expired` all do
  `init_if_needed` on one or more associated token accounts. If the
  caller does not fund the rent-exempt reserve for those accounts,
  the transaction fails. This is the intended behaviour (the caller
  pays for the state they require) but can surprise a short seller
  on a tight SOL budget.

- **No partial lease-fee refund on default.** When `close_expired`
  runs on an `Active` lease, the holder takes the entire collateral
  regardless of how many lease fees had actually accrued by then.
  This is a deliberate simplification - the `last_paid_timestamp`
  bookkeeping is in place precisely so a future version can split the
  pot correctly.

- **No pause / upgrade authority.** The program has no admin and no
  upgrade-authority-bound feature flags. The program runs or it doesn't.

---

## Running the tests

All the tests are LiteSVM-based Rust integration tests under
[`programs/asset-leasing/tests/`](programs/asset-leasing/tests/). They
exercise every instruction handler through `include_bytes!("../../../target/deploy/asset_leasing.so")`,
so a fresh build must produce the `.so` first.

### Prerequisites

- Anchor 1.0.0 (`anchor --version`)
- Solana CLI (`solana -V`)
- Rust stable (the `rust-toolchain.toml` at the repo root pins the
  compiler)

### Commands

From this directory (`defi/asset-leasing/anchor/`):

```bash
# 1. Build the BPF .so - writes to target/deploy/asset_leasing.so
anchor build

# 2. Run the LiteSVM tests (just cargo under the hood; `anchor test`
#    also works because Anchor.toml scripts.test = "cargo test")
cargo test --manifest-path programs/asset-leasing/Cargo.toml

# Or, equivalently:
anchor test --skip-local-validator
```

Expected output:

```
running 11 tests
test close_expired_cancels_listed_lease ... ok
test close_expired_reclaims_collateral_after_end_timestamp ... ok
test create_lease_locks_tokens_and_lists ... ok
test create_lease_rejects_same_mint_for_leased_and_collateral ... ok
test liquidate_rejects_healthy_position ... ok
test liquidate_rejects_mismatched_price_feed ... ok
test liquidate_seizes_collateral_on_price_drop ... ok
test pay_lease_fee_streams_collateral_by_elapsed_time ... ok
test return_lease_refunds_unused_collateral ... ok
test take_lease_posts_collateral_and_delivers_tokens ... ok
test top_up_collateral_increases_vault_balance ... ok
```

### What each test exercises

- **`create_lease_locks_tokens_and_lists`** - holder funds vault, `Lease` created, collateral vault empty.
- **`create_lease_rejects_same_mint_for_leased_and_collateral`** - guard against `leased_mint == collateral_mint`.
- **`take_lease_posts_collateral_and_delivers_tokens`** - collateral deposit + leased-token payout in one instruction.
- **`pay_lease_fee_streams_collateral_by_elapsed_time`** - lease fee math: `elapsed * lease_fee_per_second`, lease fee transferred to holder.
- **`top_up_collateral_increases_vault_balance`** - collateral balance after `top_up` equals deposit + top-up.
- **`return_lease_refunds_unused_collateral`** - happy path round-trip; leased tokens returned, residual collateral refunded, accounts closed.
- **`liquidate_seizes_collateral_on_price_drop`** - price-induced underwater position; lease fee + bounty + holder share paid, accounts closed.
- **`liquidate_rejects_healthy_position`** - program refuses to liquidate a position that passes the margin check.
- **`liquidate_rejects_mismatched_price_feed`** - program refuses a `PriceUpdateV2` whose `feed_id` does not match `lease.feed_id`.
- **`close_expired_reclaims_collateral_after_end_timestamp`** - default path; holder seizes the collateral.
- **`close_expired_cancels_listed_lease`** - holder-initiated cancel of an unrented lease.

### Note on CI

The repo's `.github/workflows/anchor.yml` runs `anchor build` before
`anchor test` for every changed anchor project. That's important for
this project: the Rust integration tests include the BPF artefact via
`include_bytes!`, so a stale or missing `.so` would break the tests.
CI is already covered.

---

## Quasar port

A parallel implementation of the same program using
[Quasar](https://github.com/blueshift-gg/quasar) lives in
[`../quasar/`](../quasar/). Quasar is a lightweight alternative to
Anchor that compiles to bare Solana program binaries without pulling in
`anchor-lang` - useful when you care about compute-unit budget, binary
size, or simply want fewer layers between your code and the runtime.

The port implements the same seven instruction handlers, the same
`Lease` state account, the same program-derived address seed
conventions, and produces the same onchain behaviour for every
happy-path and adversarial test in this README.

### Building and testing

From [`../quasar/`](../quasar/):

```bash
# Build the .so using the quasar CLI.
quasar build

# Run the LiteSVM-style tests directly with cargo. The tests call the
# compiled program from `target/deploy/quasar_asset_leasing.so`.
cargo test
```

The Quasar example in this repo's CI workflow
(`.github/workflows/quasar.yml`) runs exactly those two commands.

### What differs from the Anchor version

- **No Anchor account-validation macros.** In Quasar, account structs
  use `#[derive(Accounts)]` with an almost-identical attribute
  vocabulary (`seeds`, `bump`, `has_one`, `constraint`,
  `init_if_needed`) but the checks are lowered to plain Rust, not
  inserted by a procedural macro that calls into a support crate.

- **Explicit instruction discriminators.** Each instruction handler
  carries `#[instruction(discriminator = N)]` with `N` an explicit
  integer - Quasar uses one-byte discriminators by default rather than
  Anchor's 8-byte sha256 prefix. The wire format for every call is
  `[discriminator: u8][borsh-serialised args]`.

- **Tests talk to `QuasarSvm` directly.** Instead of the Anchor
  `Instruction { ... }.data()` / `accounts::Foo { ... }.to_account_metas()`
  helpers, the Quasar tests build each `Instruction` by hand with
  `solana_instruction::AccountMeta` entries and a manually-assembled
  byte payload. Account state is pre-populated on the SVM with
  `QuasarSvm::new().with_program(...).with_token_program()` and
  helpers from `quasar_svm::token` that synthesise `Mint` and
  `TokenAccount` bytes without running the real token-program
  initialisation instruction handlers. This keeps the tests fast but
  means the setup code is more explicit.

- **No cross-program-invocation into an associated-token-account
  program for associated token account creation.** The Anchor version
  uses `init_if_needed` + `associated_token::...` to let callers pass
  in a holder/short-seller wallet and get the token account created
  on demand. The Quasar port accepts pre-created token accounts for
  the user side of every flow, since doing `init_if_needed` correctly
  for associated token accounts in Quasar requires wiring in the
  associated token account program manually and adds noise that
  distracts from the lease mechanics. Production code would want the
  associated token account convenience back.

- **Classic Token only, not Token-2022.** The Anchor version declares
  its token accounts as `InterfaceAccount<Token>` + `token_program:
  Interface<TokenInterface>`, which accepts mints owned by either the
  classic Token program or the Token-2022 program. The Quasar port
  uses `Account<Token>` + `Program<Token>`, matching the simpler
  pattern used by the other Quasar examples in this repo. Adding
  Token-2022 support is a type-parameter swap away.

- **State layout is the same, byte for byte.** The `Lease` discriminator
  and field order match the Anchor version, so an offchain indexer
  that already decodes Anchor `Lease` accounts would also decode the
  Quasar ones after adjusting for the one-byte discriminator.

- **One lease per holder at a time.** The Anchor version keys its
  `Lease` program-derived address on `[LEASE_SEED, holder, lease_id]`
  so one holder can run many leases in parallel. Quasar's `seeds = [...]`
  macro embeds raw references into generated code and does not (yet)
  have a borrow-safe way to splice instruction args like
  `lease_id.to_le_bytes()` into the seed list, so the Quasar port
  keys its program-derived address on `[LEASE_SEED, holder]` alone -
  one active lease per holder. The `lease_id` is still stored on the
  `Lease` account for book-keeping and is a caller-supplied u64 in
  `create_lease`; the offchain client just has to ensure the previous
  lease from the same holder is `Closed` or `Liquidated` (i.e. its
  program-derived address account is gone) before creating a new one.
  Swapping in a multi-lease seed is a mechanical change once Quasar
  grows support for dynamic-byte seeds.

The code layout mirrors this directory: `src/lib.rs` registers the
entrypoint and re-exports handlers, `src/state.rs` defines `Lease` and
`LeaseStatus`, and `src/instructions/*.rs` contains one file per
handler. Tests are in `src/tests.rs`.

---

## Extending the program

Directions a real-world version of the program would consider,
grouped by effort:

### Easy

- **Add a `lease_view` read-only helper.** An offchain indexer-style
  struct that returns `{ collateral_value, debt_value, ratio_basis_points,
  is_underwater }` given the same inputs `is_underwater` uses. Useful
  for UIs that want to show "you are 15% away from liquidation".

- **Cap lease fees at collateral.** Currently `pay_lease_fee` pays
  `min(lease_fee_due, collateral_amount)` and silently leaves a debt.
  Add an explicit `LeaseFeeDebtOutstanding` error so the caller is
  warned when the stream has stalled, rather than inferring it from a
  non-zero `lease_fee_due` after settlement.

### Moderate

- **Partial-refund default.** In `close_expired` on `Active`, instead
  of giving the holder the entire collateral, split it: `lease_fee_due`
  to the holder, the rest stays with the short seller up to some
  `default_haircut_basis_points`. `last_paid_timestamp` is already
  bumped after a default close, so the timestamp invariants are ready.

- **Multiple outstanding leases per `(holder, short_seller)` pair with
  the same mint pair.** Already supported via `lease_id`, but add an
  instruction-level index account that lists open lease ids for a
  given holder so offchain tools don't have to `getProgramAccounts`
  scan.

- **Quote asset ≠ collateral mint.** Rent and liquidation math assume
  debt is priced in *collateral units*. Generalise to a third "quote"
  mint by taking the price pair at creation and carrying a
  `quote_mint` pubkey on `Lease`. Requires updates to
  `is_underwater` and a second oracle feed.

### Harder

- **Keeper auction.** Replace the fixed `liquidation_bounty_basis_points` with a
  Dutch auction that grows the bounty linearly over some window after
  the position first becomes underwater. Keeps liquidators honest on
  tight feeds and gives short sellers a chance to `top_up_collateral`
  before a keeper has an economic reason to move.

- **Flash liquidation.** Let the keeper settle the debt in the same
  transaction as the liquidation - borrow the leased amount from a
  separate liquidity pool, hand it to the holder, take the full
  collateral, repay the pool, keep the spread. Requires integrating a
  second program.

- **Token-2022 support.** The program already uses the `TokenInterface`
  trait so it accepts mints owned by either the classic Token program
  or the Token-2022 program. A real extension would test against
  Token-2022 mint extensions (transfer-fee, interest-bearing) and
  document which are compatible with the lease-fee / collateral flows.

---

## Code layout

```
defi/asset-leasing/anchor/
├── Anchor.toml
├── Cargo.toml
├── README.md              (this file)
└── programs/asset-leasing/
    ├── Cargo.toml
    ├── src/
    │   ├── constants.rs    program-derived address seeds, basis points limits, Pyth age cap
    │   ├── errors.rs
    │   ├── lib.rs          #[program] entry points
    │   ├── instructions/
    │   │   ├── mod.rs
    │   │   ├── shared.rs           transfer / close helpers
    │   │   ├── create_lease.rs
    │   │   ├── take_lease.rs
    │   │   ├── pay_lease_fee.rs
    │   │   ├── top_up_collateral.rs
    │   │   ├── return_lease.rs
    │   │   ├── liquidate.rs
    │   │   └── close_expired.rs
    │   └── state/
    │       ├── mod.rs
    │       └── lease.rs
    └── tests/
        └── test_asset_leasing.rs   LiteSVM tests
```
