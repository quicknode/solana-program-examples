# Destroy an Account

1. A `PDA` is created using the [create_user.rs](programs/destroy-an-account/src/instructions/create_user.rs) instruction.

   ```rust
   #[account(
       init,
       seeds = [User::PREFIX.as_bytes(), user.key().as_ref()],
       payer = user,
       space = User::SIZE,
       bump,
   )]
   pub user_account: Box<Account<'info, User>>,
   ```

2. The account is closed in [destroy_user.rs](programs/destroy-an-account/src/instructions/destroy_user.rs), using Anchor's `close` helper on the account info:

   ```rust
   user_account.close(user.to_account_info())?;
   ```

3. The test [destroy-an-account.ts](tests/destroy-an-account.ts) verifies that the account is null both before creation and after closing, via `fetchNullable`:

   ```typescript
   const userAccountBefore = await program.account.user.fetchNullable(userAccountAddress, "processed");
   assert.equal(userAccountBefore, null);
   // ...
   const userAccountAfter = await program.account.user.fetchNullable(userAccountAddress, "processed");
   assert.notEqual(userAccountAfter, null);
   ```
