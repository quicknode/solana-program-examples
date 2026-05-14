# Token Extensions Metadata-Pointer NFT

An Anchor program that mints an NFT using the Token Extensions metadata-pointer extension. The mint itself stores its own metadata via the metadata extension, so no separate Metaplex metadata account is needed.

This is particularly useful for games — you get arbitrary key/value metadata stored onchain that you can use to record character state. In this example, the player's level and collected wood are stored on the NFT.

When marketplaces support additional metadata, NFTs can be filtered or ranked by those fields, e.g. by character level.

A [video walkthrough](https://www.youtube.com/@SolanaFndn/videos) is available on the Solana Foundation YouTube channel.

## How to run

### Tests

```bash
cd anchor
anchor build
pnpm test
```

### JS client

```bash
cd app
pnpm install
pnpm dev
```

## Minting flow

Creating an NFT this way:

1. Create the mint account.
2. Initialize the metadata pointer (must happen *before* initializing the mint).
3. Initialize the mint with 0 decimals.
4. Initialize the metadata extension on the mint itself.
5. Add any custom fields (e.g. `level`).
6. Create the player's Associated Token Account.
7. Mint one token to the ATA.
8. Remove the mint authority — irreversible, makes it an NFT.

See `programs/extension_nft/src/instructions/mint_nft.rs` for the Rust implementation.

## Energy system (example onchain game)

The program includes a simple energy system: a player initializes a `PlayerData` account, then calls `chop_tree` to consume one energy and gain one wood. Energy refills over time, computed lazily from the last-login timestamp.

```rust
const TIME_TO_REFILL_ENERGY: i64 = 60; // seconds per energy point
const MAX_ENERGY: u64 = 100;
```

The JS client subscribes to the player account via WebSocket and runs the same energy calculation locally to show a countdown timer.

## Project structure

```text
anchor/programs/extension_nft/src/
├── instructions/
│   ├── chop_tree.rs
│   ├── init_player.rs
│   ├── mint_nft.rs
│   └── mod.rs
├── state/
│   ├── game_data.rs
│   ├── mod.rs
│   └── player_data.rs
├── constants.rs
├── errors.rs
└── lib.rs
```

`PlayerData::update_energy` (in `state/player_data.rs`) is where the lazy refill is computed; there is no separate `update_energy.rs` instruction handler.

## Session keys

The example uses [Gum session keys](https://github.com/magicblock-labs/session-keys) to auto-approve transactions: a local keypair is topped up with a small amount of SOL and is allowed to sign specific program instructions for a limited window (currently 23h). When it expires, the SOL is returned and a new session can be created.

Neither the program nor the session-keys library has been audited. Use at your own risk.

## Building your own

The example was scaffolded with `npx create-solana-game gamName`.

If you want to start fresh:

1. Install [Anchor](https://www.anchor-lang.com/docs/installation).
2. `cd anchor`, then `anchor build` and `anchor deploy`.
3. Copy the printed program ID into `lib.rs`, `Anchor.toml`, and `app/utils/anchor.ts`.
4. Rebuild and redeploy.
5. `cd app && pnpm install && pnpm dev` to run the client.

After changing the program, copy the regenerated IDL types from `target/idl/` into the client so they stay in sync.
