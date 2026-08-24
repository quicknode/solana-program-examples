# PDA Mint Authority (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

Mint tokens with a PDA as mint authority so the program controls issuance.

See also: [Pda Mint Authority overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- PDA mint authority
- CPI mint_to
- Amounts: `mint_token` takes `amount` in **minor units**, the raw integer the token program operates on. Clients convert from major units offchain: 1 token with 9 decimals is `1 * 10^9` minor units. The program never scales amounts onchain.

## Setup

From this directory (`tokens/pda-mint-authority/anchor/`):

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
