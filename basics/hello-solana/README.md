# Hello Solana

Our first Solana [program](https://solana.com/docs/terminology#program) — a "hello, world" that logs a greeting. Along the way, a quick look at what's inside a Solana transaction.

## Transactions

> For a closer look, see the [Solana docs on transactions](https://solana.com/docs/core/transactions).

Two things to keep separate:

- :key: **Transactions** are for **the Solana runtime**. They contain everything the runtime needs to allow or deny a transaction (signers, recent blockhash, etc.) and to decide what can run in parallel.
- :key: **[Instructions](https://solana.com/docs/terminology#instruction)** are for **Solana programs**. They tell a program what to do.
- :key: Your program receives one instruction at a time (`program_id`, `accounts`, `instruction_data`).

### Transaction

```text
signatures: [ s, s ]
message:
    header: 000
    addresses: [ aaa, aaa ]
    recent_blockhash: int
    instructions: [ ix, ix ]
```

### Instruction

```text
program_id: xxx
accounts: [ aaa, aaa ]
instruction_data: b[]
```
