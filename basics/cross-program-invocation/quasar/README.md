# Cross-Program Invocation — Quasar

This example contains **two separate Quasar [programs](https://solana.com/docs/terminology#program)** that work together:

- **`lever/`** — A program with [onchain](https://solana.com/docs/terminology#onchain) `PowerStatus` state and a `switch_power` [instruction handler](https://solana.com/docs/terminology#instruction-handler) that toggles a boolean.
- **`hand/`** — A program that calls the lever program's `switch_power` via [CPI](https://solana.com/docs/terminology#cross-program-invocation-cpi).

## Building

Each program is a separate Quasar workspace. Build them independently:

```bash
cd lever && quasar build
cd hand && quasar build
```

Build the hand program **after** the lever, since its tests load the lever's compiled `.so` file.

## Testing

```bash
cd lever && cargo test
cd hand && cargo test
```

The hand tests load **both** programs into `QuasarSvm` and verify that the CPI correctly toggles the lever's power state.

## CPI pattern

Quasar doesn't have a `declare_program!` equivalent for importing arbitrary program [instruction](https://solana.com/docs/terminology#instruction) types (unlike [Anchor](https://solana.com/docs/terminology#anchor)). Instead, the hand program:

1. Defines a **marker type** (`LeverProgram`) that implements the `Id` trait with the lever's program address.
2. Uses `Program<LeverProgram>` in the [accounts](https://solana.com/docs/terminology#account) struct for compile-time address and executable validation.
3. Builds the CPI instruction data **manually** using `BufCpiCall`, constructing the lever's wire format directly.

This is lower-level than Anchor's CPI pattern but gives full control and works with any program.
