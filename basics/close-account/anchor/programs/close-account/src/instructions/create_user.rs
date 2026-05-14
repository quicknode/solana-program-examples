use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CreateUserContext<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init,
        payer = user,
        space = User::DISCRIMINATOR.len() + User::INIT_SPACE,
        seeds = [
            b"USER",
            user.key().as_ref(),
        ],
        bump
    )]
    pub user_account: Account<'info, User>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_user(context: Context<CreateUserContext>, name: String) -> Result<()> {
    *context.accounts.user_account = User {
        bump: context.bumps.user_account,
        user: context.accounts.user.key(),
        name,
    };
    Ok(())
}
