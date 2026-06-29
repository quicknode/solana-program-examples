# cNFT Burn (Quasar)

Burn compressed NFTs via Metaplex Bubblegum CPIs.

See also: the [repository catalog](../../../README.md).

## Major concepts

- Compressed NFTs
- Bubblegum CPI

## Setup

From `compression/cnft-burn/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

This variant has no automated test suite yet: the instruction handlers CPI into external programs (Bubblegum, SPL Account Compression) and a QuasarSVM harness that loads those fixture binaries has not been written. `quasar build` verifies the program and CPI construction compile.

The Anchor twin at `../anchor/` has a full LiteSVM integration suite that exercises the same flows against mainnet-dumped fixture programs; use it as the behavioural reference.

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant in the same example where present.
