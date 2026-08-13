use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CloseUserAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        mut,
        seeds = [
            b"USER",
            user.address().as_ref(),
        ],
        bump = user_account.bump,
        close = user, // close account and return lamports to user
    )]
    pub user_account: BorshAccount<User>,
}

pub fn handle_close_user(_context: &mut Context<CloseUserAccountConstraints>) -> Result<()> {
    Ok(())
}
