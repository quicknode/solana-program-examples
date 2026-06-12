# Counter: Solana Pinocchio

Counter written using the Pinocchio framework, with only the Solana toolchain.

## Setup

1. Build the [program](https://solana.com/docs/terminology#program): `cargo build-sbf --manifest-path=./program/Cargo.toml`
2. Run the Rust + LiteSVM tests: `cargo test --manifest-path=./program/Cargo.toml`

Rebuild the program after every change before re-running the tests: the tests embed the `.so` at compile time, so a stale binary silently tests old code.

## Debugging

1. Start a test validator: `solana-test-validator`
2. Listen to program logs: `solana config set -ul && solana logs`
