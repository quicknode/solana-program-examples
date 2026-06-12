# checking-account-asm-program

A Solana SBPF assembly implementation, scaffolded with [sbpf](https://github.com/blueshift-gg/sbpf).

## Setup

1. Build the program: `sbpf build`
2. Run the Rust + LiteSVM tests: `cargo test`

The tests embed the `.so` from `deploy/` at compile time, so rebuild after every change or a stale binary silently tests old code.
