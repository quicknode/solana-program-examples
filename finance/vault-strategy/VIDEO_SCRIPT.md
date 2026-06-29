# Vault Strategy: a walkthrough

A video script for the `vault-strategy` example. Target runtime is roughly nine minutes at a normal speaking pace. Narration lines are what the presenter says; the indented blocks are what is on screen as a running ledger of onchain state.

Prices for TSLAx and NVDAx in this script are illustrative and match the rates the example's tests configure. They are not live quotes. USDC (US dollars), TSLAx (Tesla stock) and NVDAx (NVIDIA stock) are real assets; the swap behind the scenes is a deterministic test stand-in, which we will be honest about when we reach it.

## What we are building

NARRATION:

Let's build a vault strategy: the onchain equivalent of a mutual fund, or an actively managed ETF. You deposit cash with a manager, you receive shares, the manager invests across several assets and rebalances them over time, and your shares are priced at net asset value: the worth of everything the strategy holds, divided by the shares outstanding. The word net is a finance convention for value after subtracting what a fund owes; this strategy borrows nothing, so its net asset value is simply its holdings. For running the book, the manager earns a fee.

By the end you will have watched an asset get approved, a strategy get built, someone deposit, the manager invest and rebalance, a fee accrue, and someone redeem, and you will know which instruction handler does each one. The program controls every dollar the whole time: the manager invests the deposits but can never move them to herself, a limit we will pin down precisely.

You have seen this shape on Solana, in protocols like Symmetry and Kamino. This is the teaching-sized version.

Two things genuinely change once the strategy is onchain:

- The rules are the deployed bytecode. Maria cannot freeze redemptions, the fee is fixed at creation and capped in code at ten percent, and there is no admin lever to pull.
- Entry and exit are permissionless and settle instantly. Anyone can deposit or redeem in a single transaction, priced live, with no minimum and no end-of-day cutoff.

We will hit each piece as it shows up.

## The accounts, and who can move what

NARRATION:

Custody is the whole game, so let us name the boxes before we move money.

First, the word vault, because it gets overloaded. By the common standard a vault holds a single asset: you put one kind of token in, you get shares out. A managed mix of several assets is not one vault; it lives in several vaults, one per asset, and is usually called a basket or a fund. Symmetry calls its multi-asset products baskets. We will keep it simple: a vault is one single-asset token account, and the strategy is the whole construct that owns them. So vault strategy reads literally, a strategy built from vaults.

The center of everything is the `Strategy` account, whose address is a PDA derived from the seeds `"strategy"` plus Maria's public key. A PDA is an address with no private key: it is found deliberately off the signing curve, so no key can sign for it and only the program can, by supplying the seeds. The strategy PDA is the authority over the USDC vault, every asset vault, and the share mint. Each vault is an associated token account owned by the strategy PDA and holds exactly one asset. The share mint's address is also a PDA, seeds `"share_mint"` plus the strategy address, so it is deterministic, one share mint per strategy, with the strategy PDA as its mint authority.

The asset set is not fixed. Each asset the strategy holds gets its own small account, an `AssetConfig`, whose address is a PDA seeded by the strategy and an index: zero, one, two, and so on. That indexing matters later: the assets are exactly the range zero up to the count, so any handler that values the whole strategy can re-derive every one and refuse to run if a single asset account is missing.

One account sits outside any single strategy: a `Registry`, a curated whitelist of assets that strategies are allowed to hold. We will meet its keeper first.

ON SCREEN:

```
Registry            [off curve - PDA, seeds: "registry" + authority]   owner = curator, not a manager
Strategy            [off curve - PDA, seeds: "strategy" + manager]
    authority over: vault_usdc, every asset vault, share_mint
share_mint          [off curve - PDA, seeds: "share_mint" + strategy]   authority = Strategy PDA
AssetConfig #i      [off curve - PDA, seeds: "asset" + strategy + index]  one per asset
vault_usdc / per-asset vaults  [off curve - ATAs, one asset each]       authority = Strategy PDA
```

## Victor approves the assets

NARRATION:

Meet Victor. Victor is not a fund manager; he runs the registry, the list of assets any strategy is allowed to hold. His motive is reputational: he is the gatekeeper who vets that an asset is real and has a trustworthy price feed. He calls `initialize_registry` once, then `whitelist_asset` for each approved token, and here is the important part, each whitelist entry binds the mint to its official Pyth price feed.

Why a separate person at all? Because this is the line that stops fraud. If a manager could add any token to her own strategy, she could mint a worthless token herself, list it, and value it at whatever she liked. And even with a real token, if she could choose its price feed she could point at one she controls. Victor's registry removes both moves: a manager can only ever pick from assets Victor approved, and the price feed comes from Victor's entry, never from the manager.

ON SCREEN:

```
ADDED - Registry            [off curve - PDA]   authority: Victor

ADDED - WhitelistEntry (TSLAx)  [seeds: "whitelist" + registry + TSLAx mint]
    mint: TSLAx   price_feed: <official Pyth TSLAx feed>
ADDED - WhitelistEntry (NVDAx)  [seeds: "whitelist" + registry + NVDAx mint]
    mint: NVDAx   price_feed: <official Pyth NVDAx feed>

TOKEN MOVEMENT: none - approvals only
Fee generated: none
```

## Maria opens the strategy

NARRATION:

Maria is our portfolio manager, and she wants to run the basket and earn the fee. She calls `initialize_strategy`, binding her strategy to Victor's registry. She sets two numbers and no assets yet: a fee of one hundred basis points, which is one percent a year, and a maximum slippage of one hundred basis points, which we will use when she trades. Both are capped in code, the fee at ten percent and the slippage tolerance at ten percent, and both are fixed here at creation with no setter to change them later.

The fee cap exists because the fee is paid by minting new shares to the manager; an uncapped fee would let a manager dilute depositors to nothing by configuration alone.

ON SCREEN:

```
ADDED - Strategy            [off curve - PDA]
    manager: Maria   registry: Victor's registry
    fee_bps: 100   max_slippage_bps: 100   total_shares: 0
    asset_count: 0   total_weight_bps: 0   last_fee_accrual_timestamp: now

ADDED - share_mint, vault_usdc   (empty)

TOKEN MOVEMENT: none - setup only
Fee generated: none
```

## Maria adds the two assets

NARRATION:

Now Maria builds the basket with `add_asset`, once per asset. Each call names a mint and a target weight, and the program checks that mint against Victor's registry, copies the official price feed from the whitelist entry, creates the asset's vault, and records it at the next index. TSLAx goes in first at index zero with a forty percent weight; NVDAx at index one with sixty percent. The weights are written in basis points and the program keeps their running sum at or below ten thousand, so four thousand plus six thousand is exactly full.

Two honest notes. The weight is a target Maria maintains by hand with invest and rebalance; the program records it but does not force an allocation on deposit. And if Maria names a token Victor never whitelisted, there is simply no whitelist entry to read, and the call fails.

ON SCREEN:

```
ADDED - AssetConfig #0       [off curve - PDA, seeds: "asset" + strategy + 0]
    mint: TSLAx   price_feed: <copied from registry>   weight_bps: 4000   vault: vault_tsla
ADDED - vault_tsla           (empty)

ADDED - AssetConfig #1       [off curve - PDA, seeds: "asset" + strategy + 1]
    mint: NVDAx   price_feed: <copied from registry>   weight_bps: 6000   vault: vault_nvda
ADDED - vault_nvda           (empty)

UPDATED - Strategy   asset_count: 0 -> 2   total_weight_bps: 0 -> 10000

TOKEN MOVEMENT: none - the vaults start empty
Fee generated: none
```

## Alice deposits 900 USDC

NARRATION:

Alice wants exposure to both stocks without buying and rebalancing them herself, so she calls `deposit` with 900 USDC. `deposit` is permissionless: any user can call it. This is buying into the strategy.

The handler prices her shares against net asset value. It walks the complete asset set, index zero then index one, reading each vault's balance and each Pyth price, and it will not proceed unless every asset's accounts are present, so nothing can be hidden from the valuation. The strategy is empty, so net asset value is zero, and the first deposit is defined as one to one. Alice gets 900 shares. Shares carry six decimals, so under the hood that is 900 million minor units, but think of it as 900 shares worth a dollar each.

Checks, effects, interactions: the handler raises `total_shares` first, then pulls her USDC into the USDC vault, then mints her the shares with the strategy PDA signing.

ON SCREEN:

```
UPDATED - Strategy        total_shares: 0 -> 900,000,000
UPDATED - vault_usdc      0 -> 900 USDC
UPDATED - Alice share ATA 0 -> 900 shares

TOKEN MOVEMENT:
    Alice USDC ATA -> vault_usdc        900 USDC   (deposit)
    share_mint     -> Alice share ATA   900 shares (minted, Strategy PDA signs)

Fee generated: none - deposits do not accrue fees
```

## Maria puts the cash to work

NARRATION:

Now Maria earns her title. She calls `invest` twice, manager-only. It hands the swap to the registered router, which for this example is a deterministic mock: at a fixed rate it mints the asset into the matching vault and takes the USDC. First, 360 dollars into TSLAx at 250 dollars a share, so the TSLAx vault receives 1.44 TSLAx. Then 540 dollars into NVDAx at 180 dollars a share, so the NVDAx vault receives exactly 3 NVDAx. That is the 40/60 split, by hand.

The slippage guard is the part worth watching. Maria does not get to hand in a minimum, the program computes one. It reads the asset's Pyth price, works out how much the swap should return, and refuses anything more than her one percent tolerance below that. A bad or manipulated quote reverts instead of quietly draining the vault. The strategy PDA signs the swap, because the USDC leaves a vault only it controls.

ON SCREEN:

```
UPDATED - vault_usdc        540 USDC -> 0 USDC      (across both invests)
UPDATED - vault_tsla        0 -> 1.44 TSLAx
UPDATED - vault_nvda        0 -> 3.0 NVDAx

TOKEN MOVEMENT (invest #1):
    vault_usdc -> router treasury   360 USDC
    router     -> vault_tsla        1.44 TSLAx   (router mints; 360 / 250)
    minimum out: computed from Pyth (>= 1.4256 TSLAx at 1% tolerance), not supplied by Maria
TOKEN MOVEMENT (invest #2):
    vault_usdc -> router treasury   540 USDC
    router     -> vault_nvda        3.0 NVDAx    (router mints; 540 / 180)

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

UPDATED - Strategy        total_shares: 900,000,000 -> 1,350,000,000
UPDATED - vault_usdc      0 -> 480 USDC
UPDATED - Bob share ATA   0 -> 450 shares

TOKEN MOVEMENT:
    Bob USDC ATA -> vault_usdc        480 USDC
    share_mint   -> Bob share ATA     450 shares   (480 x 900 / 960 = 450)

Fee generated: none
```

## Maria rebalances back toward target

NARRATION:

NVIDIA's run pushed the holdings away from 40/60, so Maria calls `rebalance`. One handler, two swaps, both signed by the strategy PDA: it sells one asset for USDC, then spends that USDC on the other. Each leg carries the same oracle-computed floor as invest, so neither can be filled at a bad price.

She sells 0.36 TSLAx, receiving 90 dollars, then buys 0.5 NVDAx with that same 90 dollars. The USDC vault nets to zero change across the two legs; the strategy just shifts weight from Tesla into NVIDIA.

ON SCREEN:

```
UPDATED - vault_tsla     1.44 TSLAx -> 1.08 TSLAx     (sold 0.36)
UPDATED - vault_nvda     3.0 NVDAx  -> 3.5 NVDAx       (bought 0.5)
UPDATED - vault_usdc     480 USDC -> 480 USDC          (+90 then -90)

TOKEN MOVEMENT:
    sell leg: vault_tsla -> router (burned) 0.36 TSLAx; router treasury -> vault_usdc 90 USDC
    buy  leg: vault_usdc -> router treasury 90 USDC; router -> vault_nvda 0.5 NVDAx
    both legs: minimum out computed from each asset's Pyth price

Net asset value: 480 + 1.08 x 250 + 3.5 x 200 = 480 + 270 + 700 = 1,450 USDC
Fee generated: none - rebalance moves assets, it does not charge a fee
```

## Maria collects her fee

NARRATION:

Maria calls `collect_fees`. This is a streaming management fee, and the mechanism is worth dwelling on, because it is the opposite of the offchain world. A traditional fund deducts its expense ratio from fund assets, selling holdings to pay the manager in cash, which lowers net asset value. This program touches no vault at all. It mints new shares to the manager, proportional to time elapsed and the fee rate. Over a full year at one percent, that is one percent of the share supply, 13.5 shares, minted to Maria.

Same economics, different lever. New shares with no new assets behind them make every existing share a slightly thinner slice, so the dilution, spread across all holders, is how Alice and Bob pay the fee. Minting fee shares is the common onchain pattern: Yearn and Lido both charge their fees this way rather than skimming assets. And there is no performance fee here, only this management fee on assets under management, bounded by the cap from creation.

ON SCREEN:

```
elapsed: 1 year (illustrative)
fee_shares = total_shares x fee_bps x elapsed / (10,000 x seconds_per_year)
           = 1,350,000,000 x 100 x 1yr / (10,000 x 1yr) = 13,500,000  (13.5 shares)

UPDATED - Strategy        total_shares: 1,350,000,000 -> 1,363,500,000
                          last_fee_accrual_timestamp: updated
UPDATED - Maria share ATA 0 -> 13.5 shares

TOKEN MOVEMENT:
    share_mint -> Maria share ATA   13.5 shares (minted, Strategy PDA signs)
Fee generated: 13.5 shares to the manager; all other holders diluted ~1%
```

## Alice withdraws

NARRATION:

Alice calls `withdraw` and burns all 900 of her shares. Here is the part people miss: withdrawal is in kind and proportional. She does not get cash. She gets her exact fraction of every balance the strategy holds, across the USDC vault and both asset vaults. It is the same move an ETF makes when it redeems in kind, handing back the underlying holdings instead of cash. Just like deposit, the handler insists on seeing every asset, so her slice is computed against the whole strategy.

Her fraction is 900 shares out of the 1,363.5 that now exist. The handler floors each amount in the protocol's favor, so any rounding dust stays with the remaining holders.

ON SCREEN:

```
Alice fraction = 900,000,000 / 1,363,500,000

amount_usdc = 480,000,000 x 900,000,000 / 1,363,500,000 = 316,831,683  (316.83 USDC, floor)
amount_tsla =   1,080,000 x 900,000,000 / 1,363,500,000 =     712,871  (0.712871 TSLAx, floor)
amount_nvda =   3,500,000 x 900,000,000 / 1,363,500,000 =   2,310,231  (2.310231 NVDAx, floor)

UPDATED - Strategy        total_shares: 1,363,500,000 -> 463,500,000
UPDATED - Alice share ATA 900 shares -> 0   (burned)

TOKEN MOVEMENT:
    share_mint burns 900 shares from Alice
    vault_usdc -> Alice   316.83 USDC
    vault_tsla -> Alice   0.712871 TSLAx
    vault_nvda -> Alice   2.310231 NVDAx

Alice payout value @ 250 / 200 = 316.83 + 0.712871 x 250 + 2.310231 x 200 = about 957.10 USDC
Fee generated: none - withdrawals do not accrue fees
```

## What restricts Maria

NARRATION:

We promised to pin down what the manager can and cannot do, so here it is, now that you have seen each power in action. Maria can add assets, invest, rebalance, and collect her fee. Every one is fenced:

- She can only add assets Victor whitelisted, and each asset's price feed is copied from Victor's registry, not chosen by her.
- Her swaps go only through the one router registered at creation, and every swap's floor is computed from the oracle, not supplied by her.
- Her fee is fixed at creation, capped at ten percent, and paid only in newly minted shares.
- No instruction anywhere sends a vault's tokens to the manager. She directs the assets; she cannot withdraw them.

What is left to trust, honestly, is the router and registry the strategy was pointed at. With an honest router the worst a careless manager can do is churn and pay market slippage, which hurts depositors but does not enrich her. She cannot abscond with the principal.

## Reconcile, and where everyone ended up

NARRATION:

Let us check the books. USDC into the USDC vault was 900 from Alice plus 480 from Bob, 1,380 total. The invests sent 900 to the router; rebalance was a wash. That leaves 480 in the USDC vault, and after Alice's withdrawal, 163.17 remains. Tokens in equal tokens out.

So: Alice came in with 900 dollars, rode NVIDIA up, paid her share of a one percent fee through dilution, and left with about 957 dollars of assets, in kind. The strategy passes returns through in both directions: had NVIDIA fallen instead of risen, the same arithmetic would have redeemed Alice for less than her 900 dollars. That market risk is hers, and the program neither cushions it nor hides it. Bob bought in fairly at the higher share price and still holds 450 shares worth roughly 478 dollars. Maria earned 13.5 shares, about 14 dollars, for running the book. Victor's only role was the guest list. The strategy held custody from the first deposit to the last withdrawal, the manager never touched the vaults with her own key, and the fee she could charge was capped in the bytecode.

## Two honest footnotes

NARRATION:

First, the swap router here is a deterministic test stand-in. It mints and burns at a fixed rate with no spread, and its rate matches the Pyth price, which keeps the math clean for teaching. A real deployment would call out to a live venue, and the strategy would only trust the one router address it registered at creation. That registration is checked on every invest and rebalance.

Second, two limits worth naming. The weights are a target Maria maintains, not an allocation the program enforces on each deposit; enforcing them in-program is a reasonable thing to add. And the basket is add-only in this version, you can grow it but not prune it, because the assets are addressed by a contiguous index and removing one would leave a gap. Both are clean extensions, not assumptions baked in.

That is the whole lifecycle: approve, open, add assets, deposit, invest, price in new depositors fairly, rebalance, charge a bounded streaming fee, and redeem in kind. Thanks for watching.
