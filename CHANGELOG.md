# Changelog

All notable changes to this repository are documented here.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [2026-10-08] - Anchor v1 tests send v1 transactions

Solana's v1 transaction format (SIMD-0385) activated on mainnet beta at epoch 1035
(15 September 2026). It raises the transaction size limit from 1,232 to 4,096 bytes
and moves the compute budget out of ComputeBudget instructions and into the message.
Programs need no change for it; the client that builds the transaction does.

### Changed

- Every Anchor v1 example's tests send v1 transactions. Each test directory gains a
  `transaction_v1` module (identical in all 57 examples) whose
  `send_transaction_from_instructions` takes the same arguments and returns the same
  error type as `solana-kite`'s, so each test file changes one import and no call
  site. Kite's version builds a legacy transaction. The six tests that build a
  transaction by hand (to read logs, measure compute units or return the metadata)
  use the module's `v1_transaction` instead.
- A v1 transaction's unset limits are zero, not the legacy defaults, so the module
  asks for exactly what a legacy transaction gets without ComputeBudget instructions:
  200,000 compute units per instruction up to 1,400,000, and 64 MiB of loaded
  account data. The programs under test see the budget they always did.
- The test manifests name `solana-message` 4.2.4 and `solana-transaction` 4.1.5,
  where the v1 types live. The three compression examples, which did not use kite,
  add it for its error type, so the module is the same file everywhere.

### Added

- `basics/transaction-v1/anchor-v1`: a program that stores a 3,000 byte document in
  one instruction, which only fits in a v1 transaction. Its tests measure the
  transaction against both size limits, set each config field themselves, and show
  that an unset field means zero rather than the default.

### Note

- Kite's token helpers (`create_token_mint`, `create_associated_token_account`,
  `mint_tokens_to_token_account` and the token extension ones) send their setup
  transactions through kite's own legacy builder, and still do. They switch when
  kite's `send_transaction_from_instructions` builds v1; the module can then be
  deleted and the imports pointed back at kite.
- Anchor v2 and Quasar tests still send legacy transactions: `anchor-v2-testing`
  pins LiteSVM 0.13.1 and `quasar-svm` is on a `solana-message` without the `v1`
  module. Both predate v1.

## [2026-10-04] - Escrow: the taker signs the terms

### Fixed

- `finance/escrow` (Anchor v2, Anchor v1, Quasar, native) let a maker switch
  an offer under a taker. The offer's address is its maker and `id`, so the
  maker could cancel and re-make the same `id` at worse terms while a
  taker's `take_offer` was in flight, and the transaction would trade at the
  new terms. `take_offer` now takes `minimum_token_a_out` and
  `maximum_token_b_in`, and refuses the take with the new `OfferTermsChanged`
  error before any token moves if the vault holds less token A or the offer
  wants more token B. Tested by `test_take_offer_rejects_switched_offer` and
  `test_take_offer_rejects_switched_offer_wanting_more_token_b` in each copy;
  the Kani model gains `proof_take_offer_honors_taker_terms`.

## [2026-10-03] - Managed Fund rejects wide-confidence prices

### Fixed

- `finance/managed-fund` (Anchor v2, Anchor v1, Quasar) ignored the
  confidence interval on its Pyth prices. `load_price` now rejects a price
  whose interval exceeds 1% of the price (`MAX_CONFIDENCE_BPS`, new
  `OracleConfidenceTooWide` error), so deposit and rebalance are refused while
  publishers disagree; withdraw reads no price and still pays out in kind.
  Tested by `test_wide_confidence_price_rejected` in each copy. The web apps'
  IDLs gain the error.

## [2026-10-03] - Fundraiser: `close_contributor` is `close_contribution`

### Changed

- `finance/fundraiser` (Anchor v2, Anchor v1, Quasar) renames the account that
  records one contributor's contributions from `Contributor` to
  `Contribution`, and its closing handler from `close_contributor` to
  `close_contribution`: the handler closes that account, not the contributor.
  The PDA seed prefix becomes `"contribution"`, the Fundraiser's
  `open_contributor_accounts` becomes `open_contributions`, and the
  `ContributorAccountsOpen` error becomes `ContributionsOpen`. No behavior
  changes.

## [2026-10-03] - Order book tests cover the costliest cancel

`cancel_order` finds an order's place in the tree by walking its side from the
best price down, so its cost grows with the number of resting orders. Both
Anchor copies gain `full_side_cancel_of_the_last_scanned_order_fits_the_default_budget`,
which fills the bid side to its 512 orders, cancels the bid the walk reaches
last, and checks it stays inside the default 200,000-unit instruction budget.
It measures 50,078 compute units in the Anchor v2 copy and 57,383 in the v1
copy. No program behavior changes.

## [2026-10-02] - Token Swap (Quasar): reserves live at PDAs of the pool

### Changed

- `finance/token-swap/quasar` creates `pool_a` and `pool_b` at PDAs of the
  pool (`[b"pool_a", pool_config]`, `[b"pool_b", pool_config]`) instead of at
  addresses the client chose, as the order book's Quasar version does for its
  vaults. Clients no longer pass the reserves to `initialize_pool`.

## [2026-10-02] - Token Swap (Quasar): reserves are bound to the pool

### Fixed

- `finance/token-swap/quasar` accepted any token accounts as `pool_a` and
  `pool_b` in `deposit_liquidity`, `withdraw_liquidity`, `swap_tokens` and
  `claim_admin_fees`, so a trader could price a swap from a token account of
  their own and drain the real reserve on the other side. `PoolConfig` now
  records both reserves and every handler checks them (`InvalidPoolVault`).
  The Anchor v1 and v2 versions already bound them as associated token
  accounts of the pool.
- `finance/token-swap/quasar`'s `swap_tokens` re-checks the invariant against
  the vault balances after its transfers, rather than against computed ones.

## [2026-10-01] - Managed Fund's video script is removed

### Removed

- `finance/managed-fund/VIDEO_SCRIPT.md`, the only video script in the
  repository. It narrated an older version of the program: an `invest`
  handler that no longer exists, whitelisting, idle USDC in the fund, and
  figures that no test checks. The `PRODUCT.md` files no longer list it, and
  `test_full_lifecycle`'s doc comment points at the book's Managed Fund
  chapter, which narrates the same figures.

## [2026-10-01] - Managed Fund: decimal-aware valuation and permissionless rebalancing

### Fixed

- `finance/managed-fund` valued every asset as if it had six decimals and a
  Pyth exponent of −8. It now reads each feed's exponent and records each
  mint's decimals, in Anchor v2, Anchor v1 and Quasar.

### Changed

- `finance/managed-fund`'s `rebalance` is permissionless and computes its own
  trade from oracle prices and target weights, once drift passes a threshold
  fixed at fund creation, so no caller, the manager included, can churn the
  fund. See the example's changelogs.

## [2026-09-28] - Token Fundraiser is renamed Fundraiser

Contributors to the fundraiser receive no token, only a refund if the target is
missed, so "Token Fundraiser" described something the program does not do.

### Changed

- `finance/token-fundraiser` is now `finance/fundraiser`, in Anchor v2, Anchor
  v1 and Quasar, with its Kani proofs. The Quasar crate is `quasar-fundraiser`
  and the proofs crate `fundraiser-kani-proofs`; the Anchor programs were
  already named `fundraiser`. Accounts, instruction handlers and behavior are
  unchanged.
- `README.md`, `llms.txt` and the example's READMEs say it was formerly Token
  Fundraiser, and `finance/token-fundraiser/README.md` points old links to the
  new location.

## [2026-09-28] - Vault Strategy is renamed Managed Fund

The example called Vault Strategy is a manager-run fund: investors deposit for
shares priced at net asset value, and a manager allocates the pool across a
basket of approved assets. Finance calls that a managed fund, the onchain
equivalent of a mutual fund. It is not an index fund, whose weights follow an
outside index with no manager choosing them. "Vault" also meant two things in
the example: the whole product, and the single-asset token accounts it owns.
Now it means only the token accounts.

### Changed

- `finance/vault-strategy` is now `finance/managed-fund`, in Anchor v2, Anchor
  v1 and Quasar, with its Kani proofs and web app. The program crate is
  `managed-fund` (library `managed_fund`; Quasar `quasar-managed-fund`), the
  `Strategy` account is `Fund`, `initialize_strategy` is `initialize_fund`, the
  `StrategyNotFullyAllocated` error is `FundNotFullyAllocated`, and the error
  enum `VaultError` is `FundError`. The fund PDA's seed is `"fund"` instead of
  `"strategy"`, so fund addresses change, and `initialize_fund` and `Fund`
  have new Anchor discriminators. The program IDs and the programs' behavior
  are unchanged, and every test passes under its new name.
- The web app's IDL, client and environment variables follow the rename
  (`VITE_STRATEGY_INDEX` is `VITE_FUND_INDEX`, `VITE_VAULT_PROGRAM_ID` is
  `VITE_FUND_PROGRAM_ID`).
- `README.md` and `llms.txt` list the example as Managed Fund and say it was
  formerly Vault Strategy, and each of its READMEs says so too, so a search for
  either name finds it. `finance/vault-strategy/README.md` stays behind as a
  pointer to the new location for anyone following an old link.

## [2026-09-25] - Anchor v1 is the current stable Anchor

The README described Anchor v2 as Anchor's current major version and Anchor v1
as a previous version on long-term support. Anchor v1 is the current stable
release (1.2.0) and Anchor v2 is a release candidate (2.0.0-rc.1), so the
documentation now says so.

### Changed

- `README.md` lists Anchor v1 first, as the current major version, and Anchor v2
  as the upcoming, unreleased rewrite. Every example's links and the CI badges
  follow the same order.
- The note at the top of every `anchor-v1/README.md` names 1.2.0 as the current
  stable Anchor release instead of an LTS line.
- `CONTRIBUTING.md` describes the two Anchor directories the same way, and
  `llms.txt` names Anchor 1.2 rather than 1.1.

## [2026-09-23] - The order book evicts its worst order when a side is full

A side of the order book refused every new resting order once it was full, so
anyone willing to lock the minimum order size and pay rent for each slot could
fill a side with orders far from the spread and keep every new order on that
side out for as long as they liked. The market authority had no remedy, and
the market PDA's seeds are the two mints, so the pair could not move to a new
market. Now an order that beats a full side's worst price evicts that order
and rests in its place. The evicted order is refunded through its owner's
unsettled balance, exactly as `cancel_order` refunds it, and stamped
`Cancelled`. An order that does not beat the worst price still gets
`OrderBookFull`. The caller passes the worst order and its owner's
`MarketUser` after the maker pairs, or only the order when it is their own.
The capacity was also misstated: a side holds 512 orders, not 1024, because
every order after the first adds a leaf and an inner node to the side's
1024-node tree. All three variants (Anchor v2, Anchor v1, Quasar) change,
with the same five tests in each.

## [2026-09-23] - Anchor v1 copies match their v2 counterparts where the version allows

A scan of every `anchor/` and `anchor-v1/` pair, comparing function names,
error variants, account fields, constants and seeds, and listing every v2
program commit since July that left its v1 copy untouched, found these
differences that the Anchor version does not force:

- The transfer-hook `counter` and `account-data-as-seed` v1 copies still had
  the bug v2 fixed in August: the hook computed the new transfer count and
  dropped it, because `counter_account` was not `mut`, so the count read one
  after every transfer. Both now write the count back and report an overflow
  with the new `CounterOverflow` error instead of `AmountTooBig`, and each
  test reads the counter back after the hooked transfer.
- Interest-bearing (v1): `check_mint_data` becomes `check_rate_authority`
  with v2's signature, taking the parsed `InterestBearingConfig`. The
  handlers read the extension with anchor-spl's `get_mint_extension_data`.
- Account fields that v2 declares `pub` are `pub` in the v1 copies of
  account-data, checking-accounts, program-derived-addresses, rent,
  transfer-sol and nft-operations.
- Order book (v1): `OrderTreeRoot` derives `Default` like the v2 copy,
  replacing a hand-written impl.

What the scan still reports is forced by the Anchor version: v2's
transfer-hook `entrypoint` fallback, the local session-token reader that
replaces the v1-only `session-keys` crate, the `last_restart_slot` syscall
wrapper (the v1 copies make the same restart check through the sysvar), the
wincode span in rent, zero-copy padding, the Pyth account traits, and the
betting market's borrow-release helper.

## [2026-09-23] - Order book tests cover the deepest critbit path

A critbit tree does not rebalance, so asks at doubling prices stretch the path
to the best ask by one level each, up to 64 levels from the price half of the
128-bit key. Both Anchor copies of the order book gain two tests:
`doubling_prices_build_the_deepest_path_prices_allow` builds that path and reads
its depth from the account, and
`deepest_path_adds_little_compute_to_insert_fill_and_cancel` checks that
inserting, filling, and canceling at the bottom of it each cost less than 15,000
compute units more than on a shallow book, and stay inside the default
200,000-unit instruction budget. The README no longer says the tree stays
shallow whatever order keys arrive in, or that Phoenix uses the same structure
(it uses a red-black tree). No program behavior changes.

## [2026-09-23] - Finance examples point their production oracle path at Pyth

The oracle network that the lending, perpetual futures and prop AMM examples
modeled their price feeds on has shut down. The perpetual futures and prop AMM
mock oracle program is now `mock-price-feed` in both Anchor variants, with the
same program ID, instructions and account layout. Every production-path comment
and README in the three examples, across Anchor v2, Anchor v1 and Quasar, now
points at a Pyth `PriceUpdateV2` account, which `basics/pyth` reads. No program
behavior changes.

## [2026-09-23] - The token fundraiser and order book Anchor v1 copies catch up

Under the old rule that `anchor-v1/` copies were frozen, two v1 copies were
left behind by changes to their v2 counterparts. Now that the copies track
each other, both are ported.

- Token fundraiser (Anchor v1): `close_contributor` and the
  `FundraiserStillOpen` error, so a contributor to a successful raise can take
  back their Contributor account's rent. Same two tests as the v2 copy.
- Order book (Anchor v1): the base, quote, and fee vaults are PDAs of the
  market at `["base_vault", market]`, `["quote_vault", market]` and
  `["fee_vault", market]`, so a client derives them instead of generating and
  signing with three extra keys. The tests derive them too.

## [2026-09-23] - Anchor v1 copies track their Anchor v2 counterparts

CONTRIBUTING.md described each `anchor-v1/` copy as a frozen snapshot that
changed only to keep its build green. Several v2 examples have since gained
behavior their v1 copies lack, so a reader on the v1 LTS line was learning a
different program from the one the v2 copy and the book describe.

- CONTRIBUTING.md now says a change to what an example does goes into both
  copies, and the copies differ only where the Anchor version forces it.
- The betting market's Anchor v1 copy gains the draft state and betting close
  time from the v2 copy: `open_betting`, `betting_closes_at`, the
  `EventNotDraft`, `NotEnoughOutcomes`, `CloseTimeInPast`, `BettingClosed` and
  `BettingStillOpen` errors, a draft-or-open `cancel_event`, and the same new
  tests.

## [2026-09-22] - Lending interest and perpetual futures funding accrue by the wall clock

Both programs accrued over elapsed slots, so a rate quoted per year or per
hour depended on a guess at the slot length. The lending reserve's
`slots_per_year` default assumed 400 ms slots and charged twice the advertised
APR once the network moved to 200 ms. Interest and funding now accrue for the
seconds between the Clock's `unix_timestamp` and a stored
`last_accrual_timestamp` (lending) or `last_funding_timestamp` (perpetual
futures). `slots_per_year` is removed from `ReserveConfig`, and
`funding_rate_per_slot` is now `funding_rate_per_second`. A timestamp at or
before the stored one accrues nothing. In the Anchor variants,
`update_reserve_config` now accrues at the old curve before storing a new one;
the Quasar lending variant drops its `update_slots_per_year` instruction.
Price freshness is still counted in slots. All three variants (Anchor v2,
Anchor v1, Quasar) of both programs change.

## [2026-09-22] - Betting market events have a draft state and a close time

The betting market locked its outcome list implicitly, by refusing
`add_outcome` once `total_pool` was nonzero. Anyone could bet one minor unit on
a half-built market to freeze it with a single outcome, and the admin's
`add_outcome` and the first bet could land in either order in the same slot.
Nothing stopped bets between the real-world result and settlement, either:
`place_bet` accepted stakes until `settle_event` ran.

- Events start as `Draft`. `add_outcome` works only on a draft (else the new
  `EventNotDraft`, which replaces `BettingAlreadyStarted`), and a new admin
  handler, `open_betting`, moves a draft with at least two outcomes (else
  `NotEnoughOutcomes`) to `Open`. `place_bet` on a draft fails with
  `EventNotOpen`, so the outcome list is final before any money can arrive.
- `initialize_event` takes a `betting_closes_at` timestamp, which must be in the
  future (else `CloseTimeInPast`) and is stored on the event. `place_bet`
  requires `now < betting_closes_at` (else `BettingClosed`) and `settle_event`
  requires `now >= betting_closes_at` (else `BettingStillOpen`).
- `cancel_event` accepts a draft as well as an open event.
- Changed in the Anchor v2 and Quasar ports, with new tests for each rule, and
  two new Kani proofs: the betting and settlement windows partition time, and a
  lifecycle model in which the outcome list never changes once money is in the
  pool. The Anchor v1 port is a frozen snapshot and does not change.

## [2026-09-22] - The first-deposit minimum is counted in every share divisor

The token swap and perpetual futures examples both withhold a minimum from the
first deposit's LP shares, but only one side of their share math counted it.

- Token swap: `withdraw_liquidity` divided by `lp_supply + MINIMUM_LIQUIDITY`,
  but `deposit_liquidity` minted later deposits against the bare `lp_supply`.
  Every later depositor was minted slightly fewer LP tokens than they could
  redeem; a donation to the vaults as large as a victim's deposit, rather than
  101 times it, rounded that deposit down; and once every LP token was burned
  the pool could never take a deposit again, since the floor's reserves stayed
  behind with a supply of zero to divide by. Later deposits now divide by
  `lp_supply + MINIMUM_LIQUIDITY` as well, in all three ports.
- Perpetual futures: `add_liquidity` withheld 1,000 shares from the first
  deposit but both `add_liquidity` and `remove_liquidity` divided by the bare
  share supply, so the withheld value belonged to the share holders pro rata
  and locked nothing. Shares are priced against `Pool.liquidity` rather than
  the vault balance, so a direct donation moves nothing, but a provider who is
  also the pool's only trader can grow `liquidity` with their own funding
  payments. A new test does exactly that: the attacker opens the pool with one
  share, pays about 1,000 USDC of funding into it, and against the old program
  withdraws about 1,500 USDC after a victim deposits. Both handlers now divide
  by the share supply plus `MINIMUM_LIQUIDITY`, and a pool whose providers have
  all left prices the next deposit against the locked slice instead of
  bootstrapping it, in all three ports.
- The token swap's Kani crate models the deposit formula and proves a deposit
  followed by a withdrawal returns at most the deposit and loses less than one
  LP token's worth, a bound the bare-supply formula fails.
- The Anchor v1 ports take the same fixes, since they correct share
  accounting rather than add features.

## [2026-09-22] - Vault strategy ignores donations

The vault strategy valued itself and paid withdrawals from its vaults' token
balances, and anyone can transfer tokens into a vault. A dust-sized first
deposit followed by a donation could price one share above the next deposit
and round it down to zero shares: the first-depositor inflation attack.

- `finance/vault-strategy` (Anchor v2, Anchor v1 and Quasar) records what the
  strategy holds, `usdc_holdings` and `asset_holdings`, updated by `deposit`,
  `withdraw` and `rebalance` with what each transfer actually moved, and prices
  shares and pays withdrawals from those records. Donated tokens are outside
  the fund, and `rebalance` can neither sell nor spend them
  (`InsufficientHoldings`), as the lending example already ignores donations.
- A deposit so small that a swap returns none of its asset is rejected with
  `DepositTooSmall`; it would otherwise mint shares against a fund worth
  nothing and make every later deposit divide by zero.
- Tests in all three ports run the attack, the Kani crate proves recorded
  holdings never exceed vault balances and that a donation cannot dilute the
  next deposit, and the web apps read the recorded holdings.

## [2026-09-22] - Vault strategy rejects prices from before a cluster restart

The vault strategy checks Pyth freshness in seconds. Under Alpenglow each
leader sets the Clock's `unix_timestamp`, which may advance by at most twice
the slot time elapsed since the parent block, so after a halt the timestamp
trails real time and catches up gradually, and a price published just before
a multi-hour halt still passes the 60-second check.

- `finance/vault-strategy` (Anchor v2, Anchor v1 and Quasar) reads each Pyth
  update's `posted_slot` and rejects it when it is at or before the
  `LastRestartSlot` sysvar's slot, with the new `PricePredatesRestart` error,
  as the lending, prop-amm and perpetual-futures examples already do. Tests,
  READMEs, PRODUCT.md files, the web apps' IDLs and changelogs follow.

## [2026-09-22] - The order book's vaults are market PDAs

The order book created its base, quote, and fee vaults at public keys the client
generated, so a client had to generate and sign with three extra keys to open
a market, and nothing but the market's record said where its tokens were.

- The order book's Anchor v2 and Quasar ports create all three vaults as PDAs
  of the market, at `["base_vault", market]`, `["quote_vault", market]` and
  `["fee_vault", market]`, with the market as their authority, the same shape
  the options and prop AMM vaults already have. The stored-address checks
  that stop the fee vault being passed as the quote vault are unchanged. The
  order book itself stays client-allocated, since at about 180 KB it is too
  large for the program to create. Tests, READMEs, and changelogs follow. The
  Anchor v1 port is a frozen snapshot and does not change.

## [2026-09-22] - Anchor v2 CI no longer starts a validator for LiteSVM-only projects

Surfpool 1.6, which the Anchor v2 workflow installs as `latest`, deploys each
program at startup through a runbook that reads `target/idl/<program>.json`.
The nine projects built with `--no-idl` (the anchor#4947 workaround) have no
IDL, so `anchor test` died with `Surfpool startup failed: Runbook execution
failed` before any test ran, turning main's Anchor v2 runs red.

- `anchor test` runs with `--skip-local-validator` for exactly those nine
  projects. Each one's Anchor.toml runs `cargo test` against LiteSVM, so none
  of them ever used the validator. Every other project is tested as before.

## [2026-09-14] - Token fundraiser contributors can close their accounts

A successful raise exits through `check_contributions`, which closes the vault
and the fundraiser account but cannot reach the contributor accounts: there is
one per contributor and the claim carries none of them. Their only other
closer, `refund`, runs only on a failed raise, so every contributor to a
successful raise held their rent in an account nothing could close.

- The token fundraiser gains `close_contributor` in its Anchor v2 and Quasar
  ports: a contributor closes their own contributor account once the
  fundraiser is gone, and the rent comes back. The one check is that the
  passed fundraiser account is not owned by the program, else the new
  `FundraiserStillOpen` error, so a live contribution still closes only
  through `refund`. Tests, READMEs, and changelogs follow. The Anchor v1 port
  is a frozen snapshot and does not change.

## [2026-09-10] - Market and pool accounts own their vaults

Five finance examples kept a dataless "authority" PDA beside their state
account, existing only to own the vaults and mints and to sign for them: the
token swap, the prop AMM, the perpetual futures pool, the options venue, and the
vault strategy's mock swap router. A program-owned data account signs with its
own seeds just as well, as the escrow's offer account already does, and its
seeds never change however its data does, so the extra account bought nothing
but one more account per instruction and one more stored bump.

- The pool config, market, pool, and router config now own their vaults and
  mints directly and sign with their own seeds, in every port: Anchor v2,
  Anchor v1, and Quasar. The token swap's reserves are now associated token
  accounts of the pool config, so their addresses changed; the vault strategy's
  web app derives the router treasury from the router config.
- Each affected port's tests assert the new ownership, and its README and
  changelog follow.
- Pre-existing formatting drift and clippy findings in the prop AMM and token
  swap v1 and Quasar ports were fixed so the gates pass.

## [2026-09-08] - Anchor v1 examples on Anchor 1.2.0

Anchor 1.2.0 is the current release of the v1 line. Every `anchor-v1/` example now
builds against it. No program source changed: 1.2.0 has no breaking changes, and
nothing here calls the two functions it deprecates (`cpi_guard_enable` and
`cpi_guard_disable`). The test stack had to move, though, because the new
`anchor-lang` cannot share a dependency graph with the old LiteSVM.

### Changed

- All 56 `anchor-v1/` examples move `anchor-lang` and `anchor-spl` from `1.1.2` to
  `1.2.0`. Feature lists (`init-if-needed`, `metadata`, `token_2022`, ...) are
  unchanged.
- Their tests move from `litesvm 0.13.1` to `litesvm 0.16.0` (Agave 4.2), and the
  test-only crates that must stay in step with LiteSVM move to their 4.x lines:
  `solana-transaction 4.1.5`, `solana-message 4.2.4`, `solana-account 4.3.0`. The
  55 examples that test through `solana-kite` move to `solana-kite 0.5.0`, the
  release that makes the same LiteSVM bump. The reason: `anchor-lang 1.2.0` requires
  `solana-loader-v3-interface ^6.1.1`, which requires `solana-instruction ^3.3.0`,
  while `litesvm 0.13.1` pins `solana-instruction = "=3.2.0"`. Cargo cannot satisfy
  both, and `solana-kite 0.4.0` pins `litesvm 0.13.1`, so the whole test stack moves
  together.
- Three tests warp to a slot relative to the current one instead of an absolute
  slot: `test_stale_price_rejected` and `test_funding_charged_to_long` in
  `perpetual-futures`, and `test_swap_rejects_stale_price` in `prop-amm`. LiteSVM
  0.14 and later start the clock at a mainnet-like slot rather than zero, so a
  warp to slot 200 or 10,000 moved time backwards and the price under test never
  went stale. Every other test in the tree already warped relative to the current
  slot. No other test source changed.
- The Anchor v1 workflow installs and caches `anchor-cli 1.2.0` instead of `1.1.2`.
  Everything else about the job is as before: the CLI still comes from crates.io
  with `--locked`, the Solana CLI is still 3.1.14, and project discovery is
  unchanged.
- Every `anchor-v1/` README, `CONTRIBUTING.md` and `docs/anchor-v2-migration.md`
  name 1.2.0 as the v1 CLI to install.

### Note

- `anchor-cli 1.2.0` builds with `--tools-version v1.57 --arch v3` by default, where
  1.1.2 passed `--tools-version v1.52` and left the architecture at `cargo
  build-sbf`'s default. The v1 programs are therefore now SBPF v3 binaries built with
  platform-tools v1.57. `ANCHOR_BUILD_SBF_ARCH` overrides the architecture if that
  ever needs to change.

## [2026-09-04] - Options venue example

### Added

- `finance/options`: a fully collateralized, physically settled options venue,
  in Anchor v2, Anchor v1, and Quasar, with a Kani proof crate. A writer posts
  the whole obligation (the underlying for a call, the strike in the quote token
  for a put) and lists an option at a premium; a buyer pays the premium and becomes
  the holder; the holder may exercise before expiry; after expiry the writer
  reclaims the collateral. Eight instruction handlers (`initialize_market`,
  `write_option`, `buy_option`, `cancel_option`, `exercise_option`,
  `collect_proceeds`, `reclaim_collateral`, `collect_fees`). Every settlement
  amount is a product of two of the option's integers, so there is no division and
  no rounding in settlement; the venue's fee on each premium is the only floor.
  The market account keeps a ledger of what each vault owes, asserted against
  the vault balances after every transfer, and the proof crate walks every path
  through an option's life and shows the ledger returns to zero. No oracle: physical
  settlement moves the tokens themselves, so the program never has to know the
  price. Each option is one account, bought and exercised as a whole.
- The Anchor v2 copy joins the `--no-idl` list in `.github/workflows/anchor.yml`
  (anchor#4947: its `OptionKind` and `OptionStatus` enums reach the IDL) and the
  root Cargo workspace; the proof crate joins both matrices in
  `.github/workflows/kani.yml`.

## [2026-08-21] - Anchor v1 kept alongside Anchor v2

Anchor v1 is expected to stay on long-term support, and many deployed programs
will stay with it. Both versions of every Anchor example now ship, each built and
tested by its own CI job.

### Added

- Every one of the 55 Anchor examples gains a sibling `anchor-v1/` directory
  holding the example exactly as it stood before the v2 port, restored from
  `94abbea`, the last commit on `main` before that merge. 864 files. The 17
  third-party `.so` test fixtures are byte-identical to the ones already tracked
  under `anchor/`, so git stores one copy and the tree grows by ~2.3 MB of text.
- `.github/workflows/anchor-v1.yml` builds and tests them on Anchor 1.1.2,
  installed through avm. It is the workflow as it stood at `94abbea`, with
  project discovery changed to `find -type d -name "anchor-v1"`. The v2-only IDL
  workaround (`anchor#4947`, enum variants) is not carried over: that bug does not
  exist in 1.1.2, so v1 builds generate IDLs normally.

### Changed

- The `Anchor` workflow is now `Anchor v2`, so the two checks read as a pair. Its
  filename, triggers and `find -type d -name "anchor"` discovery are unchanged.
  Both workflows match a directory name exactly, so neither can ever see the
  other's projects.
- Both Anchor workflows install the CLI from crates.io (`cargo install anchor-cli
  --version <v> --locked`). The v1 job started out installing avm from the tip of
  anchor's `main` branch, which meant what CI installed drifted with whatever landed
  there; 1.1.2 is an ordinary published release, so it comes from the registry like
  2.0.0-rc.1 does.
- Both Anchor workflows now run weekly (v2 Mondays 03:00 UTC, v1 05:00 UTC). The
  analyze step has always treated a scheduled run as "build everything", but nothing
  declared a schedule, so that path had never executed. It is worth having because no
  Anchor project commits a `Cargo.lock` and Dependabot only covers the root workspace,
  so dependency drift in either tree is otherwise invisible until an unrelated pull
  request happens to touch it.
- The two workflows no longer report colliding check names. Both declared jobs called
  `changes`, `summary` and `build-and-test-group-N`; the v1 job names are now
  `changes (Anchor v1)`, `anchor-v1-group-N` and `summary (Anchor v1)`.
- Every Anchor example README now names the CLI its commands need, on both sides:
  `anchor/` pages say Anchor v2 and `anchor-v1/` pages say Anchor v1. A bare
  `anchor build` was unambiguous while the repository had one Anchor and is not
  any more. `README.md` and `CONTRIBUTING.md`, which sit above both, show both.

### Note

- The `anchor-v1/` crates are not members of the root Cargo workspace and cannot
  be: they carry the same package names as their `anchor/` siblings. `cargo fmt`
  and `cargo clippy` therefore do not see them, and the Anchor v1 workflow is what
  keeps them honest. They also have no committed `Cargo.lock` (`.gitignore` ignores
  `**/*/Cargo.lock`), so their transitive dependencies resolve fresh on each run and
  can break without anyone touching the directory.

## [2026-08-16] - Every Anchor example on Anchor v2.0.0-rc.1

All 55 Anchor examples build and pass their tests on 2.0.0-rc.1 (304 tests),
and `cargo fmt --check` and `cargo clippy -- -D warnings` are clean.

### Changed

- The remaining 39 Anchor examples, all of `tokens/`, `finance/` and
  `compression/`, now build against `anchor-lang` 2.0.0-rc.1, joining the
  `basics/` examples ported below. `.github/workflows/anchor.yml` installs
  2.0.0-rc.1, since `anchor build` under a v2 CLI will not build v1 programs.
- `docs/anchor-v2-migration.md` collects every difference the port ran into,
  ordered by how often it bites. The rules the compiler will not catch for you
  are called out: borrows held across CPIs, `Box`'s missing `cpi_handle_mut`
  forwarding, and hand-built read-only handles over a live data account.
- `has_one` is deprecated in v2 and this repository's `rust.yml` runs
  `cargo clippy -- -D warnings`, so every one of the 161 uses across 66 files
  in the Anchor programs moves to the `address` constraint on the sibling field
  it named. The Quasar crates keep `has_one`, which is still current there.
- The seven `transfer-hook` examples supply their own entrypoint. v2's
  `#[program(interface, ...)]` generates a CPI client and no dispatch, so the
  program declared that way builds to a ~900-byte object with no `entrypoint`
  crate has no entrypoint symbol, while an executable `#[program]` limits
  byte, which the transfer-hook interface's eight-byte values cannot use. Each
  crate now builds with `no-entrypoint`, so anchor exports its dispatch as
  `__anchor_dispatch`, and `src/entrypoint.rs` maps the interface
  discriminators onto handlers before delegating.
- `tokens/pda-mint-authority` and `tokens/token-extensions/cpi-guard` build
  their PDA by hand (`create_account` plus `initialize_mint2` /
  `initialize_account3`). Both examples exist to show an account that is its own
  authority, and a v2 SPL `init` constraint cannot name the account being
  initialized.
- `finance/order-book` keeps its ~180 KB zero-copy critbit book zero-copy: v2's
  `Account<T>` derefs straight to `T`, so `load_init` / `load_mut` simply go
  away rather than the state converting to borsh.
- Tests that asserted on an Anchor error *name* now assert on the numeric custom
  code (the `#[error_code]` discriminant plus the default 6000 offset). v2 does
  not log variant names, so the old assertions could never match.

### Removed

- `tokens/token-extensions/nft-meta-data-pointer` no longer depends on
  `session-keys`. That crate is Anchor v1 only: its `Session` derive requires
  `Option<Account<'info, SessionToken>>`, and `SessionToken` is not `Pod`, so
  v2's zero-copy `Account<T>` cannot hold it either. The program reads the
  session-token account layout itself (`src/session.rs`), checking owner,
  discriminator and PDA, and spells out the `#[session_auth_or]` fallback in the
  handler, so the gasless-session lesson and its security warning both survive.

## [2026-08-13] - Anchor examples in `basics/` move to Anchor v2.0.0-rc.1

### Changed

- Every Anchor example under `basics/` now builds against `anchor-lang`
  2.0.0-rc.1. v2 is a ground-up rewrite rather than a version bump: the crate is
  `no_std` and built on pinocchio, so handlers take `&mut Context<T>`, the
  `<'info>` lifetime disappears from `#[derive(Accounts)]` structs and account
  wrappers, `Pubkey` becomes `Address`, `.to_account_info()` becomes
  `.cpi_handle_mut()` / `.cpi_handle()`, and `.key()` becomes `.address()`.
- `#[account]` is now zero-copy and requires a `Pod` layout. State holding
  `String` or `Vec` moves to `#[account(borsh)]` plus `BorshAccount<T>`
  (`account-data`, `close-account`, `favorites`, `realloc`, `pyth`); state that
  is already fixed-layout keeps the zero-copy default but must carry explicit
  padding (`program-derived-addresses`) and cannot use `bool`
  (`cross-program-invocation` uses `PodBool`).
- Instruction data is wincode-encoded rather than borsh. The `#[program]` macro
  expands to `wincode` paths, so every program crate takes a direct `wincode`
  dependency. `BorshConfig` keeps the wire format byte-identical to borsh, so
  the checked-in account layouts and the tests that decode them with borsh are
  unaffected.
- The only edit most LiteSVM tests needed: v2's `solana_program` compat shim has
  no `system_program` submodule (the real module is at the crate root and
  exposes `ID`, not `id()`) and no `pubkey::Pubkey` unless the `compat` feature
  is on (`anchor_lang::Address` is the same 32-byte type).

### Fixed

- Anchor programs that put an `Address` in serialized state pin
  `solana-address = ">=2.6, <2.7"`. anchor-lang 2.0.0-rc.1 is built against
  wincode 0.5, but solana-address 2.7 moved to wincode 0.6; with both in the
  graph, `Address`'s wincode impls belong to the version the `#[account(borsh)]`
  derive is not using, and every `SchemaRead` / `SchemaWrite` bound fails. This
  is the same class of split that the zeropod/quasar-lang pin below addresses.

## [2026-08-04] - Oracle readers reject prices from before a cluster restart

### Added

- The three oracle-priced finance examples (`finance/lending`, `finance/prop-amm`, `finance/perpetual-futures`, Anchor and Quasar variants) now reject an oracle price stamped at or before the `LastRestartSlot` sysvar's slot, with a dedicated error (`PricePredatesRestart` / `PRICE_PREDATES_RESTART`) and a test per variant. A cluster halt stops the slot count but not the wall clock, so after a restart a feed can pass a slot-measured staleness bound while its price is hours old; the market pauses valuation until the publisher posts again. quasar-lang ships no LastRestartSlot sysvar, so each Quasar variant declares the 8-byte layout in `src/last_restart.rs` and reads it via `sol_get_sysvar`.

### Fixed

- The three Quasar variants pin `zeropod = "=0.3.3"`: zeropod 0.3.4 moved to wincode 0.5 while quasar-lang's pinned rev stays on wincode 0.4, so any fresh resolve (these projects commit no lockfile) split the graph across two wincode versions and failed every `Pod*` trait bound.

## [2026-07-23] - Metadata examples on Quasar 0.1.0 (vendored quasar-metadata)

### Added

- `tokens/quasar-metadata`: a vendored copy of the `quasar-metadata` crate from blueshift-gg/quasar rev `623bb70f` (the last revision that shipped it), adapted to compile against the 0.1.0 `quasar-lang` API (`RentAccess` type parameter on `AccountInit::init`, `try_find_program_address` rename, `Seed` import from `quasar_lang::cpi`, `unsafe` `set_data_len`). Upstream removed the crate before 0.1.0 with no replacement; vendoring it lets the Metaplex-metadata examples ride the same release pin as everything else. Provenance and local changes are documented in the crate's README and CHANGELOG.

### Changed

- `tokens/token-minter`, `tokens/nft-minter`, and `tokens/nft-operations` now migrate to the 0.1.0-release pin (`be60fca`) like every other example, depending on the vendored crate via `quasar-metadata = { path = "../quasar-metadata" }`. This supersedes the previous day's "not migrated" limitation: all 53 Quasar examples are now on 0.1.0.
- `quasar.yml` drops the `legacy-metadata-examples` job and `.github/.ghaignore` is empty again — the whole matrix builds with the one 0.1.0 CLI.

## [2026-07-22] - Quasar 0.1.0

### Changed

- Migrated 50 of the 53 Quasar examples to the Quasar `0.1.0-release` line, pinned by rev (`be60fca`) because crates.io still hosts `0.0.0` placeholders for `quasar-lang`/`quasar-cli`. Per project: `quasar-lang`/`quasar-spl` repinned (the four previously floating examples — `basics/pyth` and the three `compression` examples — are now pinned too); `Quasar.toml` rewritten to the 0.1.0 schema (`[testing] command`, `[clients] targets`; the old `[toolchain]`/`testing.language`/`testing.rust`/`clients.languages` keys are hard errors in 0.1.0); the `idl-build` feature and `"lib"` crate-type added for the new IDL build; and tests fully rewritten from the direct QuasarSVM harness (`QuasarSvm::new().with_program(...)`, `include_bytes!`, `assert_success`) to the new `quasar-test` fixture harness (`#[quasar_test]`, `Wallet`/`Mint`/`TokenAccount` fixtures, `crate::cpi` instruction builders, `Outcome` assertions). The standalone `quasar-svm` git dev-dependency is gone — `quasar-test` pulls the published `quasar-svm 0.1.0` from crates.io — and generated-client path dev-dependencies were dropped in favor of `crate::cpi` (a path dev-dependency to a not-yet-generated crate now breaks the required `cargo generate-lockfile`).
- `quasar.yml` CI installs the 0.1.0 CLI (`--rev be60fca`), runs `cargo generate-lockfile` before `quasar build` (the 0.1.0 IDL step runs `cargo metadata --locked`), and builds the three unmigrated examples in a separate `legacy-metadata-examples` job with the pre-0.1.0 CLI.
- Compute-unit assertions were dropped from the migrated tests pending recalibration under 0.1.0 (correct values are unknowable until the suite first runs on the new line).

### Known limitations

- `tokens/token-minter`, `tokens/nft-minter`, and `tokens/nft-operations` stay on the pre-0.1.0 pins (quasar `623bb70` / quasar-svm `cb7565d`): they depend on `quasar-metadata`, which was removed upstream before 0.1.0 with no replacement. They are listed in `.github/.ghaignore` and built by the legacy CI job.

## [2026-07-11] - Discoverability and FAQ pass

### Fixed

- Quasar CI broke repo-wide when `quasar-svm`'s HEAD (`c63afd2`, "sbpf v3") moved to `solana-program-runtime` 4.1 / `solana-address` 2.6, which cannot co-resolve with the pinned `quasar-lang` rev `623bb70` (needs `solana-address` <2.6). Pinned `quasar-svm` to `cb7565d` (the last rev before the bump) in every Quasar example that pins `quasar-lang`, matching the pin `prop-amm` already carried. `basics/pyth` and the three `compression` Quasar examples float both dependencies and are left as-is.

### Added

- FAQ sections, written as the questions people actually ask, in the root README and every finance example's `anchor/` README.
- `llms.txt` at the repository root: a summary and link manifest for LLM crawlers and answer engines.
- `docs/example-readme-template.md`, the example-README template that `CONTRIBUTING.md` referenced but which did not exist. It documents the H1 convention and the definition-first opener.

### Changed

- Every finance example README now titles itself `# Solana <Example> (<Framework>)` (e.g. `# Solana Escrow (Anchor)`) and opens with a self-contained definition that names Solana, so each example page stands alone in search results.
- The root README states its toolchain currency explicitly (Anchor 1.1, LiteSVM, July 2026) with a pointer to this changelog.
- `CONTRIBUTING.md` style rules now include the README H1 naming convention and the no-em-dash rule.

## [2026-07-10] - Failed fundraisers can be retired

### Added

- `token-fundraiser` (Anchor): a `close_fundraiser` instruction handler. The Fundraiser PDA is derived from the maker's key alone, so a failed raise used to lock its maker out of ever raising again. The maker can now retire a failed fundraiser (after the deadline, target missed, all contributions refunded), sweeping any direct vault donations to themselves and recovering both rent deposits, then initialize a fresh fundraiser. New error variant `RefundsOutstanding`.
- `token-fundraiser` (Anchor): tests for both contribution caps (`test_contribute_above_cap_fails`, `test_cumulative_contributions_above_cap_fail`) and for every branch of the close path (before deadline, target met, refunds outstanding, donation sweep, and close-then-raise-again).

## [2026-06-30] - Anchor 1.1.2

### Changed

- Upgraded every Anchor program from `anchor-lang`/`anchor-spl` `1.0.0` to the latest stable `1.1.2`, and bumped the Anchor CLI used by `anchor.yml` CI to match (`anchor-version: 1.1.2`).

### Fixed

- `anchor.yml` built no projects when `.ghaignore` was empty: `find … | grep -vE "$ignore_pattern"` treated the empty pattern as "match everything" and dropped the whole list, so the workflow passed without building anything. Guarded the filter (as `native.yml`, `pinocchio.yml` and `solana-asm.yml` already do).
- `vault-strategy` and `perpetual-futures` LiteSVM tests loaded their sibling mock program's `.so` with `include_bytes!`, which is evaluated at compile time. Anchor's IDL build compiles the tests before that sibling `.so` is built, so the build failed. They now read the sibling `.so` at runtime with `std::fs::read`, matching the existing `cross-program-invocation/hand` test.

## [2026-06-12] - Rust + LiteSVM tests everywhere

### Changed

- All native, Pinocchio, and ASM examples are now tested exclusively with Rust + LiteSVM. The web3.js v1 / solana-bankrun / ts-mocha TypeScript test suites (which duplicated existing Rust tests) were removed, along with their `package.json`, `pnpm-lock.yaml`, and `tsconfig.json` files and the `ts/` client directories.
- Rust tests now load the program binary from the workspace `target/deploy/` (built with `cargo build-sbf --manifest-path=./program/Cargo.toml`) instead of per-project `tests/fixtures` directories. Committed foreign-program fixtures (e.g. `mpl_token_metadata.so`) stay where they were.
- ASM examples standardized on `sbpf build`'s default `deploy/` output directory; their inline LiteSVM tests load from there.
- `tools/shank-and-codama` now generates a Rust client (`@codama/renderers-rust`) instead of a TypeScript one, wrapped in the `car-rental-service-client` crate, and its tests are Rust + LiteSVM under `program/tests/`.
- `transfer-hook/block-list` gained a Rust + LiteSVM lifecycle test (`program/tests/`) driving the program through its Codama-generated Rust SDK; the mocha/web3.js test was removed. Its `package.json` now only covers SDK generation.
- CI (`native.yml`, `pinocchio.yml`, `solana-asm.yml`) no longer installs Node/pnpm; it builds with `cargo build-sbf` (or `sbpf build`) and tests with `cargo test`.

### Added

- `basics/hello-solana/pinocchio` Rust + LiteSVM test (it previously had only a TypeScript test).

## [2026-04-08] - Quicknode fork modernization (Mike MacCana)

Mike MacCana led the Quicknode fork of the [Solana Foundation program examples](https://github.com/solana-developers/program-examples) from late 2025. The first commits on this repository lineage are dated **8 April 2026**; the summary below covers that work through the initial merge.

### What changed (high level)

**Toolchain and frameworks.** The tree had accumulated examples from several years of Solana development (including Anchor releases going back to the ~0.26 era in 2022 and many intermediate versions). The fork brought the Anchor examples up to **Anchor 1.0.0** stable (from 1.0.0-rc.5), refreshed Agave/Solana CLI pins, standardized on **pnpm**, and added parallel implementations in **[Quasar](https://quasar-lang.com/docs)**, **Pinocchio**, **Native Rust**, and **ASM** where applicable. Token-2022 examples were renamed to **`token-extensions`**.

**Testing.** Replaced the old pattern of local validators, Bankrun, and scattered TypeScript `anchor test` flows with **LiteSVM in-process tests** for most Anchor programs - matching current Anchor defaults (`cargo test` wired through `Anchor.toml` / `pnpm test`). Fixed broken or flaky tests across Native, Pinocchio, and Anchor; added missing harnesses (e.g. block-list Pinocchio). CI was reworked for a repo this size: path filtering, caching, matrix sharding, and reliable detection of framework roots.

**Programs and layout.** Broke large monolithic `lib.rs` files into **instruction handler modules**; adopted **`InitSpace`** and explicit PDA bumps instead of magic account sizes; corrected several logic bugs (escrow, token swap invariant, counter authority checks, compression Bubblegum program id, and more). Expanded finance and token-extension coverage; reorganized transfer-hook examples (including block-list under Pinocchio).

**Documentation.** Rewrote the root README (framework badges, clearer example blurbs, ASM links), ran a style and **truth audit** on READMEs, and linked canonical [Solana terminology](https://solana.com/docs/references/terminology) on first mention. Added this changelog, `CONTRIBUTING.md` (aligned with LiteSVM testing), README templates, per-example Anchor and Quasar READMEs, fixed Husky for GUI git clients, removed unused maintainer scripts (`sync-package-json`, `cicd.sh`, local-validator helpers for the allow/block-list UI), dropped the orphan `tokens/spl-token-minter/` tree, and removed legacy root `package.json` dependencies (web3.js, Bankrun, chai).

**Removed / deferred.** Dropped duplicate or WIP trees (duplicate block-list Pinocchio copy, Quasar metadata example blocked on `sol_realloc`, root `yarn.lock`). Some examples remain excluded from CI via `.ghaignore` until they build cleanly again (compression, escrow, pyth, and others - see that file for the live list).

## Before June 2026

There was **no changelog** before June 2026. Older history lives in git only.