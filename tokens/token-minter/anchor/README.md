# Token Minter (Anchor)

Mint tokens from inside your program using the [Classic Token Program](https://solana.com/docs/terminology#token-program).

See also: [Token Minter overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Mint authority on a PDA or signer
- Token account initialization
- Amounts: `mint_token` takes `amount` in **minor units**, the raw integer the token program operates on. Clients convert from major units offchain: 1 token with 9 decimals is `1 * 10^9` minor units. The program never scales amounts onchain.

## Setup

From this directory (`tokens/token-minter/anchor/`):

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
