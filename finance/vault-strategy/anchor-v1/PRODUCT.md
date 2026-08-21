# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

## Users

The frontend serves two roles first; a third exists in the program and is secondary for now.

- **Depositor (primary):** a retail user who deposits USDC into a manager-run strategy, receives shares representing proportional ownership of the whole basket, watches their position's value track the portfolio, and withdraws their slice in kind when they choose. Highest-volume, lowest-expertise surface.
- **Manager (primary):** a power user who creates and operates a strategy — registers curator-approved assets, sets target weights, rebalances as prices drift, and collects the management fee. An operations surface where correctness and confirmation matter more than reach.
- **Curator (secondary, not yet in scope):** governs the approved-asset registry that bounds what managers may hold. Low-frequency, high-trust. Recorded so future work does not treat the manager as the only privileged role.

## Product Purpose

An **educational demo dApp** shipped alongside the `vault-strategy` Solana program (part of a public Solana examples collection). Its job is to make the program's mechanics legible and touchable — show, through a real interface, how deposits are priced by NAV, how shares represent ownership, how managers configure weights and rebalance, and how in-kind withdrawal works. Success is a visitor understanding and exercising the full deposit → hold → withdraw and configure → allocate → rebalance loops against the deployed program. It teaches by letting people operate it, not by describing it.

## Positioning

A **multi-asset** strategy built from single-asset vaults, transparently priced on-chain. Unlike an ERC-4626-style single-asset vault, one "strategy" owns one vault per asset plus a USDC vault, and every deposit is deployed across the basket at its target weights in the same transaction — there is no idle-cash mode. Deposit pricing, slippage floors, and fees are all derived on-chain from the Pyth oracle and the strategy's own parameters rather than trusted from a caller, which is the truth the interface must make visible: the numbers a user sees are the numbers the program enforces.

## Operating Context

- Solana wallet–connected web app; users sign transactions in-wallet.
- Runs against the deployed program on **devnet with mock tokens** for the demo. Portfolio assets in the example are **TSLAx** (Tesla) and **NVDAx** (NVIDIA) — xStocks issued on Solana by Backed Finance; mock tokens in tests.
- Prices come from **Pyth Network** `PriceUpdateV2` accounts with a 60-second staleness window; the client must supply fresh price accounts for pricing operations.
- Swaps in the example route through a **test-only mock swap router** (a fake Jupiter) that would be replaced by real [Jupiter](https://jup.ag) in production.
- Baskets beyond ~3 assets exceed a legacy transaction's 1232-byte limit, so the client must send **v0 transactions with an Address Lookup Table**; `deposit` pulls `14 + 5N` accounts and `withdraw` `10 + 4N` for `N` assets.

## Capabilities and Constraints

Program instructions the frontend can surface:

- **Registry / curator:** `initialize_registry`, `approve_asset` (whitelist of assets managers may use).
- **Manager:** `initialize_strategy`, `add_asset`, `set_weight` (including set-to-zero to retire an asset), `rebalance`, `collect_fees`.
- **Depositor:** `deposit` (USDC → minted shares), `withdraw` (burn shares → in-kind slice of every vault).

Rules the UI must respect and reflect:

- Deposits are accepted only when target weights sum to **exactly 10,000 bps**; a strategy is either still being configured or fully allocated and live (`StrategyNotFullyAllocated` otherwise).
- Shares: first deposit is 1:1 with USDC minor units; later deposits mint `deposit_usdc × total_shares / NAV`. Share mint is a PDA owned by the strategy PDA.
- Management fee is charged by minting new shares to the manager (dilution), fixed at creation, capped at `MAX_FEE_BPS` = 1,000 bps (10%), no setter to raise it. `collect_fees` is permissionless.
- Slippage floors are computed on-chain from the Pyth price and `max_slippage_bps` (capped at 1,000 bps); a manager-supplied minimum is not trusted.
- `MAX_ASSETS` = 16. `deposit` re-derives the full `0..asset_count` PDA range and refuses to run if any asset account is missing (`IncompleteAssetAccounts`), so NAV can't be understated.
- Withdrawal is **in-kind**: the user must already hold a token account for each asset; there is no built-in sell-to-cash.

Two program ports exist in the repo — an **Anchor** implementation and a **Quasar** implementation of the same program. The frontend targets the same on-chain behavior regardless of port.

## Brand Commitments

Existing name: **Vault Strategy** (the on-chain construct is a "strategy" built from single-asset "vaults"; the README is deliberate about this distinction). No logo, palette, or visual identity is committed yet. Voice in existing docs is precise, plain, and educational — it defines terms and links canonical references rather than hyping. Future design should not contradict that register.

## Evidence on Hand

- `anchor/README.md` — thorough product and financial-concept documentation (NAV, shares, fees, weights, slippage, in-kind withdrawal, transaction-size limits).
- `VIDEO_SCRIPT.md` at the `vault-strategy` root — an existing narrative walkthrough of the product.
- `anchor/CHANGELOG.md`, `quasar/CHANGELOG.md` — history.
- Program source and tests under `anchor/programs/` and `quasar/`.
- No real users, testimonials, deployment addresses, or production metrics exist; the demo runs on devnet with mock tokens. Future work must not fabricate mainnet deployment claims, real AUM, or user counts.

## Product Principles

1. **The interface shows what the program enforces.** Every number a user sees (NAV, share price, slippage floor, fee) is derived on-chain; surface the real computed values, never a client-side approximation presented as truth.
2. **Teach by operating.** This is a demo whose purpose is comprehension — make each step of the deposit/withdraw and configure/rebalance loops legible as the user performs it.
3. **Respect the state machine.** A strategy is either being configured or live; deposits require exactly-10,000-bps allocation. The UI should make the current phase and what's required to advance it obvious rather than surfacing raw errors.
4. **Distinguish the roles.** Depositor and manager have different jobs, risks, and permissions; do not collapse them into one undifferentiated view.
5. **Devnet honesty.** Present the demo as a demo — mock tokens, devnet, a mock swap router — without dressing it as production.

## Open Decisions

- Whether this frontend ever targets **production/mainnet** with real funds is undecided; recorded here rather than assumed.
- Curator/registry-governance UI is out of first scope; not yet designed.
- Which program port (Anchor vs Quasar) the client library binds to is a later implementation decision.
