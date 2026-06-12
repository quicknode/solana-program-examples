# Recommended Program Layout: Solana Pinocchio

The recommended multi-file layout for a Solana [program](https://solana.com/docs/terminology#program), written using the [Pinocchio](https://github.com/anza-xyz/pinocchio) framework with only the Solana toolchain.

The `src` folder splits responsibilities the same way the `native` and `anchor` examples do:

- `lib.rs` - module declarations and the program entrypoint
- `processor.rs` - decodes instruction data and dispatches to the right instruction
- `instructions/` - one file per instruction (`get_on_ride`, `play_game`, `eat_food`)
- `state/` - the program's data objects (`ride`, `game`, `food`)
- `error.rs` - custom errors

## Setup

1. Build the [program](https://solana.com/docs/terminology#program): `cargo build-sbf --manifest-path=./program/Cargo.toml`
2. Run the Rust + LiteSVM tests: `cargo test --manifest-path=./program/Cargo.toml`

Rebuild the program after every change before re-running the tests: the tests embed the `.so` at compile time, so a stale binary silently tests old code.

## Credits

Ported from the [Pinocchio repository-layout example](https://github.com/solana-developers/program-examples/pull/582) contributed by [@MarkFeder](https://github.com/MarkFeder) to solana-developers/program-examples.
