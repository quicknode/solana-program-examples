# Checking Accounts

Solana [programs](https://solana.com/docs/terminology#program) should check the [instructions](https://solana.com/docs/terminology#instruction) they receive to ensure security and to make sure required invariants hold.

The exact checks depend on what the program does. Common ones include:

- Verifying that the `program_id` on the instruction matches your own program.
- Verifying the order and number of [accounts](https://solana.com/docs/terminology#account).
- Checking the initialization state of an account.
