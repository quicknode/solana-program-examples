# Checking Accounts

Solana programs should check the instructions they receive to ensure security and to make sure required invariants hold.

The exact checks depend on what the program does. Common ones include:

- Verifying that the `program_id` on the instruction matches your own program.
- Verifying the order and number of accounts.
- Checking the initialization state of an account.
