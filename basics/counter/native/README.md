# Counter: Solana Native

Counter written in Solana native, using only the Solana toolchain.

## Setup

1. Build the program: `cargo build-sbf`
2. Run the tests: `pnpm test`

## Debugging

1. Start a test validator: `pnpm start-validator`
2. Listen to program logs: `solana config set -ul && solana logs`
3. Run the tests: `pnpm run-tests`
