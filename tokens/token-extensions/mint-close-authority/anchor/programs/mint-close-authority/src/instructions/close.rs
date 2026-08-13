use anchor_lang::prelude::*;
use anchor_spl::{
    token_2022::{close_account, CloseAccount},
    token_interface::{Mint, Token2022},
};

#[derive(Accounts)]
pub struct CloseAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(
        mut,
        extensions::close_authority::authority = authority,
    )]
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
}

pub fn handler(context: &mut Context<CloseAccountConstraints>) -> Result<()> {
    // cpi to token extensions programs to close mint account
    // alternatively, this can also be done in the client
    close_account(CpiContext::new(
        context.accounts.token_program.address(),
        CloseAccount {
            account: context.accounts.mint_account.cpi_handle_mut(),
            destination: context.accounts.authority.cpi_handle_mut(),
            authority: context.accounts.authority.cpi_handle(),
        },
    ))?;
    Ok(())
}
