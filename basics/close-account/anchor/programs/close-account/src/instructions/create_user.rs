use crate::state::*;
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct CreateUserAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        init,
        payer = user,
        space = User::DISCRIMINATOR.len() + User::INIT_SPACE,
        seeds = [
            b"USER",
            user.address().as_ref(),
        ],
        bump
    )]
    pub user_account: BorshAccount<User>,
    pub system_program: Program<System>,
}

pub fn handle_create_user(
    context: &mut Context<CreateUserAccountConstraints>,
    name: String,
) -> Result<()> {
    *context.accounts.user_account = User {
        bump: context.bumps.user_account,
        user: *context.accounts.user.address(),
        name,
    };
    Ok(())
}
