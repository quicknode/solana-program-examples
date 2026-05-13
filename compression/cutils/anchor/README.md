# cNFT Utils

Example code for working with Metaplex compressed NFTs (cNFTs) inside Solana Anchor programs.

This program shows how to add custom logic around the Bubblegum mint via CPI. Two handlers:

1. `mint` — mints a cNFT to your collection by CPI'ing Bubblegum. You can also initialize your own program-specific PDA in this handler.
2. `verify` — verifies that the owner of a given cNFT actually invoked the instruction. Useful as a building block for permissioned cNFT-gated logic.

Use this as a reference for working with cNFTs in your own programs.

## Components

- `programs/` — the Anchor program. The setup uses a `validate`/`actuate` pattern via Anchor's `access_control` macro; this pairs well with the cNFT verification logic.
- `tests/` — TypeScript tests.
  - `setup.ts` — run first if you don't already have a collection with a merkle tree.
  - `tests.ts` — individual minting and verification tests.

## Deployment

Deployed on devnet at `burZc1SfqbrAP35XG63YZZ82C9Zd22QUwhCXoEUZWNF`. To deploy your own, change the program ID in `lib.rs` and `Anchor.toml`.

## Limitations

Reference implementation only.

**This example pins Anchor 0.26.0** because of mpl-bubblegum dependency constraints at the time of writing.

## How to run

1. Configure the RPC endpoint in `utils/readAPI.ts`.
2. `cd` to the example root.
3. `pnpm install`.
4. (Optional) `npx tsx tests/setup.ts` to create an NFT collection and its merkle tree.
5. Comment out the tests you don't want to run in `tests/tests.ts`.
6. If minting, set your NFT URI.
7. If verifying, set the asset ID (cNFT mint address) you want to verify.
8. Run `anchor test --skip-build --skip-deploy --skip-local-validator`.
9. View your cNFTs on devnet via the Solflare wallet.
10. You may also want to change the wallet path in `Anchor.toml`.

## Acknowledgements

- [@nickfrosty](https://twitter.com/nickfrosty) for the sample code and [live demo](https://youtu.be/LxhTxS9DexU).
- [@HeyAndyS](https://twitter.com/HeyAndyS) for the groundwork in `cnft-vault`.
- Switchboard VRF-flip (since archived) for inspiring the validate/actuate setup.
