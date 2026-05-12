# Quicknode Solana Program Examples

> A fork of the [Solana Foundation program examples](https://github.com/solana-developers/program-examples) with current versions, more programs, and additional frameworks.

[![Anchor](../../actions/workflows/anchor.yml/badge.svg)](../../actions/workflows/anchor.yml) [![Quasar](../../actions/workflows/quasar.yml/badge.svg)](../../actions/workflows/quasar.yml) [![Pinocchio](../../actions/workflows/pinocchio.yml/badge.svg)](../../actions/workflows/pinocchio.yml) [![Native](../../actions/workflows/native.yml/badge.svg)](../../actions/workflows/native.yml) [![ASM](../../actions/workflows/solana-asm.yml/badge.svg)](../../actions/workflows/solana-asm.yml)

Each example is available in one or more of the following frameworks:

- [⚓ Anchor](https://www.anchor-lang.com/) — the most popular framework for Solana development. Build with `anchor build`, test with `pnpm test` as defined in `Anchor.toml`.
- [💫 Quasar](https://quasar-lang.com/docs) — a newer, more performant framework with Anchor-compatible ergonomics. Run `pnpm test` to execute tests.
- [🤥 Pinocchio](https://github.com/febo/pinocchio) — a zero-copy, zero-allocation library for Solana programs. Run `pnpm test` to execute tests.
- [🦀 Native Rust](https://docs.solana.com/) — vanilla Rust using Solana's native crates. Run `pnpm test` to execute tests.
- [🧬 ASM](https://github.com/blueshift-gg/sbpf) — hand-written sBPF assembly built with the `sbpf` toolchain. Run `pnpm build-and-test` to build and test.

> [!NOTE]
> You don't need to write your own program for basic tasks like creating accounts, transferring SOL, or minting tokens. These are handled by existing programs like the System Program and Token Program.

## Financial Software

### Automated Market Maker

Constant product AMM (x·y=k) — create liquidity pools, deposit and withdraw liquidity, swap tokens with fees and slippage protection.

[⚓ Anchor](./tokens/token-swap/anchor) [💫 Quasar](./tokens/token-swap/quasar)

### Escrow

Peer-to-peer OTC trade — one user deposits token A and specifies how much token B they want. A counterparty fulfils the offer and both sides receive their tokens atomically.

[⚓ Anchor](./tokens/escrow/anchor) [💫 Quasar](./tokens/escrow/quasar) [🦀 Native](./tokens/escrow/native)

### Token Fundraiser

Create a fundraiser specifying a target mint and amount. Contributors deposit tokens until the goal is reached.

[⚓ Anchor](./tokens/token-fundraiser/anchor) [💫 Quasar](./tokens/token-fundraiser/quasar)

### Pyth Price Feeds

Read offchain price data onchain using the Pyth oracle network.

[⚓ Anchor](./oracles/pyth/anchor) [💫 Quasar](./oracles/pyth/quasar)

## Basics

### Hello Solana

A minimal program that logs a greeting.

[⚓ Anchor](./basics/hello-solana/anchor) [💫 Quasar](./basics/hello-solana/quasar) [🤥 Pinocchio](./basics/hello-solana/pinocchio) [🦀 Native](./basics/hello-solana/native) [🧬 ASM](./basics/hello-solana/asm)

### Account Data

Store and retrieve data using Solana accounts.

[⚓ Anchor](./basics/account-data/anchor) [💫 Quasar](./basics/account-data/quasar) [🤥 Pinocchio](./basics/account-data/pinocchio) [🦀 Native](./basics/account-data/native)

### Counter

Use a PDA to store global state — a counter that increments when called.

[⚓ Anchor](./basics/counter/anchor) [💫 Quasar](./basics/counter/quasar) [🤥 Pinocchio](./basics/counter/pinocchio) [🦀 Native](./basics/counter/native)

### Favorites

Save and update per-user state, ensuring users can only modify their own data.

[⚓ Anchor](./basics/favorites/anchor) [💫 Quasar](./basics/favorites/quasar) [🤥 Pinocchio](./basics/favorites/pinocchio) [🦀 Native](./basics/favorites/native)

### Checking Accounts

Validate that accounts provided in incoming instructions meet specific criteria.

[⚓ Anchor](./basics/checking-accounts/anchor) [💫 Quasar](./basics/checking-accounts/quasar) [🤥 Pinocchio](./basics/checking-accounts/pinocchio) [🦀 Native](./basics/checking-accounts/native) [🧬 ASM](./basics/checking-accounts/asm)

### Close Account

Close an account and reclaim its lamports.

[⚓ Anchor](./basics/close-account/anchor) [💫 Quasar](./basics/close-account/quasar) [🤥 Pinocchio](./basics/close-account/pinocchio) [🦀 Native](./basics/close-account/native)

### Create Account

Create new accounts on the blockchain.

[⚓ Anchor](./basics/create-account/anchor) [💫 Quasar](./basics/create-account/quasar) [🤥 Pinocchio](./basics/create-account/pinocchio) [🦀 Native](./basics/create-account/native) [🧬 ASM](./basics/create-account/asm)

### Cross-Program Invocation

Call one program from another — the hand program invokes the lever program to toggle a switch.

[⚓ Anchor](./basics/cross-program-invocation/anchor) [💫 Quasar](./basics/cross-program-invocation/quasar) [🦀 Native](./basics/cross-program-invocation/native)

### PDA Rent Payer

Use a PDA to pay rent for creating a new account.

[⚓ Anchor](./basics/pda-rent-payer/anchor) [💫 Quasar](./basics/pda-rent-payer/quasar) [🤥 Pinocchio](./basics/pda-rent-payer/pinocchio) [🦀 Native](./basics/pda-rent-payer/native)

### Processing Instructions

Add parameters to an instruction handler and use them.

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

### SPL Token Minter

Mint tokens from inside your own program using the Token program.

[⚓ Anchor](./tokens/spl-token-minter/anchor) [💫 Quasar](./tokens/spl-token-minter/quasar) [🦀 Native](./tokens/spl-token-minter/native)

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

Create token mints, mint tokens, and transfer tokens using Token Extensions.

[⚓ Anchor](./tokens/token-extensions/basics/anchor) [💫 Quasar](./tokens/token-extensions/basics/quasar)

### CPI Guard

Prevent certain token actions from occurring within cross-program invocations.

[⚓ Anchor](./tokens/token-extensions/cpi-guard/anchor) [💫 Quasar](./tokens/token-extensions/cpi-guard/quasar)

### Default Account State

Create new token accounts that are frozen by default.

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

Store metadata directly inside the token mint account, without needing additional programs.

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
