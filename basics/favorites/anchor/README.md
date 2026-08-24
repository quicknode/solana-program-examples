# Favorites (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

Store per-user favorites in a [PDA](https://solana.com/docs/terminology#program-derived-address-pda). [Account](https://solana.com/docs/terminology#account) constraints ensure each user can only modify their own data.

See also: the [repository catalog](../../../README.md).

## Major concepts

- Per-user PDA keyed by signer
- Anchor constraints for authority checks

## Setup

```bash
anchor build
```

## Testing

```bash
anchor test
```

LiteSVM tests in `programs/` assert that users cannot overwrite each other's state.

## Usage

`anchor deploy` targets the cluster in `Anchor.toml`.