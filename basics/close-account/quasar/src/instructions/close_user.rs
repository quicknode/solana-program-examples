use {crate::state::UserState, quasar_lang::prelude::*};

/// Accounts for closing a user account.
/// The `close(dest = user)` attribute mirrors Anchor's `close = user`: at the
/// derive epilogue Quasar zeroes the discriminator, drains lamports to the
/// destination, reassigns the owner to the system program, and resizes to 0.
#[derive(Accounts)]
pub struct CloseUser {
    #[account(mut)]
    pub user: Signer,
    #[account(mut, close(dest = user))]
    pub user_account: Account<UserState>,
}

#[inline(always)]
pub fn handle_close_user(_accounts: &mut CloseUser) -> Result<(), ProgramError> {
    Ok(())
}
