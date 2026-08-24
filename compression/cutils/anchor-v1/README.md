# cNFT Utils

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

Example code for working with Metaplex compressed NFTs (cNFTs) inside Solana [Anchor](https://solana.com/docs/terminology#anchor) [programs](https://solana.com/docs/terminology#program).

This program shows how to add custom logic around the Bubblegum [mint](https://solana.com/docs/terminology#token-mint) via [CPI](https://solana.com/docs/terminology#cross-program-invocation-cpi). Two handlers:

1. `mint` - mints a cNFT to your collection by CPI'ing Bubblegum. You can also initialize your own program-specific [PDA](https://solana.com/docs/terminology#program-derived-address-pda) in this handler.
2. `verify` - verifies that the owner of a given cNFT actually invoked the [instruction](https://solana.com/docs/terminology#instruction). Useful as a building block for permissioned cNFT-gated logic.

Use this as a reference for working with cNFTs in your own programs.

## Components

- `programs/cutils/` - the Anchor program. Instruction handlers live in `src/instructions/` (`handle_mint`, `handle_verify`).

## Testing

A Rust [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm) integration suite lives in `programs/cutils/tests/`. It loads mainnet-dumped fixture binaries for Bubblegum, SPL Account Compression, and SPL Noop from `tests/fixtures/` (see the README there), so the CPIs run against the real programs in-process.

```bash
cargo build-sbf
cargo test
```

## Deployment

The program ID declared in [`programs/cutils/src/lib.rs`](programs/cutils/src/lib.rs) is `BuFyrgRYzg2nPhqYrxZ7d9uYUs4VXtxH71U8EcoAfTQZ`. Whether this address is currently deployed on any cluster is not tracked in this repo - verify with `solana program show <id>` against the cluster you care about.

To deploy your own copy, change the program ID in `lib.rs` and `Anchor.toml`, then run `anchor build && anchor deploy`.

## Limitations

Reference implementation only.

## Acknowledgements

- [@nickfrosty](https://twitter.com/nickfrosty) for the sample code and [live demo](https://youtu.be/LxhTxS9DexU).
- [@HeyAndyS](https://twitter.com/HeyAndyS) for the groundwork in `cnft-vault`.
