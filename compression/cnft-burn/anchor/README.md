# cnft-burn

An Anchor program that burns compressed NFTs (cNFTs) in your collection. The program performs a CPI into the Metaplex Bubblegum program to do the burn.

## Components

- `programs/` — the Anchor program.
- `tests/` — tests for the program.

## Deployment

The program is deployed on devnet at `FbeHkUEevbhKmdk5FE5orcTaJkCYn5drwZoZXaxQXXNn`. To deploy your own copy, change the program ID in `lib.rs` and `Anchor.toml`.

## How to run

1. Configure the RPC endpoint in `cnft-burn.ts`.
2. `anchor build` from the example root.
3. `anchor deploy` to deploy to your chosen cluster.
4. `pnpm test` to run the tests.

## Acknowledgements

- [Metaplex](https://github.com/metaplex-foundation/) for the Bubblegum program and instruction builders.
- [@nickfrosty](https://twitter.com/nickfrosty) for the sample code that fetches and creates cNFTs.
