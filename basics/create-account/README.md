# Create Account

Create a Solana account.

The account is a **system account** — owned by the System Program, which means only the System Program can modify its data. In this example, the account simply holds some SOL.

The tests cover two ways to create the account:

1. **Via cross-program invocation (CPI):** the client sends a transaction to our deployed program, which in turn calls the System Program.
2. **Directly:** the client sends the create-account transaction straight to the System Program.

See [cross-program-invocation](../cross-program-invocation) for more CPI examples.

## Links

- [Solana Cookbook — How to Create a System Account](https://solana.com/developers/cookbook/accounts/create-account)
- [Rust Docs — `solana_system_interface::instruction::create_account`](https://docs.rs/solana-system-interface/latest/solana_system_interface/instruction/fn.create_account.html)
