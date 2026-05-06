use {
    crate::state::{UserState, UserStateInner},
    quasar_lang::{prelude::*, sysvars::Sysvar},
};

/// Accounts for creating a new user.
#[derive(Accounts)]
pub struct CreateUser {
    #[account(mut)]
    pub user: Signer,
    #[account(mut, init, payer = user, address = UserState::seeds(user.address()))]
    pub user_account: Account<UserState>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_create_user(
    accounts: &mut CreateUser,
    name: &str,
    bump: u8,
) -> Result<(), ProgramError> {
    let user_address = *accounts.user.to_account_view().address();
    let rent = Rent::get()?;
    accounts.user_account.set_inner(
        UserStateInner { bump, user: user_address, name },
        accounts.user.to_account_view(),
        rent.lamports_per_byte(),
        rent.exemption_threshold_raw(),
    )
}
