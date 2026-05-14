# cnft-burn

An [Anchor](https://solana.com/docs/terminology#anchor) [program](https://solana.com/docs/terminology#program) that burns compressed NFTs (cNFTs) in your collection. The program performs a [CPI](https://solana.com/docs/terminology#cross-program-invocation-cpi) into the Metaplex Bubblegum program to do the burn.

## Components

- `programs/cnft-burn/` — the Anchor program.
- `migrations/` — deployment script.

There is no `tests/` directory in this example today. The program is intended to be deployed and exercised against a real cluster.

## Deployment

The program ID declared in [`programs/cnft-burn/src/lib.rs`](programs/cnft-burn/src/lib.rs) is `C6qxH8n6mZxrrbtMtYWYSp8JR8vkQ55X1o4EBg7twnMv`. Whether this address is currently deployed on any cluster is not tracked in this repo — verify with `solana program show <id>` against the cluster you care about.

To deploy your own copy, change the program ID in `lib.rs` and `Anchor.toml`, then run `anchor build && anchor deploy`.

## Acknowledgements

- [Metaplex](https://github.com/metaplex-foundation/) for the Bubblegum program and [instruction](https://solana.com/docs/terminology#instruction) builders.
- [@nickfrosty](https://twitter.com/nickfrosty) for the sample code that fetches and creates cNFTs.
