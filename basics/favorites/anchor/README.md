# Favorites (Anchor)

Store per-user favorites in a [PDA](https://solana.com/docs/terminology#program-derived-address-pda). [Account](https://solana.com/docs/terminology#account) constraints ensure each user can only modify their own data.

See also: the [repository catalog](../../README.md).

## Major concepts

- Per-user PDA keyed by signer
- Anchor constraints for authority checks

## Setup

```bash
pnpm install
anchor build
```

## Testing

```bash
pnpm test
```

LiteSVM tests in `programs/` assert that users cannot overwrite each other's state.

## Usage

`anchor deploy` targets the cluster in `Anchor.toml`. Used in [Solana Professional Education](https://github.com/solana-developers/professional-education).