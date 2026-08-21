# Close Account

> [!NOTE]
> This is the **Anchor v1** copy of this example, kept for programs staying on the
> Anchor v1 LTS line. Every `anchor` command on this page needs the v1 CLI:
> `avm install 1.1.2 && avm use 1.1.2`. The Anchor v2 version of this example is in
> [`../anchor`](../anchor/).

Two [instruction handlers](https://solana.com/docs/terminology#instruction-handler): `create_user` initializes a [PDA](https://solana.com/docs/terminology#program-derived-address-pda) `User` [account](https://solana.com/docs/terminology#account), and `close_user` closes it and returns the [rent](https://solana.com/docs/terminology#rent) to the user.

1. `create_user` initializes the PDA with [Anchor](https://solana.com/docs/terminology#anchor)'s `init` constraint:

   ```rust
   #[account(
       init,
       payer = user,
       space = User::DISCRIMINATOR.len() + User::INIT_SPACE,
       seeds = [b"USER", user.key().as_ref()],
       bump,
   )]
   pub user_account: Account<'info, User>,
   ```

   See [`programs/close-account/src/instructions/create_user.rs`](programs/close-account/src/instructions/create_user.rs).

2. `close_user` closes the account using Anchor's `close` constraint, which returns [lamports](https://solana.com/docs/terminology#lamport) to the given account:

   ```rust
   #[account(
       mut,
       seeds = [b"USER", user.key().as_ref()],
       bump = user_account.bump,
       close = user, // close account and return lamports to user
   )]
   pub user_account: Account<'info, User>,
   ```

   See [`programs/close-account/src/instructions/close_user.rs`](programs/close-account/src/instructions/close_user.rs).

## Setup

```bash
anchor build
```

## Testing

Tests live in [`programs/close-account/tests/test_close_account.rs`](programs/close-account/tests/test_close_account.rs) and run in-process with LiteSVM:

```bash
anchor test
```
