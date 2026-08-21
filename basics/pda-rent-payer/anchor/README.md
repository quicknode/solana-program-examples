# PDA Rent Payer (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

Use a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) to pay [rent](https://solana.com/docs/terminology#rent) when creating another account.

See also: [Pda Rent Payer overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- PDA as signer via seeds and bump
- Rent payer pattern

## Setup

From this directory (`basics/pda-rent-payer/anchor/`):

```bash
anchor build
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`), [Anchor](https://www.anchor-lang.com/docs).

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
anchor test
```

This runs `cargo test` as configured in `Anchor.toml`. Tests call instruction handlers and check onchain state.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
