# NFT Operations (Quasar)

Collection mint, NFT mint, and collection verification via Metaplex. The Quasar twin of the [Anchor](../anchor/) variant, sharing its program ID and instruction surface.

See also: the [repository catalog](../../../README.md).

## Major concepts

- A PDA at seeds `["authority"]` is the mint authority and update authority for the collection and every NFT.
- `create_collection` mints a **collection NFT**: it mints one token, creates the Metaplex metadata account (marked as a sized collection via `CollectionDetails`), and creates the master edition. Metadata `name`, `symbol`, and `uri` are instruction arguments, bounded to the Metaplex limits by their types (`String<32>`, `String<10>`, `String<200>`), so oversized values are rejected at instruction decoding.
- `mint_nft` mints an individual NFT the same way, with an unverified reference to the collection in its metadata.
- `verify_collection` verifies the NFT's collection membership through a `VerifySizedCollectionItem` CPI signed by the PDA authority.
- The metadata-creation and verification CPIs are built in the program (`src/instructions/mod.rs` and `verify_collection.rs`) rather than with `quasar_metadata`'s helpers, because the helpers cannot encode creators, collection references, or sized-collection details, and mark the collection metadata readonly during verification.

## Setup

From `tokens/nft-operations/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

In-process tests via **Quasar SVM** (`quasar-svm` in `Quasar.toml`):

```bash
cargo test
```

The suite loads the Metaplex Token Metadata program from the fixture shared with the Anchor twin (`../anchor/tests/fixtures/mpl_token_metadata.so`) and exercises the full lifecycle: create the collection, mint an NFT into it, and verify membership, asserting token balances and metadata contents. No local validator.

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant of the same example.
