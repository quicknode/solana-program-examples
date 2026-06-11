use {crate::state::User, quasar_lang::prelude::*};

/// Accounts for closing a user account.
/// The `address = ...` check binds `user_account` to the signer's own PDA:
/// without it, anyone could pass someone else's user account and pocket its
/// rent. The `close(dest = user)` attribute mirrors Anchor's `close = user`:
/// at the derive epilogue Quasar zeroes the discriminator, drains lamports to
/// the destination, reassigns the owner to the system program, and resizes
/// to 0.
#[derive(Accounts)]
pub struct CloseUserAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(mut, close(dest = user), address = User::seeds(user.address()))]
    pub user_account: Account<User>,
}

#[inline(always)]
pub fn handle_close_user(_accounts: &mut CloseUserAccountConstraints) -> Result<(), ProgramError> {
    Ok(())
}
