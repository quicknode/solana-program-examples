# Test fixtures — mainnet program binaries

These `.so` files are the compiled on-chain programs the cutils test CPIs
into, dumped from Solana **mainnet-beta** so [LiteSVM](https://github.com/LiteSVM/litesvm)
can load them locally (LiteSVM only bundles System/Token/Token-2022/ATA). They
are the real programs — not modified — so accounts they create/verify behave
exactly as on mainnet.

`mpl_token_metadata.so` is required because the cutils `mint` instruction CPIs
Bubblegum `MintToCollectionV1`, which in turn validates a real Token-Metadata
collection NFT (mint + metadata + master edition) that the test builds.

| File | Program | Program ID | Source | Dumped (UTC) | Slot |
|------|---------|------------|--------|--------------|------|
| `mpl_bubblegum.so` | Metaplex Bubblegum (cNFTs) | `BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY` | mainnet-beta | 2026-06-05 | 424532091 |
| `spl_account_compression.so` | SPL Account Compression | `cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK` | mainnet-beta | 2026-06-05 | 424532091 |
| `spl_noop.so` | SPL Noop (log wrapper) | `noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV` | mainnet-beta | 2026-06-05 | 424532091 |
| `mpl_token_metadata.so` | Metaplex Token Metadata | `metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s` | mainnet-beta | 2026-06-05 | 424532091 |

## Refreshing

These are point-in-time snapshots. To re-dump (e.g. after an upstream program
upgrade), update the date/slot above and run:

```bash
solana program dump BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY mpl_bubblegum.so           -u https://api.mainnet-beta.solana.com
solana program dump cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK  spl_account_compression.so -u https://api.mainnet-beta.solana.com
solana program dump noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV  spl_noop.so                -u https://api.mainnet-beta.solana.com
solana program dump metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s mpl_token_metadata.so      -u https://api.mainnet-beta.solana.com
```
