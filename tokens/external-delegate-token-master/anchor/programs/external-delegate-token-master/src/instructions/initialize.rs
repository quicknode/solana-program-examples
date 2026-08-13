use anchor_lang::prelude::*;

use crate::UserAccount;

#[derive(Accounts)]
pub struct InitializeAccountConstraints {
    #[account(
        init,
        payer = authority,
        space = UserAccount::DISCRIMINATOR.len() + UserAccount::INIT_SPACE,
    )]
    pub user_account: BorshAccount<UserAccount>,

    #[account(mut)]
    pub authority: Signer,

    pub system_program: Program<System>,
}

pub fn handler(context: &mut Context<InitializeAccountConstraints>) -> Result<()> {
    let user_account = &mut context.accounts.user_account;
    user_account.authority = *context.accounts.authority.address();
    user_account.ethereum_address = [0; 20];
    user_account.nonce = 0;
    Ok(())
}
