# cNFT Vault

Example code for working with Metaplex compressed NFTs (cNFTs) inside Solana [Anchor](https://solana.com/docs/terminology#anchor) [programs](https://solana.com/docs/terminology#program).

The program keeps a PDA-owned vault. You send cNFTs to the vault, then withdraw them via the program's [instruction handlers](https://solana.com/docs/terminology#instruction-handler).

Two handlers:

- A simple transfer that withdraws one cNFT.
- A withdraw that handles two cNFTs in a single transaction.

Use this as a reference for working with cNFTs in your own programs.

## Components

- `programs/cnft-vault/` — the Anchor program.

There is no `tests/` directory in this example today. The program is intended to be deployed and exercised against a real cluster.

## Deployment

The program ID declared in [`programs/cnft-vault/src/lib.rs`](programs/cnft-vault/src/lib.rs) is `Fd4iwpPWaCU8BNwGQGtvvrcvG4Tfizq3RgLm8YLBJX6D`. Whether this address is currently deployed on any cluster is not tracked in this repo — verify with `solana program show <id>` against the cluster you care about.

To deploy your own copy, change the program ID in `lib.rs` and `Anchor.toml`, then run `anchor build && anchor deploy`.

## Limitations

This is a reference implementation. There's no authorization on withdraws — anyone can withdraw any cNFT in the vault. It's not optimized for compute either. Treat it as a proof of concept.

## Further resources

A video walkthrough is available on [Solandy's YouTube channel](https://youtu.be/qzr-q_E7H0M).
