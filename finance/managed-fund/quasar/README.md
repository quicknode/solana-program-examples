# Solana Managed Fund (Quasar)

A manager-run investment fund on Solana, written with Quasar. A manager assembles a basket of curator-approved
assets at target weights; anyone can deposit USDC and receive shares priced at
the fund's net asset value, and each deposit is immediately deployed into the
basket by swapping USDC into every asset at its weight. Withdrawals burn shares
and redeem a proportional slice of every vault, paid in kind. This is the shape
of a mutual fund. Some platforms call the product a vault strategy, and this
example was called `vault-strategy` until it was renamed `managed-fund`.

This is a [Quasar](https://github.com/blueshift-gg/quasar) port of the Anchor
example in [`../anchor`](../anchor). It contains two programs, each its own
Quasar project:

- `managed-fund/` - the fund itself (program ID
  `VLT5W7bqhRN4nCdRpXm8UfHRxZd9EuZGqiSAkGHQfGh`).
- `mock-swap-router/` - a stand-in constant-rate swap venue the fund trades
  through (program ID `SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm`). Its
  `RouterConfig` account (`["router_config"]`) is both its state and its
  signer: it owns the USDC treasury, is the mint authority of every asset it
  mints, and signs the router's token CPIs with its own seeds, as the Fund
  PDA does for the vaults.

Both share the same program IDs as the Anchor build. The mock router mints an
asset against USDC at an admin-set fixed rate, standing in for a real AMM or
aggregator so the example is self-contained.

## How it works

A separate registry authority curates a registry of assets, binding each
approved mint to its official price feed. This authority is deliberately not the
fund manager: it vets which real assets and feeds are safe, and the manager
only chooses among them, so a manager can never list a token they mint
themselves or pair a real mint with a feed they control.

- `initialize_registry` records the curator; `approve_asset` approves a mint
  and records its price feed. The registry holds no list: an asset is approved
  exactly when its `ApprovedAsset` account exists.
- A manager opens a basket with `initialize_fund` (choosing a management fee
  and a slippage tolerance), then adds approved assets with `add_asset`, each
  at a target weight in basis points. The weights must sum to 100% before the
  fund accepts deposits, so every deposit is fully invested. `set_weight`
  retunes a weight or retires an asset by setting it to zero.
- `deposit` prices the incoming USDC against the fund's net asset value (the
  USDC vault plus every asset vault valued at its oracle price), mints shares for
  that fraction of the fund, and deploys the deposit across the basket by
  swapping a weight-sized slice into each asset through the router. The first
  deposit into an empty fund mints shares one-to-one. A deposit so small that a
  swap returns none of its asset is refused (`DepositTooSmall`).
- The valuation and every payout use the holdings the fund has recorded
  (`usdc_holdings`, and `asset_holdings` stored as little-endian u64s because
  zero-copy accounts hold byte arrays only), not the vaults' token balances.
  Tokens transferred straight into a vault are outside the fund, so a donation
  cannot inflate the share price, the first-depositor attack.
- `withdraw` burns shares and pays out a proportional slice of the USDC vault and
  every asset vault, in kind.
- `rebalance(sell_index, buy_index)` sells one asset for USDC and buys another
  with it, restoring the target weights as prices drift. Anyone may call it,
  because the program sizes the trade: it values every asset (the same five
  accounts per asset as `deposit`), requires the asset sold to sit above its
  target by at least the fund's `rebalance_threshold_bps` of the fund's value
  (`DriftBelowThreshold`; a retired asset may always be sold) and the asset
  bought to sit below its own (`NotUnderweight`), and trades the smaller of the
  two gaps. A fund at its targets has no trade to make, so nobody can churn it.
  The threshold is set at creation between 100 and 2,000 basis points
  (`RebalanceThresholdOutOfRange`) and never changes. Both legs are floored to
  the oracle price so a bad swap route reverts, and the buy leg spends only what
  the sale brought in.
- `collect_fees` accrues the time-based management fee by minting fresh shares to
  the manager, diluting holders at the configured annual rate.

Every swap and rebalance leg is bounded by the registered price feed: the
program computes the oracle-implied output and rejects any swap that falls short
by more than the fund's slippage tolerance. Values scale by each asset's mint
decimals, recorded when the asset is added, and by the exponent each Pyth feed
reports (−8 for crypto USD feeds, −5 for US equities), so a basket can mix
assets of any precision.

## Accounts and PDAs

- **Registry** `["registry", authority]` and **ApprovedAsset**
  `["approved_asset", registry, mint]` - the curator record and, one account per
  approved mint, the asset set with each mint's price feed.
- **Fund** `["fund", index]` - one basket, addressed by a counter. Holds
  the manager, registry, share mint, USDC mint, router, fee, slippage, total
  shares, and running weight sum. The Fund PDA is the authority of the share
  mint and every vault, so the program signs all mints and payouts.
- **AssetConfig** `["asset", fund, index]` - one basket asset (mint, copied
  price feed, vault, target weight). The set is the contiguous range
  `0..asset_count`, so a valuation can re-derive every asset and refuse to
  proceed if one is missing.
- **Share mint** `["share_mint", fund]`, **USDC vault**
  `["usdc_vault", fund]`, and per-asset vaults `["asset_vault", fund, index]`.

`deposit` and `withdraw` reference every asset at once, so the client passes the
per-asset accounts as remaining accounts (five per asset for deposit, four for
withdraw), in index order.

## Safety and custody

- Deposited USDC and every asset sit in program-owned vaults whose authority is
  the Fund PDA; only the deployed program can move them, and it does so only
  along deposit, withdraw, and rebalance. There is no manager path to withdraw
  holdings, nor to choose a trade: rebalancing is sized by the program.
- The share supply is updated before any mint or burn (checks-effects-
  interactions), and value computations use u128 intermediates with checked
  arithmetic, flooring in the fund's favour.
- The management fee is capped (10% per year) and the slippage tolerance is
  capped (10%), so neither can be configured to drain the fund.
- Price feeds are validated against the address recorded on the asset config and
  rejected if stale or non-positive, or if posted at or before the last cluster
  restart. Under Alpenglow the Clock's timestamp trails real time after a halt,
  so a pre-halt price can still pass the 60-second check; `load_price` also
  requires the update's `posted_slot` to be after the `LastRestartSlot`
  sysvar's slot (`PricePredatesRestart`). A price whose confidence interval is
  wider than 1% of the price (`MAX_CONFIDENCE_BPS`) is rejected too
  (`OracleConfidenceTooWide`). `withdraw` reads no price, so investors can
  always leave in kind.

## What the Quasar port does differently

The valuation, fee, and swap-floor math are identical to the Anchor build. The
differences follow from Quasar's model:

- **The cross-program swap is built by hand.** Anchor generates a typed CPI
  client (`mock_swap_router::cpi::*`); Quasar has no such generation, so each
  swap is a `CpiDynamic` call whose account list and instruction data the program
  constructs directly. The router's instruction wire format (a one-byte
  discriminator plus two little-endian u64s) is encoded inline.
- **The oracle and foreign token accounts are read by raw byte offset** through
  `UncheckedAccount` views, the same field layout the Anchor build parses.
- **Pool vaults are program-derived token accounts** rather than associated
  token accounts, matching the other Quasar finance examples.
- **The share mint carries no freeze authority** (it is never used); the Anchor
  build sets it to the fund PDA.
- **A hand-declared `LastRestartSlot` sysvar.** quasar-lang ships only the
  Clock and Rent sysvars, so `src/last_restart.rs` declares the 8-byte layout
  itself and reads it with the same `sol_get_sysvar` syscall. `load_price` uses
  it to reject prices posted before a cluster restart.

## Building and testing

Requires the [Solana toolchain](https://docs.anza.xyz/cli/install) and the
[Quasar CLI](https://github.com/blueshift-gg/quasar). Build both programs before
testing, because the deposit test loads the router's compiled `.so`:

```sh
cargo install --git https://github.com/blueshift-gg/quasar quasar-cli --locked
(cd mock-swap-router && quasar build)
(cd managed-fund && quasar build)
(cd mock-swap-router && cargo test)
(cd managed-fund && cargo test)
```

The router suite (`mock-swap-router/src/tests.rs`) exercises initialize, set-rate,
and a USDC-for-asset swap. The fund suite (`managed-fund/src/tests.rs`) drives
the manager setup (registry, approve asset, fund, add asset) and a two-program
deposit that deploys USDC into the basket through the router CPI, asserting share
minting, vault balances, and treasury flow. A second deposit test shows a price
posted before a cluster restart is rejected until Pyth posts again. The
rebalance tests sign as a stranger and check that a fund at its targets, within
its threshold, or just rebalanced cannot be traded
(`test_rebalance_cannot_churn` and its neighbors), and
`test_valuation_scales_by_decimals_and_exponent` runs the story with an
eight-decimal asset on an exponent −5 feed. `test_wide_confidence_price_rejected`
widens NVDAx's confidence interval to 2% of its price and checks that deposit
and rebalance are refused while withdraw still pays out, and that a band of
exactly 1% is accepted.

## Extending

- Multiple assets per basket (up to the 16-asset cap) with a v0 transaction plus
  an Address Lookup Table for the larger account list.
- A real AMM or aggregator in place of the mock router.
- Deposit and withdraw fees in addition to the time-based management fee.
