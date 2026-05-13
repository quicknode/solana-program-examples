# cNFT Vault

Example code for working with Metaplex compressed NFTs (cNFTs) inside Solana Anchor programs.

The program keeps a PDA-owned vault. You send cNFTs to the vault, then withdraw them via the program's instruction handlers.

Two handlers:

- A simple transfer that withdraws one cNFT.
- A withdraw that handles two cNFTs in a single transaction.

Use this as a reference for working with cNFTs in your own programs.

## Components

- `programs/` — the Anchor program.
- `tests/` — TypeScript client-side tests.
- `tests/scripts/` — standalone scripts you can run individually. `withdrawWithLookup.ts` demonstrates using the program with Address Lookup Tables.

## Deployment

Deployed on devnet at `CNftyK7T8udPwYRzZUMWzbh79rKrz9a5GwV2wv7iEHpk`. To deploy your own, change the program ID in `lib.rs` and `Anchor.toml`.

## Limitations

This is a reference implementation. There's no authorization on withdraws — anyone can withdraw any cNFT in the vault. It's not optimized for compute either. Treat it as a proof of concept.

## Further resources

A video walkthrough is available on [Solandy's YouTube channel](https://youtu.be/qzr-q_E7H0M).
