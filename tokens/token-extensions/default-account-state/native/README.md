# Token Extensions - Default Account State

This extension sets a default state for all [token accounts](https://solana.com/docs/terminology#token-account) of a given [mint](https://solana.com/docs/terminology#token-mint).

[Account](https://solana.com/docs/terminology#account) states:

- **initialized:** a normal token account that can transfer, etc.
- **frozen:** the owner cannot perform any token actions until the account is unfrozen.

## Setup

1. Build the program: `cargo build-sbf --manifest-path=./program/Cargo.toml`
2. Run the Rust + LiteSVM tests: `cargo test --manifest-path=./program/Cargo.toml`

Rebuild the program after every change before re-running the tests: the tests embed the `.so` at compile time, so a stale binary silently tests old code.
