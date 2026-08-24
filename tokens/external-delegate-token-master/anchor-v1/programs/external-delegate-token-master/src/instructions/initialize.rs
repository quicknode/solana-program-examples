use anchor_lang::prelude::*;

use crate::UserAccount;

#[derive(Accounts)]
pub struct InitializeAccountConstraints<'info> {
    #[account(
        init,
        payer = authority,
        space = UserAccount::DISCRIMINATOR.len() + UserAccount::INIT_SPACE,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<InitializeAccountConstraints>) -> Result<()> {
    let user_account = &mut context.accounts.user_account;
    user_account.authority = context.accounts.authority.key();
    user_account.ethereum_address = [0; 20];
    user_account.nonce = 0;
    Ok(())
}
