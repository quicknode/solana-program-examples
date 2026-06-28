# Vault Strategy: a walkthrough

A video script for the `vault-strategy` example. Target runtime is roughly eight minutes at a normal speaking pace. Narration lines are what the presenter says; the indented blocks are what is on screen as a running ledger of onchain state.

Prices for TSLAx and NVDAx in this script are illustrative and match the rates the example's tests configure. They are not live quotes. USDC (US dollars), TSLAx (Tesla stock) and NVDAx (NVIDIA stock) are real assets; the swap behind the scenes is a deterministic test stand-in, which we will be honest about when we reach it.

## What we are building

NARRATION:

Let's build a vault strategy: the onchain equivalent of a mutual fund, or an actively managed ETF. You deposit cash with a manager, you receive shares, the manager invests across several assets and rebalances them over time, and your shares are priced at net asset value: the worth of everything the strategy holds, divided by the shares outstanding. The word net is a finance convention for value after subtracting what a fund owes; this strategy borrows nothing, so its net asset value is simply its holdings. For running the book, the manager earns a fee.

By the end you will have watched someone deposit, the manager invest and rebalance, a fee accrue, and someone redeem, and you will know which instruction handler does each one. The program controls every dollar the whole time: the manager invests the deposits but can never move them to herself, a limit we will pin down precisely.

You have seen this shape on Solana, in protocols like Symmetry and Kamino. This is the teaching-sized version.

Two things genuinely change once the strategy is onchain:

- The rules are the deployed bytecode. Maria cannot freeze redemptions, the fee is fixed at creation and capped in code at ten percent, and there is no admin lever to pull.
- Entry and exit are permissionless and settle instantly. Anyone can deposit or redeem in a single transaction, priced live, with no minimum and no end-of-day cutoff.

We will hit each piece as it shows up.

## The accounts, and who can move what

NARRATION:

Custody is the whole game, so let us name the boxes before we move money.

First, the word vault, because it gets overloaded. By the common standard a vault holds a single asset: you put one kind of token in, you get shares out. A managed mix of several assets is not one vault; it lives in several vaults, one per asset, and is usually called a basket or a fund. Symmetry calls its multi-asset products baskets. We will keep it simple: a vault is one single-asset token account, and the strategy is the whole construct that owns them. So vault strategy reads literally, a strategy built from vaults.

The center of everything is the `Strategy` account, a PDA derived from the seeds `"strategy"` plus Maria's public key. It is the authority over four accounts: a USDC vault, a TSLAx vault, an NVDAx vault, and the share mint. Each vault is an associated token account owned by the strategy PDA, and holds exactly one asset. The share mint is also created at a PDA, seeds `"share_mint"` plus the strategy address. A PDA is just a public key with no private key behind it, derived from seeds so the program can find it and sign for it; the mint's address is therefore deterministic, one share mint per strategy, and the strategy PDA is its mint and freeze authority.

One structural limit to call out now: the program holds exactly two assets, A and B, plus USDC. Not three, not ten thousand. Supporting more would mean storing a list of mints, weights, and vaults and looping over them, which is a real code change, not a setting.

What that buys us: only the strategy PDA can sign to move tokens out of those vaults or to mint shares. Maria manages the strategy, but she cannot reach into the vaults with her own keypair. Her powers are exactly three instruction handlers, and we will see the limit on each.

ON SCREEN:

```
Strategy            [off curve - PDA, seeds: "strategy" + manager]
    authority over: vault_usdc, vault_asset_a, vault_asset_b, share_mint
share_mint          [off curve - PDA, seeds: "share_mint" + strategy]   authority = Strategy PDA
vault_usdc / _a / _b [off curve - ATAs, one asset each]                  authority = Strategy PDA
```

## Maria opens the strategy

NARRATION:

Maria is the strategy's manager, and she wants to run it and earn the fee. She calls `initialize_strategy`. She sets two target weights, one per asset: forty percent to TSLAx and sixty percent to NVDAx. Weights are written in basis points, so that is 4000 and 6000, and the program requires they sum to 10000, which is one hundred percent. A weight is a target share of the strategy's value, not a token balance and not a count of assets.

One honest detail up front, and it is the crux of how much you must trust Maria: those weights are a target she maintains by hand. The program stores them but no handler reads them to force an allocation. Deposits arrive as USDC and sit in the USDC vault until Maria chooses to invest. The 40/60 split is a promise she keeps with `invest` and `rebalance`, not a rule the bytecode enforces on each deposit.

So let us pin down exactly what Maria can and cannot do. She can do three things, all visible in this walkthrough: invest USDC into one of the two registered assets, rebalance from one registered asset into the other, and collect her fee. Every one of those is fenced:

- She can only trade the two asset mints registered at creation, and only through the one swap router registered at creation. The program checks the router's address on every call, so she cannot route through a different one later.
- Each swap carries a minimum-out that the transaction specifies, so a bad fill reverts instead of quietly bleeding a vault.
- Her fee is set once at creation, capped at ten percent a year, and paid only in newly minted shares. There is no setter to raise it later.
- There is no instruction anywhere that sends a vault's tokens to the manager. She can direct the assets; she cannot withdraw them.

What is left to trust, honestly, is the router Maria registered and that she sets sane slippage. With an honest router, the worst a careless manager can do is churn and pay market slippage, which hurts depositors but does not enrich her. She cannot abscond with the principal. That is the same trust boundary as a delegate-managed vault, where one manager trades pooled funds but can never pull them out.

The fee cap lives in code. `initialize_strategy` rejects any fee above `MAX_FEE_BPS`, ten percent, because the fee is paid by minting new shares, and an uncapped fee would let a manager dilute depositors to nothing by configuration alone.

ON SCREEN:

```
ADDED - Strategy            [off curve - PDA]
    manager: Maria   weight_bps_a: 4000 (TSLAx)   weight_bps_b: 6000 (NVDAx)
    fee_bps: 100   total_shares: 0   last_fee_accrual_timestamp: now

ADDED - share_mint, vault_usdc, vault_asset_a, vault_asset_b   (all empty)

TOKEN MOVEMENT: none - setup only
Fee generated: none
```

## Alice deposits 900 USDC

NARRATION:

Alice wants exposure to both stocks without buying and rebalancing them herself, so she calls `deposit` with 900 USDC. `deposit` is permissionless: any user can call it, not just the manager. This is buying into the strategy.

The handler prices her shares against net asset value, the total worth of the strategy. It reads both Pyth feeds straight from the raw account bytes at fixed offsets, checks each price is positive and no more than sixty seconds stale, and computes net asset value as the USDC vault balance plus each asset vault balance times its price. The strategy is empty, so net asset value is zero, and the first deposit is defined as one to one. Alice gets 900 shares. Shares carry six decimals, so under the hood that is 900 million minor units, but think of it as 900 shares worth a dollar each.

Checks, effects, interactions: the handler raises `total_shares` first, then pulls her USDC into the USDC vault, then mints her the shares with the strategy PDA signing.

ON SCREEN:

```
UPDATED - Strategy
    total_shares: 0 -> 900,000,000

UPDATED - vault_usdc        [authority = Strategy PDA]
    balance: 0 -> 900 USDC

UPDATED - Alice share ATA
    balance: 0 -> 900 shares

TOKEN MOVEMENT:
    Alice USDC ATA -> vault_usdc        900 USDC   (deposit)
    share_mint     -> Alice share ATA   900 shares (minted, Strategy PDA signs)

Fee generated: none - deposits do not accrue fees
```

## Maria puts the cash to work

NARRATION:

Now Maria earns her title. She calls `invest` twice. `invest` is manager-only; the account constraints require her signature via `has_one = manager`.

`invest` does not hold a price of its own. It makes a cross-program call into the swap router, which for this example is a deterministic mock: at a fixed rate it mints the asset into the matching vault and takes the USDC. First, 360 dollars into TSLAx at 250 dollars a share, so the TSLAx vault receives 1.44 TSLAx. Then 540 dollars into NVDAx at 180 dollars a share, so the NVDAx vault receives exactly 3 NVDAx. That is the 40/60 split, by hand.

The strategy PDA signs both swaps, because the USDC is leaving the USDC vault, which only the PDA controls.

ON SCREEN:

```
UPDATED - vault_usdc        540 USDC -> 0 USDC      (across both invests)
UPDATED - vault_asset_a     0 -> 1.44 TSLAx
UPDATED - vault_asset_b     0 -> 3.0 NVDAx

TOKEN MOVEMENT (invest #1):
    vault_usdc -> router treasury   360 USDC
    router     -> vault_asset_a     1.44 TSLAx   (router mints; 360 / 250)
TOKEN MOVEMENT (invest #2):
    vault_usdc -> router treasury   540 USDC
    router     -> vault_asset_b     3.0 NVDAx    (router mints; 540 / 180)

Net asset value now: 0 + 1.44 x 250 + 3.0 x 180 = 360 + 540 = 900 USDC
Fee generated: none
```

## NVIDIA rises, and Bob pays the new price

NARRATION:

Time passes. NVDAx climbs from 180 to 200. Nothing onchain changes from a price move by itself; the NVDAx vault still holds the same 3 NVDAx, now worth more. Net asset value rises to 960 dollars while the share count is still 900. Each share is now worth about a dollar and seven cents.

Bob wants the same exposure Alice has, but he arrives now, after the gain, so he is the one who shows us how shares are priced. He calls `deposit` with 480 dollars. This is the moment the share math matters, and it is the same rule a mutual fund uses: you buy shares at today's net asset value. Bob does not get 480 shares. The handler computes shares as his deposit times total shares divided by net asset value: 480 times 900 divided by 960, which is exactly 450 shares. He pays the current price, so he does not dilute Alice's gain, and Alice's earlier deposit does not subsidize his.

ON SCREEN:

```
Net asset value before Bob: 0 + 1.44 x 250 + 3.0 x 200 = 360 + 600 = 960 USDC

UPDATED - Strategy
    total_shares: 900,000,000 -> 1,350,000,000

UPDATED - vault_usdc        0 -> 480 USDC
UPDATED - Bob share ATA     0 -> 450 shares

TOKEN MOVEMENT:
    Bob USDC ATA -> vault_usdc        480 USDC
    share_mint   -> Bob share ATA     450 shares   (480 x 900 / 960 = 450)

Fee generated: none
```

## Maria rebalances back toward target

NARRATION:

NVIDIA's run pushed the holdings away from 40/60, so Maria calls `rebalance`. One handler, two swaps, both signed by the strategy PDA: it sells one asset for USDC, then spends that USDC on the other.

She sells 0.36 TSLAx, receiving 90 dollars, then buys 0.5 NVDAx with that same 90 dollars. Both legs name a minimum out, so a bad rate would revert rather than silently lose value. The USDC vault nets to zero change across the two legs; the strategy just shifts weight from Tesla into NVIDIA.

ON SCREEN:

```
UPDATED - vault_asset_a     1.44 TSLAx -> 1.08 TSLAx     (sold 0.36)
UPDATED - vault_asset_b     3.0 NVDAx  -> 3.5 NVDAx       (bought 0.5)
UPDATED - vault_usdc        480 USDC -> 480 USDC          (+90 then -90)

TOKEN MOVEMENT:
    sell leg: vault_asset_a -> router (burned) 0.36 TSLAx; router treasury -> vault_usdc 90 USDC
    buy  leg: vault_usdc -> router treasury 90 USDC; router -> vault_asset_b 0.5 NVDAx

Net asset value: 480 + 1.08 x 250 + 3.5 x 200 = 480 + 270 + 700 = 1,450 USDC
Fee generated: none - rebalance moves assets, it does not charge a fee
```

## Maria collects her fee

NARRATION:

Maria calls `collect_fees`. This is a streaming management fee, and the mechanism is worth dwelling on, because it is the opposite of the offchain world. A traditional fund deducts its expense ratio from fund assets, selling holdings to pay the manager in cash, which lowers net asset value. This program touches no vault at all. It mints new shares to the manager, proportional to time elapsed and the fee rate. Over a full year at one percent, that is one percent of the share supply, 13.5 shares, minted to Maria.

Same economics, different lever. New shares with no new assets behind them make every existing share a slightly thinner slice, so the dilution, spread across all holders, is how Alice and Bob pay the fee. Minting fee shares is the common onchain pattern: Yearn and Lido both charge their fees this way rather than skimming assets. And it is honest to say there is no performance fee here, only this management fee on assets under management, bounded by the cap from creation.

ON SCREEN:

```
elapsed: 1 year (illustrative)
fee_shares = total_shares x fee_bps x elapsed / (10,000 x seconds_per_year)
           = 1,350,000,000 x 100 x 1yr / (10,000 x 1yr) = 13,500,000  (13.5 shares)

UPDATED - Strategy
    total_shares: 1,350,000,000 -> 1,363,500,000
    last_fee_accrual_timestamp: updated

UPDATED - Maria share ATA   0 -> 13.5 shares

TOKEN MOVEMENT:
    share_mint -> Maria share ATA   13.5 shares (minted, Strategy PDA signs)
Fee generated: 13.5 shares to the manager; all other holders diluted ~1%
```

## Alice withdraws

NARRATION:

Alice calls `withdraw` and burns all 900 of her shares. Here is the part people miss: withdrawal is in kind and proportional. She does not get cash. She gets her exact fraction of every balance the strategy holds, across all three vaults: USDC, TSLAx, and NVDAx alike. It is the same move an ETF makes when it redeems in kind, handing back the underlying holdings instead of cash.

Her fraction is 900 shares out of the 1,363.5 that now exist. The handler floors each amount in the protocol's favor, so any rounding dust stays with the remaining holders.

ON SCREEN:

```
Alice fraction = 900,000,000 / 1,363,500,000

amount_usdc = 480,000,000 x 900,000,000 / 1,363,500,000 = 316,831,683  (316.83 USDC, floor)
amount_a    =   1,080,000 x 900,000,000 / 1,363,500,000 =     712,871  (0.712871 TSLAx, floor)
amount_b    =   3,500,000 x 900,000,000 / 1,363,500,000 =   2,310,231  (2.310231 NVDAx, floor)

UPDATED - Strategy        total_shares: 1,363,500,000 -> 463,500,000
UPDATED - Alice share ATA 900 shares -> 0   (burned)

TOKEN MOVEMENT:
    share_mint burns 900 shares from Alice
    vault_usdc     -> Alice   316.83 USDC
    vault_asset_a  -> Alice   0.712871 TSLAx
    vault_asset_b  -> Alice   2.310231 NVDAx

Alice payout value @ 250 / 200 = 316.83 + 0.712871 x 250 + 2.310231 x 200 = about 957.10 USDC
Fee generated: none - withdrawals do not accrue fees
```

## Reconcile, and where everyone ended up

NARRATION:

Let us check the books. USDC into the USDC vault was 900 from Alice plus 480 from Bob, 1,380 total. The invests sent 900 to the router; rebalance was a wash. That leaves 480 in the USDC vault, and after Alice's withdrawal, 163.17 remains. Tokens in equal tokens out.

So: Alice came in with 900 dollars, rode NVIDIA up, paid her share of a one percent fee through dilution, and left with about 957 dollars of assets, in kind. The strategy passes returns through in both directions: had NVIDIA fallen instead of risen, the same arithmetic would have redeemed Alice for less than her 900 dollars. That market risk is hers, and the program neither cushions it nor hides it. Bob bought in fairly at the higher share price and still holds 450 shares worth roughly 478 dollars. Maria earned 13.5 shares, about 14 dollars, for running the book. The strategy held custody from the first deposit to the last withdrawal, the manager never touched the vaults with her own key, and the fee she could charge was capped in the bytecode.

## Two honest footnotes

NARRATION:

First, the swap router here is a deterministic test stand-in. It mints and burns at a fixed rate with no spread, and its rate matches the Pyth price, which keeps the math clean for teaching. A real deployment would call out to a live venue, and the strategy would only trust the one router address it registered at initialization. That registration is checked on every `invest` and `rebalance`.

Second, the 40/60 weights are a target Maria maintains, not an allocation the program enforces per deposit. Before real money, that is the thing to harden: enforce the weights in-program, timelock any change to the router, and bound per-swap slippage. Those are additions, not assumptions, and they would shrink what depositors must take on trust to almost nothing.

That is the whole lifecycle: open, deposit, invest, price in new depositors fairly, rebalance, charge a bounded streaming fee, and redeem in kind. Thanks for watching.
