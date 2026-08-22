# Counter (Anchor)

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

Increment a global counter stored in a [PDA](https://solana.com/docs/terminology#program-derived-address-pda). [Anchor](https://solana.com/docs/terminology#anchor) adds an explicit `initialize` handler that the native variant handles differently.

See also: the [repository catalog](../../../README.md).

## Major concepts

- PDA seeds for global state
- `init` vs `mut` account constraints
- Instruction handlers: initialize and increment

## Setup

From `basics/counter/anchor/`:

```bash
anchor build
```

## Testing

```bash
anchor test
```

LiteSVM integration tests in `programs/counter_anchor/tests/` call handlers and assert the stored count.

## Usage

Inspect `programs/counter_anchor/src/` for seeds and handler definitions.