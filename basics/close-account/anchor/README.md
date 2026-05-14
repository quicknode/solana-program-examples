# Close Account

Two instruction handlers: `create_user` initializes a PDA `UserState` account, and `close_user` closes it and returns the rent to the user.

1. `create_user` initializes the PDA with Anchor's `init` constraint:

   ```rust
   #[account(
       init,
       payer = user,
       space = UserState::DISCRIMINATOR.len() + UserState::INIT_SPACE,
       seeds = [b"USER", user.key().as_ref()],
       bump,
   )]
   pub user_account: Account<'info, UserState>,
   ```

   See [`programs/close-account/src/instructions/create_user.rs`](programs/close-account/src/instructions/create_user.rs).

2. `close_user` closes the account using Anchor's `close` constraint, which returns lamports to the given account:

   ```rust
   #[account(
       mut,
       seeds = [b"USER", user.key().as_ref()],
       bump = user_account.bump,
       close = user, // close account and return lamports to user
   )]
   pub user_account: Account<'info, UserState>,
   ```

   See [`programs/close-account/src/instructions/close_user.rs`](programs/close-account/src/instructions/close_user.rs).

## Tests

Tests live in [`programs/close-account/tests/test_close_account.rs`](programs/close-account/tests/test_close_account.rs) and run against litesvm. `Anchor.toml`'s `scripts.test` is `cargo test`, so `anchor test` builds the program and runs the Rust tests:

```bash
anchor test
```
