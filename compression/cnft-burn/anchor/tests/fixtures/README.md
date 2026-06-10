# Test fixtures - mainnet program binaries

These `.so` files are the compiled onchain programs the cNFT-burn test CPIs
into, dumped from Solana **mainnet-beta** so [LiteSVM](https://github.com/LiteSVM/litesvm)
can load them locally (LiteSVM only bundles System/Token/Token Extensions/ATA). They
are the real programs - not modified - so accounts they create/verify behave
exactly as on mainnet.

| File | Program | Program ID | Source | Dumped (UTC) | Slot |
|------|---------|------------|--------|--------------|------|
| `mpl_bubblegum.so` | Metaplex Bubblegum (cNFTs) | `BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY` | mainnet-beta | 2026-06-05 | 424532091 |
| `spl_account_compression.so` | SPL Account Compression | `cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK` | mainnet-beta | 2026-06-05 | 424532091 |
| `spl_noop.so` | SPL Noop (log wrapper) | `noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV` | mainnet-beta | 2026-06-05 | 424532091 |

## Refreshing

These are point-in-time snapshots. To re-dump (e.g. after an upstream program
upgrade), update the date/slot above and run:

```bash
solana program dump BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY mpl_bubblegum.so           -u https://api.mainnet-beta.solana.com
solana program dump cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK  spl_account_compression.so -u https://api.mainnet-beta.solana.com
solana program dump noopb9bkMVfRPU8AsbpTUg8AQkHtKwMYZiFUjNRtMmV  spl_noop.so                -u https://api.mainnet-beta.solana.com
```
