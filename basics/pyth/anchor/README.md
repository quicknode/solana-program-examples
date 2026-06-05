# Pyth Price Feeds (Anchor)

Read a [Pyth](https://pyth.network/) price feed account and log price, confidence, and exponent.

See also: [Pyth overview](../README.md) and the [repository catalog](../../../README.md).

> [!NOTE]
> **The official `pyth-solana-receiver-sdk` is not Anchor 1.0 compatible (as of June 2026), so this example vendors the `PriceUpdateV2` account type instead of importing it.**
>
> The latest `pyth-solana-receiver-sdk` (1.2.0) builds against `anchor-lang` 0.32 and pulls `pythnet-sdk` (2.3.1), which still derives **borsh 0.10** on `PriceFeedMessage`. Anchor 0.32's `AnchorSerialize`/`AnchorDeserialize` derives require **borsh 1.x**, so `pyth-solana-receiver-sdk`'s own `PriceUpdateV2` fails to compile:
>
> ```
> error[E0277]: the trait bound `pythnet_sdk::messages::PriceFeedMessage: BorshSerialize` is not satisfied
> ```
>
> No published `pyth-solana-receiver-sdk` targets `anchor-lang` 1.0 (which this repo standardizes on), and no `pythnet-sdk` release has migrated to borsh 1.x — so the dependency can't simply be upgraded. Tracked upstream at [pyth-network/pyth-crosschain#3756](https://github.com/pyth-network/pyth-crosschain/issues/3756).
>
> As a workaround, `programs/pythexample/src/lib.rs` mirrors the on-chain `PriceUpdateV2` layout locally (same fields, same 8-byte discriminator, owned by the Pyth Receiver program) so accounts written by Pyth deserialize unchanged. Replace the vendored type with the SDK import once an Anchor 1.0 / borsh 1.x compatible release ships.

## Major concepts

- Oracle price accounts
- Consuming external onchain data in a program

## Setup

From this directory (`basics/pyth/anchor/`):

```bash
pnpm install
anchor build
```

Prerequisites: [Agave](https://docs.anza.xyz/) CLI (version in `Anchor.toml` `[toolchain]`), [Anchor](https://www.anchor-lang.com/docs), and `pnpm`.

## Testing

Tests run in-process with [LiteSVM](https://www.anchor-lang.com/docs/testing/litesvm). No local validator.

```bash
pnpm test
```

This runs `cargo test` as configured in `Anchor.toml`. Tests call instruction handlers and check onchain state.

## Usage

Read the program `programs/` source and `Anchor.toml` for deployed program IDs. For deployment, use `anchor build && anchor deploy` against your target cluster.
