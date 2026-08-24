# Favorites (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

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