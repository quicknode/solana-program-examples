use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{close_account, CloseAccount},
    token_interface::{Mint, Token2022},
};

#[derive(Accounts)]
pub struct CloseAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        extensions::close_authority::authority = authority,
    )]
    pub mint_account: InterfaceAccount<'info, Mint>,
    pub token_program: Program<'info, Token2022>,
}

pub fn handler(context: Context<CloseAccountConstraints>) -> Result<()> {
    // cpi to token extensions programs to close mint account
    // alternatively, this can also be done in the client
    close_account(CpiContext::new(
        context.accounts.token_program.key(),
        CloseAccount {
            account: context.accounts.mint_account.to_account_info(),
            destination: context.accounts.authority.to_account_info(),
            authority: context.accounts.authority.to_account_info(),
        },
    ))?;
    Ok(())
}
