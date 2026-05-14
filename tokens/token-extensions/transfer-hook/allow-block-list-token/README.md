# Allow/Block-List Token

A [Token Extensions](https://solana.com/docs/terminology#token-extensions-program) example that gates transfers through an allow/block list managed by a separate authority. The list is consumed by a transfer hook.

One list authority can manage lists for many [mints](https://solana.com/docs/terminology#token-mint) — useful when an issuer wants a third-party-managed list or wants to share a single list across a set of assets.

## Features

New tokens are created with several configuration options:

- Permanent delegate
- Allow list
- Block list
- Metadata
- Authorities

The issuer can choose one of three list modes:

- **Force Allow:** everyone receiving tokens must be explicitly allow-listed.
- **Block:** anyone can receive tokens unless they're block-listed.
- **Threshold Allow:** anyone can receive tokens unless block-listed *up to* a configurable threshold. Transfers above the threshold require explicit allow-listing.

These configurations are stored in the token mint's metadata.

The repo includes a UI (based on the `legacy-next-tailwind-basic` template) to manage allow/block lists. It also lets you create transfer-hook-enabled mints and perform transfers, since most wallets don't currently fetch transfer-hook dependencies on devnet or locally.

## Setup

```bash
pnpm install
anchor build       # replace your program ID first
pnpm run build     # build the UI
pnpm run dev       # serve the UI
```

### Local testing

Scripts manage the local validator and deployment:

- `./scripts/start.sh` — start the local validator and deploy the [program](https://solana.com/docs/terminology#program) (uses the [Anchor](https://solana.com/docs/terminology#anchor) CLI and the default Anchor keypair).
- `./scripts/stop.sh` — stop the local validator.
