# Transfer Tokens (Anchor)

Transfer tokens between token accounts via CPI to the token program.

See also: [Transfer Tokens overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Associated token accounts
- `transfer_checked`, which carries the mint and decimals through the CPI
- `anchor_spl::token_interface` types, so the same program works against both the Classic Token Program and the Token Extensions Program
- Amounts: `mint_token` and `transfer_tokens` take `amount` in **minor units**, the raw integer the token program operates on. Clients convert from major units offchain: 1 token with 9 decimals is `1 * 10^9` minor units. The program never scales amounts onchain.

## Setup

From this directory (`tokens/transfer-tokens/anchor/`):

```bash
pnpm install
anchor build
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`), [Anchor](https://www.anchor-lang.com/docs), and `pnpm`.

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
anchor test
```

(`anchor test` runs the command configured in `Anchor.toml` `[scripts] test`.) Tests call instruction handlers and check onchain state.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
