# cNFT Utils

Example code for working with Metaplex compressed NFTs (cNFTs) inside Solana [Anchor](https://solana.com/docs/terminology#anchor) [programs](https://solana.com/docs/terminology#program).

This program shows how to add custom logic around the Bubblegum [mint](https://solana.com/docs/terminology#token-mint) via [CPI](https://solana.com/docs/terminology#cross-program-invocation-cpi). Two handlers:

1. `mint` — mints a cNFT to your collection by CPI'ing Bubblegum. You can also initialize your own program-specific [PDA](https://solana.com/docs/terminology#program-derived-address-pda) in this handler.
2. `verify` — verifies that the owner of a given cNFT actually invoked the [instruction](https://solana.com/docs/terminology#instruction). Useful as a building block for permissioned cNFT-gated logic.

Use this as a reference for working with cNFTs in your own programs.

## Components

- `programs/cutils/` — the Anchor program. The setup uses a `validate`/`actuate` pattern via Anchor's `access_control` macro; this pairs well with the cNFT verification logic.

There is no `tests/` directory in this example today. The program is intended to be deployed and exercised against a real cluster.

## Deployment

The program ID declared in [`programs/cutils/src/lib.rs`](programs/cutils/src/lib.rs) is `BuFyrgRYzg2nPhqYrxZ7d9uYUs4VXtxH71U8EcoAfTQZ`. Whether this address is currently deployed on any cluster is not tracked in this repo — verify with `solana program show <id>` against the cluster you care about.

To deploy your own copy, change the program ID in `lib.rs` and `Anchor.toml`, then run `anchor build && anchor deploy`.

## Limitations

Reference implementation only.

## Acknowledgements

- [@nickfrosty](https://twitter.com/nickfrosty) for the sample code and [live demo](https://youtu.be/LxhTxS9DexU).
- [@HeyAndyS](https://twitter.com/HeyAndyS) for the groundwork in `cnft-vault`.
- Switchboard VRF-flip (since archived) for inspiring the validate/actuate setup.
