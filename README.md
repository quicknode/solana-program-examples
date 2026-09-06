# Solana Program Examples

![Quicknode Solana Program Examples](assets/banner.png?v=1)

_Solana program examples ('smart contracts') in Anchor 2, Anchor 1, Quasar, Pinocchio, native Rust, and sBPF assembly. Focused on financial software, plus the basics, tokens, Token Extensions, state compression, and more._

Working, tested, up-to-date examples of common Solana programs (what other chains call smart contracts), maintained by [Quicknode](https://www.quicknode.com/chains/solana). Current as of August 2026 (see [CHANGELOG.md](./CHANGELOG.md)): every example builds and passes CI on **Anchor 2.0.0-rc.1**, the current multi-file program layout (one file per instruction handler, account type, etc), and [LiteSVM](https://github.com/LiteSVM/litesvm) tests rather than the older `solana-test-validator` / web3.js stack. Every Anchor example also keeps its last **Anchor 1.1.2** version in a sibling `anchor-v1/` directory, built and tested by its own CI job, for programs staying on the Anchor v1 LTS line.

[![Anchor v2](../../actions/workflows/anchor.yml/badge.svg)](../../actions/workflows/anchor.yml) [![Anchor v1](../../actions/workflows/anchor-v1.yml/badge.svg)](../../actions/workflows/anchor-v1.yml) [![Quasar](../../actions/workflows/quasar.yml/badge.svg)](../../actions/workflows/quasar.yml) [![Pinocchio](../../actions/workflows/pinocchio.yml/badge.svg)](../../actions/workflows/pinocchio.yml) [![Native](../../actions/workflows/native.yml/badge.svg)](../../actions/workflows/native.yml) [![ASM](../../actions/workflows/solana-asm.yml/badge.svg)](../../actions/workflows/solana-asm.yml)

Each example is available in one or more of the following frameworks:

- [⚓ Anchor v2](https://www.anchor-lang.com/) - the most popular framework for Solana development, in its current major version. Lives in `anchor/`. Install with `cargo install anchor-cli --version 2.0.0-rc.1 --locked`, build with `anchor build`, test with `anchor test`.
- [⚓ Anchor v1](https://www.anchor-lang.com/) - the previous major version, on long-term support and still what many deployed programs use. Lives in `anchor-v1/`, alongside the v2 copy of the same example. Install with `avm install 1.1.2 && avm use 1.1.2`, then the same `anchor build` / `anchor test`.
- [💫 Quasar](https://quasar-lang.com/docs) - a newer, more performant framework with Anchor-compatible ergonomics. Build with `quasar build`, test with `quasar test`.
- [🤥 Pinocchio](https://github.com/anza-xyz/pinocchio) - a zero-copy, zero-allocation library for Solana programs. Build with `cargo build-sbf --manifest-path=./program/Cargo.toml`, test with `cargo test --manifest-path=./program/Cargo.toml`.
- [🦀 Native Rust](https://docs.anza.xyz/) - vanilla Rust using Solana's native crates. Build with `cargo build-sbf --manifest-path=./program/Cargo.toml`, test with `cargo test --manifest-path=./program/Cargo.toml`.
- [🧬 ASM](https://github.com/blueshift-gg/sbpf) - hand-written sBPF assembly built with the `sbpf` toolchain. Build with `sbpf build`, test with `cargo test`.

> [!NOTE]
> You don't need to write your own program for basic tasks like creating [accounts](https://solana.com/docs/terminology#account), transferring SOL, or minting tokens. These are handled by existing programs like the System Program and Token Program.

## Getting started

You need [Rust](https://www.rust-lang.org/tools/install), [Solana CLI](https://docs.anza.xyz/cli/install), [Anchor](https://www.anchor-lang.com/docs/installation), and [pnpm](https://pnpm.io/installation) installed. Clone the repo and `cd` into any example directory, then run its tests with the command for that framework (shown above) - for an Anchor example, `anchor test`. `pnpm` is used for repo-wide formatting and linting, not for running an example's tests.

To deploy to mainnet or devnet you'll need an RPC endpoint. [Quicknode](https://www.quicknode.com/chains/solana) provides free and paid Solana endpoints - create one and set it as your cluster in `Anchor.toml` or with `solana config set --url <your-endpoint>`.

## Financial software ("DeFi")

The programs are examples of common financial primitives on Solana. As well as tests these all have [formal verification using Kani](https://github.com/model-checking/kani). Every finance program ships with proofs that verify its money-math invariants exhaustively over all inputs. See each program's `kani-proofs/` directory for the harnesses and what they prove.

### Escrow

**Start here - the best first finance program to learn on Solana.** A neutral account that holds funds until both sides deliver, like a real-estate escrow or a lawyer's trust account. The maker deposits token A and names how much token B they want; when a taker supplies token B, the program swaps both in a single all-or-nothing transaction. This swap is the core idea behind every onchain exchange.

[⚓ Anchor v2](./finance/escrow/anchor) [⚓ Anchor v1](./finance/escrow/anchor-v1) [💫 Quasar](./finance/escrow/quasar) [🦀 Native](./finance/escrow/native)

🎬 Video: [Build a Solana program (smart contract) in 30 minutes](https://www.youtube.com/watch?v=B5eBWWQfQuM)

### Lending

A borrow/lend market like Solend or Kamino: suppliers deposit a token and receive share tokens whose exchange rate rises as borrowers pay interest, borrowers post those shares as collateral to draw a different token against it up to a loan-to-value limit, and liquidators close part of any position that crosses its health threshold. Interest accrues through a utilization-based rate curve and a cumulative index, so no per-account accrual loop is needed.

[⚓ Anchor v2](./finance/lending/anchor) [⚓ Anchor v1](./finance/lending/anchor-v1) [💫 Quasar](./finance/lending/quasar)

### Order Book based Exchange

A typical NYSE/NASDAQ-style order book-based exchange. Buyers post **bids** (the price they'll pay), sellers post **asks** (the price they'll accept), and a trade happens when a bid and an ask meet. The exchange operator collects fees from trading. Similar to popular Solana exchanges like Openbook and Phoenix.

[⚓ Anchor v2](./finance/order-book/anchor) [⚓ Anchor v1](./finance/order-book/anchor-v1) [💫 Quasar](./finance/order-book/quasar)

🎬 Video: [How to make a crypto exchange on Solana](https://www.youtube.com/watch?v=ioFkpaKHXgg)

### AMM based Exchange

An exchange with no order book: swaps fill instantly against a shared liquidity pool funded by **liquidity providers**, who earn a cut of the trading fees. Prices are set algorithmically by the pool's balances. Anyone can create a pool, add or remove liquidity, and swap tokens, with slippage protection on every trade. Similar to Solana exchanges like Raydium and Orca.

[⚓ Anchor v2](./finance/token-swap/anchor) [⚓ Anchor v1](./finance/token-swap/anchor-v1) [💫 Quasar](./finance/token-swap/quasar)

### Prop AMM

A **proprietary AMM**: a market-making firm funds a venue with its own capital and quotes both sides of it, selling the base token at the oracle price plus a spread and buying it back at the oracle price minus the spread. No pricing curve, no liquidity providers, no pool shares: the operator is the only capital in the market, can re-quote or pull its quotes at will, and earns the spread instead of a fee. Because the price comes from an oracle rather than the pool's balances, trades have no price impact and nothing to sandwich. This is the design behind venues like Lifinity, SolFi, and HumidiFi, which fill most Solana swap volume through Jupiter routing.

[⚓ Anchor v2](./finance/prop-amm/anchor) [⚓ Anchor v1](./finance/prop-amm/anchor-v1) [💫 Quasar](./finance/prop-amm/quasar)

### Vault Strategy

A managed investment fund onchain, like an ETF or mutual fund. Investors deposit USDC for shares, a manager allocates the pool across a basket of assets (here, stocks like TSLAx and NVDAx), and each share's value tracks the fund's net asset value. The manager earns a management fee, and investors redeem a proportional slice of the underlying assets.

[⚓ Anchor v2](./finance/vault-strategy/anchor) [⚓ Anchor v1](./finance/vault-strategy/anchor-v1) [💫 Quasar](./finance/vault-strategy/quasar)

### Betting Market

Parimutuel (pooled) prediction market - an admin opens an event with multiple outcomes, bettors stake tokens on an outcome, and at settlement the losing pool (minus a protocol fee) is split among winners in proportion to their stake.

[⚓ Anchor v2](./finance/betting-market/anchor) [⚓ Anchor v1](./finance/betting-market/anchor-v1) [💫 Quasar](./finance/betting-market/quasar)

🎬 Video: [How to build a PolyMarket/Kalshi style betting market on Solana](https://www.youtube.com/watch?v=jE3-IA1FBs0)


### Options

A fully collateralized, physically settled **options** venue. A writer posts the whole of what a holder could ever claim (the underlying for a **call**, the strike in USDC for a **put**) and lists the option at a premium; a buyer pays the premium and becomes the holder; the holder may **exercise** at any time before expiry, paying the strike for the underlying (or delivering the underlying for the strike); after expiry the writer reclaims whatever was not exercised. Because the collateral is always in the vault, no position can be under water, so there is no margin, no liquidator, and no oracle. The venue earns a fee on every premium.

[⚓ Anchor v2](./finance/options/anchor) [⚓ Anchor v1](./finance/options/anchor-v1) [💫 Quasar](./finance/options/quasar)

### Perpetual Futures

A perpetual futures exchange: a venue for making leveraged bets on an asset's price without ever owning the asset. Traders post collateral and open a **long** (betting the price rises) or **short** (betting it falls) sized up to several times their collateral; their profit or loss tracks the price move and is paid in the collateral token. Rather than matching buyers to sellers, every trade is against a shared **liquidity pool** that other users fund and that is the counterparty to all of it: the pool pays winners and keeps losers' collateral, and its providers earn the trading and funding fees in return. The price comes from an oracle, positions accrue a funding fee over time, and anyone can **liquidate** a position whose collateral can no longer cover its loss. This is the design behind venues like Jupiter Perpetuals and GMX.

[⚓ Anchor v2](./finance/perpetual-futures/anchor) [⚓ Anchor v1](./finance/perpetual-futures/anchor-v1) [💫 Quasar](./finance/perpetual-futures/quasar)

### Token Fundraiser

Onchain crowdfunding, like Kickstarter or GoFundMe. A creator sets a target amount in a chosen token, and contributors deposit into the fundraiser's account until the goal is reached.

[⚓ Anchor v2](./finance/token-fundraiser/anchor) [⚓ Anchor v1](./finance/token-fundraiser/anchor-v1) [💫 Quasar](./finance/token-fundraiser/quasar)

## Single concept examples

### Hello Solana

A minimal program that logs a greeting.

[⚓ Anchor v2](./basics/hello-solana/anchor) [⚓ Anchor v1](./basics/hello-solana/anchor-v1) [💫 Quasar](./basics/hello-solana/quasar) [🤥 Pinocchio](./basics/hello-solana/pinocchio) [🦀 Native](./basics/hello-solana/native) [🧬 ASM](./basics/hello-solana/asm)

### Account Data

Store and retrieve data using Solana accounts.

[⚓ Anchor v2](./basics/account-data/anchor) [⚓ Anchor v1](./basics/account-data/anchor-v1) [💫 Quasar](./basics/account-data/quasar) [🤥 Pinocchio](./basics/account-data/pinocchio) [🦀 Native](./basics/account-data/native)

### Counter

Use a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) to store global state - a counter that increments when called.

[⚓ Anchor v2](./basics/counter/anchor) [⚓ Anchor v1](./basics/counter/anchor-v1) [💫 Quasar](./basics/counter/quasar) [🤥 Pinocchio](./basics/counter/pinocchio) [🦀 Native](./basics/counter/native)

### Favorites

Save and update per-user state, ensuring users can only modify their own data.

[⚓ Anchor v2](./basics/favorites/anchor) [⚓ Anchor v1](./basics/favorites/anchor-v1) [💫 Quasar](./basics/favorites/quasar) [🤥 Pinocchio](./basics/favorites/pinocchio) [🦀 Native](./basics/favorites/native)

### Checking Accounts

Validate that accounts provided in incoming [instructions](https://solana.com/docs/terminology#instruction) meet specific criteria.

[⚓ Anchor v2](./basics/checking-accounts/anchor) [⚓ Anchor v1](./basics/checking-accounts/anchor-v1) [💫 Quasar](./basics/checking-accounts/quasar) [🤥 Pinocchio](./basics/checking-accounts/pinocchio) [🦀 Native](./basics/checking-accounts/native) [🧬 ASM](./basics/checking-accounts/asm)

### Close Account

Close an account and reclaim its [lamports](https://solana.com/docs/terminology#lamport).

[⚓ Anchor v2](./basics/close-account/anchor) [⚓ Anchor v1](./basics/close-account/anchor-v1) [💫 Quasar](./basics/close-account/quasar) [🤥 Pinocchio](./basics/close-account/pinocchio) [🦀 Native](./basics/close-account/native)

### Create Account

Create new accounts on the blockchain.

[⚓ Anchor v2](./basics/create-account/anchor) [⚓ Anchor v1](./basics/create-account/anchor-v1) [💫 Quasar](./basics/create-account/quasar) [🤥 Pinocchio](./basics/create-account/pinocchio) [🦀 Native](./basics/create-account/native) [🧬 ASM](./basics/create-account/asm)

### Cross-Program Invocation

Call one program from another - the hand program invokes the lever program to toggle a switch.

[⚓ Anchor v2](./basics/cross-program-invocation/anchor) [⚓ Anchor v1](./basics/cross-program-invocation/anchor-v1) [💫 Quasar](./basics/cross-program-invocation/quasar) [🦀 Native](./basics/cross-program-invocation/native)

### PDA Rent Payer

Use a PDA to pay [rent](https://solana.com/docs/terminology#rent) for creating a new account.

[⚓ Anchor v2](./basics/pda-rent-payer/anchor) [⚓ Anchor v1](./basics/pda-rent-payer/anchor-v1) [💫 Quasar](./basics/pda-rent-payer/quasar) [🤥 Pinocchio](./basics/pda-rent-payer/pinocchio) [🦀 Native](./basics/pda-rent-payer/native)

### Processing Instructions

Add parameters to an [instruction handler](https://solana.com/docs/terminology#instruction-handler) and use them.

[⚓ Anchor v2](./basics/processing-instructions/anchor) [⚓ Anchor v1](./basics/processing-instructions/anchor-v1) [💫 Quasar](./basics/processing-instructions/quasar) [🤥 Pinocchio](./basics/processing-instructions/pinocchio) [🦀 Native](./basics/processing-instructions/native)

### Program Derived Addresses

Store and retrieve state using PDAs as deterministic account addresses.

[⚓ Anchor v2](./basics/program-derived-addresses/anchor) [⚓ Anchor v1](./basics/program-derived-addresses/anchor-v1) [💫 Quasar](./basics/program-derived-addresses/quasar) [🤥 Pinocchio](./basics/program-derived-addresses/pinocchio) [🦀 Native](./basics/program-derived-addresses/native)

### Realloc

Handle accounts that need to grow or shrink in size.

[⚓ Anchor v2](./basics/realloc/anchor) [⚓ Anchor v1](./basics/realloc/anchor-v1) [💫 Quasar](./basics/realloc/quasar) [🤥 Pinocchio](./basics/realloc/pinocchio) [🦀 Native](./basics/realloc/native)

### Rent

Calculate an account's size to determine the minimum rent-exempt balance.

[⚓ Anchor v2](./basics/rent/anchor) [⚓ Anchor v1](./basics/rent/anchor-v1) [💫 Quasar](./basics/rent/quasar) [🤥 Pinocchio](./basics/rent/pinocchio) [🦀 Native](./basics/rent/native)

### Repository Layout

Structure a larger Solana program across multiple files and modules.

[⚓ Anchor v2](./basics/repository-layout/anchor) [⚓ Anchor v1](./basics/repository-layout/anchor-v1) [💫 Quasar](./basics/repository-layout/quasar) [🦀 Native](./basics/repository-layout/native)

### Transfer SOL

Send SOL between two accounts.

[⚓ Anchor v2](./basics/transfer-sol/anchor) [⚓ Anchor v1](./basics/transfer-sol/anchor-v1) [💫 Quasar](./basics/transfer-sol/quasar) [🤥 Pinocchio](./basics/transfer-sol/pinocchio) [🦀 Native](./basics/transfer-sol/native) [🧬 ASM](./basics/transfer-sol/asm)

### Pyth Price Feeds

An **oracle** brings real-world market prices - a dollar, a stock, a token - [onchain](https://solana.com/docs/terminology#onchain), like a Bloomberg terminal feeding live quotes. [Pyth](https://pyth.network/) publishes low-latency prices from institutional sources, each in its own price feed account. This example reads a feed and logs its price, confidence interval, and exponent - the building block an AMM, lending market, or vault uses to value assets.

[⚓ Anchor v2](./basics/pyth/anchor) [⚓ Anchor v1](./basics/pyth/anchor-v1) [💫 Quasar](./basics/pyth/quasar)

## Tokens

### Create Token

Create a token mint with a symbol and icon.

[⚓ Anchor v2](./tokens/create-token/anchor) [⚓ Anchor v1](./tokens/create-token/anchor-v1) [💫 Quasar](./tokens/create-token/quasar) [🦀 Native](./tokens/create-token/native)

### Mint NFT

Mint an NFT from inside your own program using the Token and Metaplex Token Metadata programs.

[⚓ Anchor v2](./tokens/nft-minter/anchor) [⚓ Anchor v1](./tokens/nft-minter/anchor-v1) [💫 Quasar](./tokens/nft-minter/quasar) [🦀 Native](./tokens/nft-minter/native)

### NFT Operations

Create an NFT collection, mint NFTs, and verify NFTs as part of a collection using Metaplex Token Metadata.

[⚓ Anchor v2](./tokens/nft-operations/anchor) [⚓ Anchor v1](./tokens/nft-operations/anchor-v1) [💫 Quasar](./tokens/nft-operations/quasar)

### Token Minter

Mint tokens from inside your own program using the [Classic Token Program](https://solana.com/docs/terminology#token-program).

[⚓ Anchor v2](./tokens/token-minter/anchor) [⚓ Anchor v1](./tokens/token-minter/anchor-v1) [💫 Quasar](./tokens/token-minter/quasar) [🦀 Native](./tokens/token-minter/native)

### Transfer Tokens

Transfer tokens between accounts.

[⚓ Anchor v2](./tokens/transfer-tokens/anchor) [⚓ Anchor v1](./tokens/transfer-tokens/anchor-v1) [💫 Quasar](./tokens/transfer-tokens/quasar) [🦀 Native](./tokens/transfer-tokens/native)

### PDA Mint Authority

Mint tokens using a PDA as the mint authority, so your program controls token issuance.

[⚓ Anchor v2](./tokens/pda-mint-authority/anchor) [⚓ Anchor v1](./tokens/pda-mint-authority/anchor-v1) [💫 Quasar](./tokens/pda-mint-authority/quasar) [🦀 Native](./tokens/pda-mint-authority/native)

### External Delegate Token Master

Control token transfers using an external secp256k1 delegate signature.

[⚓ Anchor v2](./tokens/external-delegate-token-master/anchor) [⚓ Anchor v1](./tokens/external-delegate-token-master/anchor-v1) [💫 Quasar](./tokens/external-delegate-token-master/quasar)

## Token Extensions

### Basics

Create token mints, mint tokens, and transfer tokens using [Token Extensions](https://solana.com/docs/terminology#token-extensions-program).

[⚓ Anchor v2](./tokens/token-extensions/basics/anchor) [⚓ Anchor v1](./tokens/token-extensions/basics/anchor-v1) [💫 Quasar](./tokens/token-extensions/basics/quasar)

### CPI Guard

Prevent certain token actions from occurring within [cross-program invocations](https://solana.com/docs/terminology#cross-program-invocation-cpi).

[⚓ Anchor v2](./tokens/token-extensions/cpi-guard/anchor) [⚓ Anchor v1](./tokens/token-extensions/cpi-guard/anchor-v1) [💫 Quasar](./tokens/token-extensions/cpi-guard/quasar)

### Default Account State

Create new [token accounts](https://solana.com/docs/terminology#token-account) that are frozen by default.

[⚓ Anchor v2](./tokens/token-extensions/default-account-state/anchor) [⚓ Anchor v1](./tokens/token-extensions/default-account-state/anchor-v1) [💫 Quasar](./tokens/token-extensions/default-account-state/quasar) [🦀 Native](./tokens/token-extensions/default-account-state/native)

### Group Pointer

Create tokens that belong to larger groups using the Group Pointer extension.

[⚓ Anchor v2](./tokens/token-extensions/group/anchor) [⚓ Anchor v1](./tokens/token-extensions/group/anchor-v1) [💫 Quasar](./tokens/token-extensions/group/quasar)

### Immutable Owner

Create token accounts whose owning program cannot be changed.

[⚓ Anchor v2](./tokens/token-extensions/immutable-owner/anchor) [⚓ Anchor v1](./tokens/token-extensions/immutable-owner/anchor-v1) [💫 Quasar](./tokens/token-extensions/immutable-owner/quasar)

### Interest Bearing Tokens

Create tokens that show an interest calculation, updating their displayed balance over time.

[⚓ Anchor v2](./tokens/token-extensions/interest-bearing/anchor) [⚓ Anchor v1](./tokens/token-extensions/interest-bearing/anchor-v1) [💫 Quasar](./tokens/token-extensions/interest-bearing/quasar)

### Memo Transfer

Require all transfers to include a descriptive memo.

[⚓ Anchor v2](./tokens/token-extensions/memo-transfer/anchor) [⚓ Anchor v1](./tokens/token-extensions/memo-transfer/anchor-v1) [💫 Quasar](./tokens/token-extensions/memo-transfer/quasar)

### Onchain Metadata

Store metadata directly inside the token [mint account](https://solana.com/docs/terminology#token-mint), without needing additional programs.

[⚓ Anchor v2](./tokens/token-extensions/metadata/anchor) [⚓ Anchor v1](./tokens/token-extensions/metadata/anchor-v1)

### NFT Metadata Pointer

Create an NFT using the metadata pointer extension, storing onchain metadata (including custom fields) inside the mint.

[⚓ Anchor v2](./tokens/token-extensions/nft-meta-data-pointer/anchor-example/anchor) [⚓ Anchor v1](./tokens/token-extensions/nft-meta-data-pointer/anchor-example/anchor-v1)

### Mint Close Authority

Allow a designated account to close a token mint.

[⚓ Anchor v2](./tokens/token-extensions/mint-close-authority/anchor) [⚓ Anchor v1](./tokens/token-extensions/mint-close-authority/anchor-v1) [💫 Quasar](./tokens/token-extensions/mint-close-authority/quasar) [🦀 Native](./tokens/token-extensions/mint-close-authority/native)

### Multiple Extensions

Use multiple Token Extensions on a single mint at once.

[🦀 Native](./tokens/token-extensions/multiple-extensions/native)

### Non-Transferable Tokens

Create tokens that cannot be transferred between accounts.

[⚓ Anchor v2](./tokens/token-extensions/non-transferable/anchor) [⚓ Anchor v1](./tokens/token-extensions/non-transferable/anchor-v1) [💫 Quasar](./tokens/token-extensions/non-transferable/quasar) [🦀 Native](./tokens/token-extensions/non-transferable/native)

### Permanent Delegate

Create tokens that remain under the control of a designated account, even when transferred elsewhere.

[⚓ Anchor v2](./tokens/token-extensions/permanent-delegate/anchor) [⚓ Anchor v1](./tokens/token-extensions/permanent-delegate/anchor-v1) [💫 Quasar](./tokens/token-extensions/permanent-delegate/quasar)

### Transfer Fee

Create tokens with a built-in transfer fee.

[⚓ Anchor v2](./tokens/token-extensions/transfer-fee/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-fee/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-fee/quasar) [🦀 Native](./tokens/token-extensions/transfer-fee/native)

### Transfer Hook - Hello World

A minimal transfer hook that executes custom logic on every token transfer.

[⚓ Anchor v2](./tokens/token-extensions/transfer-hook/hello-world/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-hook/hello-world/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-hook/hello-world/quasar)

### Transfer Hook - Counter

Count how many times tokens have been transferred.

[⚓ Anchor v2](./tokens/token-extensions/transfer-hook/counter/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-hook/counter/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-hook/counter/quasar)

### Transfer Hook - Account Data as Seed

Use token account owner data as seeds to derive extra accounts in a transfer hook.

[⚓ Anchor v2](./tokens/token-extensions/transfer-hook/account-data-as-seed/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-hook/account-data-as-seed/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-hook/account-data-as-seed/quasar)

### Transfer Hook - Allow/Block List

Restrict or allow token transfers using an onchain list managed by a list authority.

[⚓ Anchor v2](./tokens/token-extensions/transfer-hook/allow-block-list-token/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-hook/allow-block-list-token/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-hook/allow-block-list-token/quasar)

### Transfer Hook - Transfer Cost

Charge an additional fee on every token transfer.

[⚓ Anchor v2](./tokens/token-extensions/transfer-hook/transfer-cost/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-hook/transfer-cost/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-hook/transfer-cost/quasar)

### Transfer Hook - Transfer Switch

Enable or disable token transfers with an onchain switch.

[⚓ Anchor v2](./tokens/token-extensions/transfer-hook/transfer-switch/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-hook/transfer-switch/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-hook/transfer-switch/quasar)

### Transfer Hook - Whitelist

Restrict transfers so only whitelisted accounts can receive tokens.

[⚓ Anchor v2](./tokens/token-extensions/transfer-hook/whitelist/anchor) [⚓ Anchor v1](./tokens/token-extensions/transfer-hook/whitelist/anchor-v1) [💫 Quasar](./tokens/token-extensions/transfer-hook/whitelist/quasar)

## Compression

### cNFT Burn

Burn compressed NFTs.

[⚓ Anchor v2](./compression/cnft-burn/anchor) [⚓ Anchor v1](./compression/cnft-burn/anchor-v1) [💫 Quasar](./compression/cnft-burn/quasar)

### cNFT Vault

Store Metaplex compressed NFTs inside a PDA.

[⚓ Anchor v2](./compression/cnft-vault/anchor) [⚓ Anchor v1](./compression/cnft-vault/anchor-v1) [💫 Quasar](./compression/cnft-vault/quasar)

### Compression Utilities

Work with Metaplex compressed NFTs.

[⚓ Anchor v2](./compression/cutils/anchor) [⚓ Anchor v1](./compression/cutils/anchor-v1) [💫 Quasar](./compression/cutils/quasar)

## Tools

### Shank and Codama

Generate an IDL from a native Rust program with [Shank](https://github.com/metaplex-foundation/shank), then generate a Rust client from that IDL with [Codama](https://github.com/codama-idl/codama).

[🦀 Native](./tools/shank-and-codama/native)

## FAQ

### Is a Solana program the same as a smart contract?

Yes. "Program" is Solana's term for what other chains call a smart contract: code deployed onchain that holds state in accounts and executes instructions. Everything in this repository is a Solana program.

### How do I build an escrow on Solana?

Start with the [escrow example](./finance/escrow/anchor/), the best first finance program to learn: one state PDA, one vault, and three instruction handlers that swap two tokens atomically. There is also a [30-minute video walkthrough](https://www.youtube.com/watch?v=B5eBWWQfQuM) building it from scratch.

### How do I test a Solana program without running a validator?

Use [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm): Rust integration tests that run the program in-process, orders of magnitude faster than `solana-test-validator`. Every example in this repository tests this way, and `anchor init` scaffolds it by default since Anchor 1.0.

### What is the difference between Anchor, Quasar, Pinocchio, and native Rust?

[Anchor](https://www.anchor-lang.com/) is the most popular Solana framework, generating account validation and serialization from declarative constraints. [Quasar](https://quasar-lang.com/docs) offers Anchor-like ergonomics with zero-copy, `no_std` performance and smaller binaries. [Pinocchio](https://github.com/anza-xyz/pinocchio) is a zero-allocation library for hand-rolled programs, and native Rust uses Solana's crates directly with no framework. Most examples here ship several variants of the same program so you can compare.

### What is the best way to learn Solana finance programming?

Work through the [finance examples](#financial-software-defi) in order of complexity: [escrow](./finance/escrow/anchor/), then [token swap AMM](./finance/token-swap/anchor/), [order book](./finance/order-book/anchor/), [lending](./finance/lending/anchor/), and [perpetual futures](./finance/perpetual-futures/anchor/). Each README explains the financial mechanics in plain terms before the code.

### Are these examples production-ready?

They are teaching examples: every one builds and passes CI, and the finance programs additionally carry [Kani](https://github.com/model-checking/kani) formal-verification proofs of their money math. None are audited or deployed to mainnet, so treat them as reference implementations to learn from, not code to deploy as-is.

## Acknowledgements

Big thanks to Joe Caulfield and Solana Foundation for originally creating this repository.

---

**PRs welcome!** Follow the [contributing guidelines](./CONTRIBUTING.md) and see [CHANGELOG.md](./CHANGELOG.md) for release history.
