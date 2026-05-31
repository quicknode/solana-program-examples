# Quicknode Solana Program Examples

> A fork of the [Solana Foundation program examples](https://github.com/solana-developers/program-examples) with current versions, more [programs](https://solana.com/docs/terminology#program), and additional frameworks.

[![Anchor](../../actions/workflows/anchor.yml/badge.svg)](../../actions/workflows/anchor.yml) [![Quasar](../../actions/workflows/quasar.yml/badge.svg)](../../actions/workflows/quasar.yml) [![Pinocchio](../../actions/workflows/pinocchio.yml/badge.svg)](../../actions/workflows/pinocchio.yml) [![Native](../../actions/workflows/native.yml/badge.svg)](../../actions/workflows/native.yml) [![ASM](../../actions/workflows/solana-asm.yml/badge.svg)](../../actions/workflows/solana-asm.yml)

Each example is available in one or more of the following frameworks:

- [⚓ Anchor](https://www.anchor-lang.com/) — the most popular framework for Solana development. Build with `anchor build`, test with `pnpm test` as defined in `Anchor.toml`.
- [💫 Quasar](https://quasar-lang.com/docs) — a newer, more performant framework with Anchor-compatible ergonomics. Run `pnpm test` to execute tests.
- [🤥 Pinocchio](https://github.com/anza-xyz/pinocchio) — a zero-copy, zero-allocation library for Solana programs. Run `pnpm test` to execute tests.
- [🦀 Native Rust](https://docs.anza.xyz/) — vanilla Rust using Solana's native crates. Run `pnpm test` to execute tests.
- [🧬 ASM](https://github.com/blueshift-gg/sbpf) — hand-written sBPF assembly built with the `sbpf` toolchain. Run `pnpm build-and-test` to build and test.

> [!NOTE]
> You don't need to write your own program for basic tasks like creating [accounts](https://solana.com/docs/terminology#account), transferring SOL, or minting tokens. These are handled by existing programs like the System Program and Token Program.

## Financial Software

### Escrow

**Start here — this is the best first finance program to learn on Solana.** An escrow is a neutral holding account that lets two people who don't trust each other trade safely. One person (the maker) deposits token A and specifies how much token B they want in return. When a second person (the taker) supplies that token B, the program hands each side what it was promised in a single transaction — so either the whole trade happens, or none of it does. This "all-or-nothing" swap is the core idea behind every onchain exchange, which is why it's the right place to begin.

[⚓ Anchor](./finance/escrow/anchor) [💫 Quasar](./finance/escrow/quasar) [🦀 Native](./finance/escrow/native)

### Order Book based Exchange

An order book is the list of buy and sell offers behind most exchanges. Buyers post **bids** (the price they'll pay), sellers post **asks** (the price they'll accept), and a trade happens when a bid and an ask meet. This program implements that idea: traders post limit orders at chosen prices, their tokens are locked in program-controlled vaults, and orders are matched against the opposite side using **price-time priority** (best price first, and for equal prices, whoever was there first). Trading fees collect in a dedicated fee vault, proceeds wait as unsettled balances, and traders withdraw them with `settle_funds`. A minimal teaching example of the mechanics behind exchanges like Openbook and Phoenix.

[⚓ Anchor](./finance/order-book/anchor)

### AMM based Exchange

An automated market maker (AMM) is an exchange with no order book. Instead of matching buyers with sellers, swaps fill instantly against a shared, onchain liquidity pool that other users fund — those users are **liquidity providers**, and they earn a cut of the trading fees in return. Prices are set algorithmically by the pool's balances and move to reflect demand: buying an asset shrinks its share of the pool and pushes its price up. This program lets anyone create a pool, deposit or withdraw liquidity, and swap one token for another, with fees paid to liquidity providers and slippage protection so a swap can't execute at a worse price than expected. This is how exchanges like Raydium and Orca work.

[⚓ Anchor](./finance/token-swap/anchor) [💫 Quasar](./finance/token-swap/quasar)

### Token Fundraiser

A crowdfunding campaign onchain. A creator opens a fundraiser by choosing which token they want to raise and a target amount. Contributors deposit that token into the fundraiser's account until the goal is reached — a simple introduction to collecting funds from many people into a single program-controlled account.

[⚓ Anchor](./finance/token-fundraiser/anchor) [💫 Quasar](./finance/token-fundraiser/quasar)

### Vault Strategy

A managed investment fund onchain. Investors deposit USDC and receive shares representing their slice of the fund. A manager allocates the pooled money across a basket of assets (here, stocks like TSLAx and NVDAx), and each share's value tracks the fund's net asset value — its total holdings divided by the number of shares. The manager earns a management fee over time, and investors withdraw a proportional, in-kind slice of the underlying assets. Demonstrates share minting, value-per-share pricing, fee accrual, and calling another program (CPI) to swap assets.

[⚓ Anchor](./finance/vault-strategy/anchor)

### Perpetual Futures

A perpetual futures exchange — a venue for making leveraged bets on an asset's price without ever owning the asset. Traders post collateral and open a **long** (betting the price rises) or **short** (betting it falls) sized up to several times their collateral; their profit or loss tracks the price move and is paid in the collateral token. Rather than matching buyers to sellers, every trade is against a shared **liquidity pool** that other users fund and that is the counterparty to all of it — the pool pays winners and keeps losers' collateral, and its providers earn the trading and funding fees in return. The price comes from an oracle, positions accrue a funding fee over time, and anyone can **liquidate** a position whose collateral can no longer cover its loss. This is the design behind venues like Jupiter Perpetuals and GMX.

[⚓ Anchor](./finance/perpetual-futures/anchor) [💫 Quasar](./finance/perpetual-futures/quasar)

## Single concept examples

### Hello Solana

A minimal program that logs a greeting.

[⚓ Anchor](./basics/hello-solana/anchor) [💫 Quasar](./basics/hello-solana/quasar) [🤥 Pinocchio](./basics/hello-solana/pinocchio) [🦀 Native](./basics/hello-solana/native) [🧬 ASM](./basics/hello-solana/asm)

### Account Data

Store and retrieve data using Solana accounts.

[⚓ Anchor](./basics/account-data/anchor) [💫 Quasar](./basics/account-data/quasar) [🤥 Pinocchio](./basics/account-data/pinocchio) [🦀 Native](./basics/account-data/native)

### Counter

Use a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) to store global state — a counter that increments when called.

[⚓ Anchor](./basics/counter/anchor) [💫 Quasar](./basics/counter/quasar) [🤥 Pinocchio](./basics/counter/pinocchio) [🦀 Native](./basics/counter/native)

### Favorites

Save and update per-user state, ensuring users can only modify their own data.

[⚓ Anchor](./basics/favorites/anchor) [💫 Quasar](./basics/favorites/quasar) [🤥 Pinocchio](./basics/favorites/pinocchio) [🦀 Native](./basics/favorites/native)

### Checking Accounts

Validate that accounts provided in incoming [instructions](https://solana.com/docs/terminology#instruction) meet specific criteria.

[⚓ Anchor](./basics/checking-accounts/anchor) [💫 Quasar](./basics/checking-accounts/quasar) [🤥 Pinocchio](./basics/checking-accounts/pinocchio) [🦀 Native](./basics/checking-accounts/native) [🧬 ASM](./basics/checking-accounts/asm)

### Close Account

Close an account and reclaim its [lamports](https://solana.com/docs/terminology#lamport).

[⚓ Anchor](./basics/close-account/anchor) [💫 Quasar](./basics/close-account/quasar) [🤥 Pinocchio](./basics/close-account/pinocchio) [🦀 Native](./basics/close-account/native)

### Create Account

Create new accounts on the blockchain.

[⚓ Anchor](./basics/create-account/anchor) [💫 Quasar](./basics/create-account/quasar) [🤥 Pinocchio](./basics/create-account/pinocchio) [🦀 Native](./basics/create-account/native) [🧬 ASM](./basics/create-account/asm)

### Cross-Program Invocation

Call one program from another — the hand program invokes the lever program to toggle a switch.

[⚓ Anchor](./basics/cross-program-invocation/anchor) [💫 Quasar](./basics/cross-program-invocation/quasar) [🦀 Native](./basics/cross-program-invocation/native)

### PDA Rent Payer

Use a PDA to pay [rent](https://solana.com/docs/terminology#rent) for creating a new account.

[⚓ Anchor](./basics/pda-rent-payer/anchor) [💫 Quasar](./basics/pda-rent-payer/quasar) [🤥 Pinocchio](./basics/pda-rent-payer/pinocchio) [🦀 Native](./basics/pda-rent-payer/native)

### Processing Instructions

Add parameters to an [instruction handler](https://solana.com/docs/terminology#instruction-handler) and use them.

[⚓ Anchor](./basics/processing-instructions/anchor) [💫 Quasar](./basics/processing-instructions/quasar) [🤥 Pinocchio](./basics/processing-instructions/pinocchio) [🦀 Native](./basics/processing-instructions/native)

### Program Derived Addresses

Store and retrieve state using PDAs as deterministic account addresses.

[⚓ Anchor](./basics/program-derived-addresses/anchor) [💫 Quasar](./basics/program-derived-addresses/quasar) [🤥 Pinocchio](./basics/program-derived-addresses/pinocchio) [🦀 Native](./basics/program-derived-addresses/native)

### Realloc

Handle accounts that need to grow or shrink in size.

[⚓ Anchor](./basics/realloc/anchor) [💫 Quasar](./basics/realloc/quasar) [🤥 Pinocchio](./basics/realloc/pinocchio) [🦀 Native](./basics/realloc/native)

### Rent

Calculate an account's size to determine the minimum rent-exempt balance.

[⚓ Anchor](./basics/rent/anchor) [💫 Quasar](./basics/rent/quasar) [🤥 Pinocchio](./basics/rent/pinocchio) [🦀 Native](./basics/rent/native)

### Repository Layout

Structure a larger Solana program across multiple files and modules.

[⚓ Anchor](./basics/repository-layout/anchor) [💫 Quasar](./basics/repository-layout/quasar) [🦀 Native](./basics/repository-layout/native)

### Transfer SOL

Send SOL between two accounts.

[⚓ Anchor](./basics/transfer-sol/anchor) [💫 Quasar](./basics/transfer-sol/quasar) [🤥 Pinocchio](./basics/transfer-sol/pinocchio) [🦀 Native](./basics/transfer-sol/native) [🧬 ASM](./basics/transfer-sol/asm)

### Pyth Price Feeds

Finance programs often need real-world prices — what a dollar, a stock, or another token is worth right now. An **oracle** brings that offchain market data [onchain](https://solana.com/docs/terminology#onchain). [Pyth](https://pyth.network/) is an oracle that publishes low-latency prices from institutional sources, with each asset's price living in its own account called a price feed. This minimal example reads a feed and logs its price, confidence interval, and exponent — the building block a program like an AMM, a lending market, or a vault uses to value assets.

[⚓ Anchor](./basics/pyth/anchor) [💫 Quasar](./basics/pyth/quasar)

## Tokens

### Create Token

Create a token mint with a symbol and icon.

[⚓ Anchor](./tokens/create-token/anchor) [💫 Quasar](./tokens/create-token/quasar) [🦀 Native](./tokens/create-token/native)

### Mint NFT

Mint an NFT from inside your own program using the Token and Metaplex Token Metadata programs.

[⚓ Anchor](./tokens/nft-minter/anchor) [💫 Quasar](./tokens/nft-minter/quasar) [🦀 Native](./tokens/nft-minter/native)

### NFT Operations

Create an NFT collection, mint NFTs, and verify NFTs as part of a collection using Metaplex Token Metadata.

[⚓ Anchor](./tokens/nft-operations/anchor) [💫 Quasar](./tokens/nft-operations/quasar)

### Token Minter

Mint tokens from inside your own program using the [Classic Token Program](https://solana.com/docs/terminology#token-program).

[⚓ Anchor](./tokens/token-minter/anchor) [💫 Quasar](./tokens/token-minter/quasar) [🦀 Native](./tokens/token-minter/native)

### Transfer Tokens

Transfer tokens between accounts.

[⚓ Anchor](./tokens/transfer-tokens/anchor) [💫 Quasar](./tokens/transfer-tokens/quasar) [🦀 Native](./tokens/transfer-tokens/native)

### PDA Mint Authority

Mint tokens using a PDA as the mint authority, so your program controls token issuance.

[⚓ Anchor](./tokens/pda-mint-authority/anchor) [💫 Quasar](./tokens/pda-mint-authority/quasar) [🦀 Native](./tokens/pda-mint-authority/native)

### External Delegate Token Master

Control token transfers using an external secp256k1 delegate signature.

[⚓ Anchor](./tokens/external-delegate-token-master/anchor) [💫 Quasar](./tokens/external-delegate-token-master/quasar)

## Token Extensions

### Basics

Create token mints, mint tokens, and transfer tokens using [Token Extensions](https://solana.com/docs/terminology#token-extensions-program).

[⚓ Anchor](./tokens/token-extensions/basics/anchor) [💫 Quasar](./tokens/token-extensions/basics/quasar)

### CPI Guard

Prevent certain token actions from occurring within [cross-program invocations](https://solana.com/docs/terminology#cross-program-invocation-cpi).

[⚓ Anchor](./tokens/token-extensions/cpi-guard/anchor) [💫 Quasar](./tokens/token-extensions/cpi-guard/quasar)

### Default Account State

Create new [token accounts](https://solana.com/docs/terminology#token-account) that are frozen by default.

[⚓ Anchor](./tokens/token-extensions/default-account-state/anchor) [💫 Quasar](./tokens/token-extensions/default-account-state/quasar) [🦀 Native](./tokens/token-extensions/default-account-state/native)

### Group Pointer

Create tokens that belong to larger groups using the Group Pointer extension.

[⚓ Anchor](./tokens/token-extensions/group/anchor) [💫 Quasar](./tokens/token-extensions/group/quasar)

### Immutable Owner

Create token accounts whose owning program cannot be changed.

[⚓ Anchor](./tokens/token-extensions/immutable-owner/anchor) [💫 Quasar](./tokens/token-extensions/immutable-owner/quasar)

### Interest Bearing Tokens

Create tokens that show an interest calculation, updating their displayed balance over time.

[⚓ Anchor](./tokens/token-extensions/interest-bearing/anchor) [💫 Quasar](./tokens/token-extensions/interest-bearing/quasar)

### Memo Transfer

Require all transfers to include a descriptive memo.

[⚓ Anchor](./tokens/token-extensions/memo-transfer/anchor) [💫 Quasar](./tokens/token-extensions/memo-transfer/quasar)

### Onchain Metadata

Store metadata directly inside the token [mint account](https://solana.com/docs/terminology#token-mint), without needing additional programs.

[⚓ Anchor](./tokens/token-extensions/metadata/anchor)

### NFT Metadata Pointer

Create an NFT using the metadata pointer extension, storing onchain metadata (including custom fields) inside the mint.

[⚓ Anchor](./tokens/token-extensions/nft-meta-data-pointer/anchor-example/anchor)

### Mint Close Authority

Allow a designated account to close a token mint.

[⚓ Anchor](./tokens/token-extensions/mint-close-authority/anchor) [💫 Quasar](./tokens/token-extensions/mint-close-authority/quasar) [🦀 Native](./tokens/token-extensions/mint-close-authority/native)

### Multiple Extensions

Use multiple Token Extensions on a single mint at once.

[🦀 Native](./tokens/token-extensions/multiple-extensions/native)

### Non-Transferable Tokens

Create tokens that cannot be transferred between accounts.

[⚓ Anchor](./tokens/token-extensions/non-transferable/anchor) [💫 Quasar](./tokens/token-extensions/non-transferable/quasar) [🦀 Native](./tokens/token-extensions/non-transferable/native)

### Permanent Delegate

Create tokens that remain under the control of a designated account, even when transferred elsewhere.

[⚓ Anchor](./tokens/token-extensions/permanent-delegate/anchor) [💫 Quasar](./tokens/token-extensions/permanent-delegate/quasar)

### Transfer Fee

Create tokens with a built-in transfer fee.

[⚓ Anchor](./tokens/token-extensions/transfer-fee/anchor) [💫 Quasar](./tokens/token-extensions/transfer-fee/quasar) [🦀 Native](./tokens/token-extensions/transfer-fee/native)

### Transfer Hook — Hello World

A minimal transfer hook that executes custom logic on every token transfer.

[⚓ Anchor](./tokens/token-extensions/transfer-hook/hello-world/anchor) [💫 Quasar](./tokens/token-extensions/transfer-hook/hello-world/quasar)

### Transfer Hook — Counter

Count how many times tokens have been transferred.

[⚓ Anchor](./tokens/token-extensions/transfer-hook/counter/anchor) [💫 Quasar](./tokens/token-extensions/transfer-hook/counter/quasar)

### Transfer Hook — Account Data as Seed

Use token account owner data as seeds to derive extra accounts in a transfer hook.

[⚓ Anchor](./tokens/token-extensions/transfer-hook/account-data-as-seed/anchor) [💫 Quasar](./tokens/token-extensions/transfer-hook/account-data-as-seed/quasar)

### Transfer Hook — Allow/Block List

Restrict or allow token transfers using an onchain list managed by a list authority.

[⚓ Anchor](./tokens/token-extensions/transfer-hook/allow-block-list-token/anchor) [💫 Quasar](./tokens/token-extensions/transfer-hook/allow-block-list-token/quasar)

### Transfer Hook — Transfer Cost

Charge an additional fee on every token transfer.

[⚓ Anchor](./tokens/token-extensions/transfer-hook/transfer-cost/anchor) [💫 Quasar](./tokens/token-extensions/transfer-hook/transfer-cost/quasar)

### Transfer Hook — Transfer Switch

Enable or disable token transfers with an onchain switch.

[⚓ Anchor](./tokens/token-extensions/transfer-hook/transfer-switch/anchor) [💫 Quasar](./tokens/token-extensions/transfer-hook/transfer-switch/quasar)

### Transfer Hook — Whitelist

Restrict transfers so only whitelisted accounts can receive tokens.

[⚓ Anchor](./tokens/token-extensions/transfer-hook/whitelist/anchor) [💫 Quasar](./tokens/token-extensions/transfer-hook/whitelist/quasar)

## Compression

### cNFT Burn

Burn compressed NFTs.

[⚓ Anchor](./compression/cnft-burn/anchor) [💫 Quasar](./compression/cnft-burn/quasar)

### cNFT Vault

Store Metaplex compressed NFTs inside a PDA.

[⚓ Anchor](./compression/cnft-vault/anchor) [💫 Quasar](./compression/cnft-vault/quasar)

### Compression Utilities

Work with Metaplex compressed NFTs.

[⚓ Anchor](./compression/cutils/anchor) [💫 Quasar](./compression/cutils/quasar)

---

**PRs welcome!** Follow the [contributing guidelines](./CONTRIBUTING.md) to keep things consistent.
