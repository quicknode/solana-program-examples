# Counter (Anchor)

> [!NOTE]
> This is the **Anchor v2** copy of this example. Every `anchor` command on this page
> needs the v2 CLI: `cargo install anchor-cli --version 2.0.0-rc.1 --locked` (avm has
> no prebuilt binary for this pre-release). The Anchor v1 version of this example is in
> [`../anchor-v1`](../anchor-v1/).

Increment a global counter stored in a [PDA](https://solana.com/docs/terminology#program-derived-address-pda). [Anchor](https://solana.com/docs/terminology#anchor) adds an explicit `initialize_counter` handler that the native variant handles differently.

See also: the [repository catalog](../../README.md).

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