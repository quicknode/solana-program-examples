# Counter (Anchor)

Increment a global counter stored in a [PDA](https://solana.com/docs/terminology#program-derived-address-pda). [Anchor](https://solana.com/docs/terminology#anchor) adds an explicit `initialize` handler that the native variant handles differently.

See also: the [repository catalog](../../README.md).

## Major concepts

- PDA seeds for global state
- `init` vs `mut` account constraints
- Instruction handlers: initialize and increment

## Setup

From `basics/counter/anchor/`:

```bash
pnpm install
anchor build
```

## Testing

```bash
pnpm test
```

LiteSVM integration tests in `programs/counter_anchor/tests/` call handlers and assert the stored count.

## Usage

Inspect `programs/counter_anchor/src/` for seeds and handler definitions.