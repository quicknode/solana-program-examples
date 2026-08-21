# Hello Solana (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

Minimal program that logs a greeting. Introduces transactions, instructions, and program entrypoints.

See also: [Hello Solana overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Program entrypoint and instruction data
- Logging from onchain code

## Setup

From this directory (`basics/hello-solana/anchor/`):

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
