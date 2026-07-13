# Solana Vault Strategy (Quasar)

A tokenized multi-asset vault on Solana, written with Quasar. A manager assembles a basket of whitelisted
assets at target weights; anyone can deposit USDC and receive shares priced at
the vault's net asset value, and each deposit is immediately deployed into the
basket by swapping USDC into every asset at its weight. Withdrawals burn shares
and redeem a proportional slice of every vault, paid in kind. This is the shape
of an onchain index fund or a managed ETF.

This is a [Quasar](https://github.com/blueshift-gg/quasar) port of the Anchor
example in [`../anchor`](../anchor). It contains two programs, each its own
Quasar project:

- `vault-strategy/` - the vault itself (program ID
  `VLT5W7bqhRN4nCdRpXm8UfHRxZd9EuZGqiSAkGHQfGh`).
- `mock-swap-router/` - a stand-in constant-rate swap venue the vault trades
  through (program ID `SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm`).

Both share the same program IDs as the Anchor build. The mock router mints an
asset against USDC at an admin-set fixed rate, standing in for a real AMM or
aggregator so the example is self-contained.

## How it works

A separate protocol authority curates a registry of assets, binding each
approved mint to its official price feed. This authority is deliberately not the
strategy manager: it vets which real assets and feeds are safe, and the manager
only chooses among them, so a manager can never list a token they mint
themselves or pair a real mint with a feed they control.

- `initialize_registry` opens the registry; `whitelist_asset` approves a mint
  and records its price feed.
- A manager opens a basket with `initialize_strategy` (choosing a management fee
  and a slippage tolerance), then adds whitelisted assets with `add_asset`, each
  at a target weight in basis points. The weights must sum to 100% before the
  vault accepts deposits, so every deposit is fully invested. `set_weight`
  retunes a weight or retires an asset by setting it to zero.
- `deposit` prices the incoming USDC against the vault's net asset value (the
  USDC vault plus every asset vault valued at its oracle price), mints shares for
  that fraction of the vault, and deploys the deposit across the basket by
  swapping a weight-sized slice into each asset through the router. The first
  deposit into an empty vault mints shares one-to-one.
- `withdraw` burns shares and pays out a proportional slice of the USDC vault and
  every asset vault, in kind.
- `rebalance` lets the manager sell one asset for USDC and buy another with it,
  keeping holdings near their targets as prices drift. Both legs are floored to
  the oracle price so a bad swap route reverts.
- `collect_fees` accrues the time-based management fee by minting fresh shares to
  the manager, diluting holders at the configured annual rate.

Every swap and rebalance leg is bounded by the registered price feed: the
program computes the oracle-implied output and rejects any swap that falls short
by more than the strategy's slippage tolerance.

## Accounts and PDAs

- **Registry** `["registry", authority]` and **WhitelistEntry**
  `["whitelist", registry, mint]` - the curated asset set and each approved
  mint's price feed.
- **Strategy** `["strategy", index]` - one basket, addressed by a counter. Holds
  the manager, registry, share mint, USDC mint, router, fee, slippage, total
  shares, and running weight sum. The Strategy PDA is the authority of the share
  mint and every vault, so the program signs all mints and payouts.
- **AssetConfig** `["asset", strategy, index]` - one basket asset (mint, copied
  price feed, vault, target weight). The set is the contiguous range
  `0..asset_count`, so a valuation can re-derive every asset and refuse to
  proceed if one is missing.
- **Share mint** `["share_mint", strategy]`, **USDC vault**
  `["usdc_vault", strategy]`, and per-asset vaults `["asset_vault", strategy, index]`.

`deposit` and `withdraw` reference every asset at once, so the client passes the
per-asset accounts as remaining accounts (five per asset for deposit, four for
withdraw), in index order.

## Safety and custody

- Deposited USDC and every asset sit in program-owned vaults whose authority is
  the Strategy PDA; only the deployed program can move them, and it does so only
  along deposit, withdraw, and rebalance. There is no manager path to withdraw
  holdings, only to trade them within the basket or collect the configured fee.
- The share supply is updated before any mint or burn (checks-effects-
  interactions), and value computations use u128 intermediates with checked
  arithmetic, flooring in the vault's favour.
- The management fee is capped (10% per year) and the slippage tolerance is
  capped (10%), so neither can be configured to drain the vault.
- Price feeds are validated against the address recorded on the asset config and
  rejected if stale or non-positive.

## What the Quasar port does differently

The valuation, fee, and swap-floor math are identical to the Anchor build. The
differences follow from Quasar's model:

- **The cross-program swap is built by hand.** Anchor generates a typed CPI
  client (`mock_swap_router::cpi::*`); Quasar has no such generation, so each
  swap is a `CpiDynamic` call whose account list and instruction data the vault
  constructs directly. The router's instruction wire format (a one-byte
  discriminator plus two little-endian u64s) is encoded inline.
- **The oracle and foreign token accounts are read by raw byte offset** through
  `UncheckedAccount` views, the same field layout the Anchor build parses.
- **Pool vaults are program-derived token accounts** rather than associated
  token accounts, matching the other Quasar finance examples.
- **The share mint carries no freeze authority** (it is never used); the Anchor
  build sets it to the strategy PDA.

## Building and testing

Requires the [Solana toolchain](https://docs.anza.xyz/cli/install) and the
[Quasar CLI](https://github.com/blueshift-gg/quasar). Build both programs before
testing, because the deposit test loads the router's compiled `.so`:

```sh
cargo install --git https://github.com/blueshift-gg/quasar quasar-cli --locked
(cd mock-swap-router && quasar build)
(cd vault-strategy && quasar build)
(cd mock-swap-router && cargo test)
(cd vault-strategy && cargo test)
```

The router suite (`mock-swap-router/src/tests.rs`) exercises initialize, set-rate,
and a USDC-for-asset swap. The vault suite (`vault-strategy/src/tests.rs`) drives
the manager setup (registry, whitelist, strategy, add asset) and a two-program
deposit that deploys USDC into the basket through the router CPI, asserting share
minting, vault balances, and treasury flow.

## Extending

- Multiple assets per basket (up to the 16-asset cap) with a v0 transaction plus
  an Address Lookup Table for the larger account list.
- A real AMM or aggregator in place of the mock router.
- Deposit and withdraw fees in addition to the time-based management fee.
- Rebalance automation driven by weight drift beyond a threshold.
