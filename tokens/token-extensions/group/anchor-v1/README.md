# Token Extensions - Group Pointer (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

Link tokens to a group using the Group Pointer extension.

See also: the [repository catalog](../../../../README.md).

## Major concepts

- Group pointer extension
- Grouped mint metadata

## Setup

From this directory (`tokens/token-extensions/group/anchor/`):

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
