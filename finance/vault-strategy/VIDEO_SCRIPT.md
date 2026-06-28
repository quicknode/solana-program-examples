# Vault Strategy: a five-minute walkthrough

A video script for the `vault-strategy` example. Target runtime is roughly seven minutes at a normal speaking pace. Narration lines are what the presenter says; the indented blocks are what is on screen as a running ledger of onchain state.

Prices for TSLAx and NVDAx in this script are illustrative and match the rates the example's tests configure. They are not live quotes. USDC, TSLAx (Tesla stock) and NVDAx (NVIDIA stock) are real assets; the swap behind the scenes is a deterministic test stand-in, which we will be honest about when we reach it.

## Cold open: where everyone ends up

NARRATION:

Here is the ending first, then we will earn it.

Maria runs a managed basket: forty percent Tesla stock, sixty percent NVIDIA stock, with a one percent annual management fee. Alice puts in 900 dollars because she wants that basket without having to buy and rebalance two stocks herself. NVIDIA rises. Bob shows up later and pays the new, higher price per share, not a discount. Maria collects her fee. Alice cashes out and walks away with about 957 dollars, paid not in pure cash but as her exact slice of everything the vault holds.

Nobody trusted anybody to hold cash off to the side. Every dollar lived in a program-owned vault the entire time. Let us watch it happen, one instruction handler at a time.

## What it is, in finance you already know

NARRATION:

Strip the jargon and this is an actively managed fund. In traditional finance you would call it a mutual fund, or an actively managed ETF: you hand cash to a portfolio manager, you receive units, the manager buys a basket and rebalances it over time, the fund prices your units at net asset value, and the manager takes an expense ratio every year for running it. When you leave an ETF, it can even pay you in kind, handing back the underlying shares instead of cash.

Every one of those pieces has a line in this program. Units are share tokens. Net asset value is computed live from a Pyth oracle. The expense ratio is the management fee. In-kind redemption is exactly how `withdraw` works. The portfolio manager is Maria.

You have seen this shape on Solana, too. Drift Vaults let a manager trade depositors' pooled funds for a fee. Symmetry runs weighted token baskets that rebalance. Kamino issues vault shares priced at net asset value. This example is the teaching-sized version of that family.

So what actually changes when the fund is onchain, past the buzzwords? Four things that matter:

- The rules are the deployed bytecode, not a prospectus you trust a custodian to honor. Maria cannot freeze redemptions or quietly raise the fee. The fee is even capped in code at ten percent.
- Entry and exit are permissionless and settle instantly. No minimum, no transfer agent, no end-of-day cutoff. Alice deposits and redeems in single transactions, and so can anyone.
- The price comes from an oracle, not an end-of-day accountant. That is a real dependency, not a free lunch: a stale or wrong Pyth price would misprice every deposit, which is why the program refuses any price older than sixty seconds.
- You custody your own units. Your shares live in your wallet, not on a broker's ledger, and an onchain bug is final in a way a fund's back-office error is not.

Keep that mapping in your head. We will hit each piece as it shows up.

## The accounts, and who can move what

NARRATION:

Custody is the whole game, so let us name the boxes before we move money.

The center of everything is the `Strategy` account, a PDA derived from the seeds `"strategy"` plus Maria's public key. That PDA is the authority over four things: a USDC vault, a TSLAx vault, an NVDAx vault, and the share mint. The three vaults are associated token accounts owned by the strategy PDA. The share mint is its own PDA, seeds `"share_mint"` plus the strategy address, and the strategy PDA is its mint authority.

What that buys us: only the strategy PDA can sign to move tokens out of those vaults or to mint shares. Maria is the manager, but Maria cannot reach into the vaults with her own keypair. Her powers are exactly three instruction handlers, and the program caps the worst one. We will see each.

ON SCREEN:

```
Strategy            [off curve - PDA, seeds: "strategy" + manager]
    authority over: vault_usdc, vault_asset_a, vault_asset_b, share_mint
share_mint          [off curve - PDA, seeds: "share_mint" + strategy]   authority = Strategy PDA
vault_usdc / _a / _b [off curve - ATAs]                                  authority = Strategy PDA
```

## Step 1: Maria opens the strategy

NARRATION:

Maria is our portfolio manager, and she wants to run the basket and earn the fee. She calls `initialize_strategy`. She sets the two weights, which must sum to ten thousand basis points, a fee of one hundred basis points, which is one percent a year, and she registers the swap router and the two Pyth price feeds the vault will trust.

One honest detail up front: those weights are a target Maria maintains by hand. The program records them, but no handler reads them to force an allocation. Deposits arrive as plain USDC and sit idle until Maria chooses to invest. The forty-sixty split is a promise Maria keeps with `invest` and `rebalance`, not a rule the bytecode enforces on each deposit.

The fee, though, is bounded. `initialize_strategy` rejects any fee above `MAX_FEE_BPS`, ten percent, because the fee is paid by minting new shares, and an uncapped fee would let a manager dilute depositors to nothing by configuration alone.

ON SCREEN:

```
ADDED - Strategy            [off curve - PDA]
    manager: Maria   weight_bps_a: 4000 (TSLAx)   weight_bps_b: 6000 (NVDAx)
    fee_bps: 100   total_shares: 0   last_fee_accrual_timestamp: now

ADDED - share_mint, vault_usdc, vault_asset_a, vault_asset_b   (all empty)

TOKEN MOVEMENT: none - setup only
Fee generated: none
```

## Step 2: Alice deposits 900 USDC

NARRATION:

Alice wants the Tesla-plus-NVIDIA basket without buying and rebalancing two stocks herself, so she calls `deposit` with 900 USDC. `deposit` is permissionless: any user can call it, not just the manager. This is buying into the fund.

The handler prices her shares against net asset value, the total worth of the vault. It reads both Pyth feeds straight from the raw account bytes at fixed offsets, checks each price is positive and no more than sixty seconds stale, and computes net asset value as the USDC balance plus each asset balance times its price. The vault is empty, so net asset value is zero, and the first deposit is defined as one to one. Alice gets 900 shares. Shares carry six decimals, so under the hood that is 900 million minor units, but think of it as 900 shares worth a dollar each.

Checks, effects, interactions: the handler raises `total_shares` first, then pulls her USDC into the vault, then mints her the shares with the strategy PDA signing.

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

## Steps 3 and 4: Maria puts the cash to work

NARRATION:

Now Maria earns her title. She calls `invest` twice. `invest` is manager-only; the account constraints require her signature via `has_one = manager`.

`invest` does not hold a price of its own. It makes a cross-program call into the swap router, which for this example is a deterministic mock: at a fixed rate it mints the asset to the vault and takes the USDC. First, 360 dollars into TSLAx at 250 dollars a share, so the vault receives 1.44 TSLAx. Then 540 dollars into NVDAx at 180 dollars a share, so the vault receives exactly 3 NVDAx. That is the forty-sixty split, by hand.

The strategy PDA signs both swaps, because the USDC is leaving a vault that only the PDA controls.

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

## Step 5 and 6: NVIDIA rises, and Bob pays the new price

NARRATION:

Time passes. NVDAx climbs from 180 to 200. Nothing onchain changes from a price move by itself; the vault simply holds 3 NVDAx that are now worth more. Net asset value rises to 960 dollars while the share count is still 900. Each share is now worth about a dollar and seven cents.

Bob wants the same basket Alice does, but he arrives now, after the gain, so he is the one who shows us how units are priced. He calls `deposit` with 480 dollars. This is the moment the share math matters, and it is the same rule a mutual fund uses: you buy units at today's net asset value. Bob does not get 480 shares. The handler computes shares as his deposit times total shares divided by net asset value: 480 times 900 divided by 960, which is exactly 450 shares. He pays the current price, so he does not dilute Alice's gain, and Alice's earlier deposit does not subsidize his.

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

## Step 7: Maria rebalances back toward target

NARRATION:

NVIDIA's run pushed the basket away from forty-sixty, so Maria calls `rebalance`. One handler, two swaps, both signed by the strategy PDA: it sells one asset for USDC, then spends that USDC on the other.

She sells 0.36 TSLAx, receiving 90 dollars, then buys 0.5 NVDAx with that same 90 dollars. Both legs name a minimum out, so a bad rate would revert rather than silently lose value. USDC nets to zero change across the two legs; the vault just shifts weight from Tesla into NVIDIA.

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

## Step 8: Maria collects her fee

NARRATION:

Maria calls `collect_fees`. This is a streaming management fee, and the mechanism is the point: the program does not skim tokens from a vault. It mints new shares to the manager, proportional to time elapsed and the fee rate. Over a full year at one percent, that is one percent of the share supply, 13.5 shares, minted to Maria.

New shares with no new assets behind them means every existing share is now a slightly thinner slice. That dilution, spread across all holders, is how Alice and Bob actually pay the fee. This is the expense ratio of a mutual fund, charged the Solana way: by minting the manager new units rather than by selling fund assets to cut her a check. It is honest to say so out loud: there is no separate performance fee here, only this management fee on assets under management, and it is the cap from step one that stops it from ever becoming a drain.

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

## Step 9: Alice withdraws

NARRATION:

Alice calls `withdraw` and burns all 900 of her shares. Here is the part people miss: withdrawal is in kind and proportional. She does not get cash. She gets her exact fraction of every balance the vault holds, USDC and TSLAx and NVDAx alike. This is an ETF in-kind redemption: just as an authorized participant hands back fund units and receives the underlying shares, Alice's burn returns her slice of the actual holdings, not a cash settlement.

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

Let us check the books. USDC into the vault was 900 from Alice plus 480 from Bob, 1,380 total. The invests sent 900 to the router; rebalance was a wash. That leaves 480 in the vault, and after Alice's withdrawal, 163.17 remains. Tokens in equal tokens out.

So: Alice came in with 900 dollars, rode NVIDIA up, paid her share of a one percent fee through dilution, and left with about 957 dollars worth of basket, in kind. Bob bought in fairly at the higher share price and still holds 450 shares worth roughly 478 dollars. Maria earned 13.5 shares, about 14 dollars, for running the book. The vault held custody from the first deposit to the last withdrawal, the manager never touched the vaults with her own key, and the fee she could charge was capped in the bytecode.

## Two honest footnotes

NARRATION:

First, the swap router here is a deterministic test stand-in. It mints and burns at a fixed rate with no spread, and its rate matches the Pyth price, which keeps the math clean for teaching. A real deployment would call out to a live venue and the strategy would only trust the one router address it registered at initialization. That registration is checked on every `invest` and `rebalance`.

Second, the forty-sixty weights are a target Maria maintains, not an allocation the program enforces per deposit. If you want the vault to auto-allocate on deposit, that is a feature to add, not something to assume is already there.

That is the whole lifecycle: open, deposit, invest, price in new depositors fairly, rebalance, charge a bounded streaming fee, and redeem in kind. Thanks for watching.
