# Block List

A block-list program that implements the Token-2022 transfer-hook `execute` instruction.

A central authority maintains a block list — a collection of blocked wallets. Token issuers (transfer-hook extension authorities) can wire this program in as their hook and choose an operation mode: filter the source wallet only, or both source and destination.

## Operation modes

The mode depends on whether the block list is empty, plus the issuer's choice. Each mode corresponds to a different `extra-account-metas` account built for the mint (see `setup_extra_metas` below). When the list goes from empty to non-empty, the issuer must call `setup_extra_metas` again.

- **Empty extra metas** — default when the config counter is 0.
- **Check source** — default when the config counter is > 0.
- **Check both source and destination** — optional behavior when the counter is > 0.

## Accounts

### `Config`

- Defines the block-list authority.
- Tracks the number of blocked wallets.

### `WalletBlock`

- Marks a single wallet as blocked.

## Instruction handlers

### `init`

Initializes the global `Config` account with an authority.

### `block_wallet`

Adds a wallet to the block list, creating a `WalletBlock` record.

### `unblock_wallet`

Removes a wallet from the block list, closing its `WalletBlock` record.

### `setup_extra_metas`

Sets up the `extra-account-metas` account that the transfer-hook extension depends on. Takes an optional bool to switch operation modes when the counter is non-zero.

Once wallets are added to the block list, the issuer must call this again to pick one of the blocking modes.

### `tx_hook`

The hook invoked during token transfers.

## Repository layout

- **Program:** a Pinocchio-based block list under [`pinocchio/program/`](pinocchio/program/).
- **SDKs:** Codama-generated Rust and TypeScript SDKs under [`pinocchio/sdk/`](pinocchio/sdk/).
- **CLI:** a Rust CLI to interact with the program.

## Building

All commands below should be run from the [`pinocchio/`](pinocchio/) directory.

Install dependencies:

```bash
cd pinocchio
pnpm install
```

Build the program:

```bash
cd program
cargo build-sbf
```

Deploy it:

```bash
solana program deploy --program-id <your_program_keypair.json> target/deploy/block_list.so
```

Generate the SDKs:

```bash
pnpm run generate-sdks
```

Build the CLI:

```bash
cd cli
cargo build
```

## Setup

### Block list

Initialize the list and set the authority:

```bash
target/debug/block-list-cli init
```

Add a wallet:

```bash
target/debug/block-list-cli block-wallet <wallet_address>
```

Remove a wallet:

```bash
target/debug/block-list-cli unblock-wallet <wallet_address>
```

### Token mint

Create a new mint with the hook wired up:

```bash
spl-token create-token --program-2022 --transfer-hook BLoCKLSG2qMQ9YxEyrrKKAQzthvW4Lu8Eyv74axF6mf
```

Initialize the extra account metas:

```bash
target/debug/block-list-cli setup-extra-metas <wallet_address>
```

Switch to checking both source and destination wallets:

```bash
target/debug/block-list-cli setup-extra-metas --check-both-wallets <wallet_address>
```

## Devnet deployment

The program is deployed to devnet at `BLoCKLSG2qMQ9YxEyrrKKAQzthvW4Lu8Eyv74axF6mf`.

Example transactions:

- [Empty block list — transfer succeeds](https://explorer.solana.com/tx/2EnQD5mFZvrR3EAyFamCfxJDS3yAtZQxNVhFtK46PanCgbX6rpvgcQ961ZAs8H3auawJZPaVZMpAxoj3qZK55mHT?cluster=devnet)
- [Block list checking source only — transfer succeeds](https://explorer.solana.com/tx/4pmx31Lx5mXS7FWUtRjAxdRiwKZKCwJv3Du2qGhbLpQUenBuRxRUbrCaGGVjLjeDtpt4AXHzoNex1ppBsmKWSS7r?cluster=devnet)
- [Block list checking both — transfer succeeds](https://explorer.solana.com/tx/Q5Bk6GjGQ9TJtwS5zjDKp7GiFZK6efmGNCcxjqcmzf1YoZZJVE3rQkkSgSBNo7tst4hjUX6SJMsmEGXQ2NAdBjF?cluster=devnet)

## Disclaimer

This code has not been audited or reviewed. Use at your own discretion.
