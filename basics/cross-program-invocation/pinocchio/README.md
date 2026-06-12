# Cross Program Invocation: Solana Pinocchio

A [Cross Program Invocation (CPI)](https://solana.com/docs/core/cpi) example written using the [Pinocchio](https://github.com/anza-xyz/pinocchio) framework with only the Solana toolchain.

Two programs work together:

- `lever` - owns a `power` account that stores a single on/off byte. It exposes `initialize` (create the account) and `switch_power` (flip the byte and log who pulled it).
- `hand` - takes a name, then invokes `lever`'s `switch_power` instruction via CPI, forwarding the name.

Because Pinocchio runs in `no_std` without an allocator, `hand` builds the CPI instruction buffer on the stack and caps the forwarded name length.

## Setup

1. Build both [programs](https://solana.com/docs/terminology#program): `pnpm build`
2. Build and test: `pnpm build-and-test`

The tests live in `programs/lever/tests/test.rs` and run on-chain in [LiteSVM](https://github.com/LiteSVM/litesvm) - there are no JavaScript tests. They exercise the full `initialize -> pull -> pull-again` CPI flow plus an invalid-discriminator rejection case. `pnpm build` compiles both programs to `tests/fixtures` so the LiteSVM test can load them.

## Credits

Ported from the [Pinocchio cross-program-invocation example](https://github.com/solana-developers/program-examples/pull/584) contributed by [@MarkFeder](https://github.com/MarkFeder) to solana-developers/program-examples.
