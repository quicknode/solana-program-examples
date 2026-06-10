# Pyth Price Feeds (Quasar)

Read a Pyth price feed and use oracle data in program logic.

See also: [Pyth overview](../README.md) and the [repository catalog](../../../README.md).

## Major concepts

- Oracle accounts
- Price feed layout
- Oracle account validation: `read_price` only accepts accounts owned by the Pyth Receiver program (`rec5EKMGg6MxZYaMdyBfgwp4d5rB9T1VQH5pJv5LtFJ`)
- Price freshness: updates older than `MAXIMUM_PRICE_AGE_SECONDS` are rejected (compared against `publish_time`, a unix timestamp in seconds)

## Setup

From `basics/pyth/quasar/`:

```bash
quasar build
```

Prerequisites: [Quasar](https://quasar-lang.com/docs) CLI and [Agave](https://docs.anza.xyz/) toolchain (see `Quasar.toml`).

## Testing

In-process tests via **Quasar SVM** (`quasar-svm` in `Quasar.toml`):

```bash
cargo test
```

Tests invoke instruction handlers and assert onchain state. No local validator.

## Usage

Read `src/` and `Quasar.toml`. Compare with the [Anchor](../anchor/) variant in the same example where present.
