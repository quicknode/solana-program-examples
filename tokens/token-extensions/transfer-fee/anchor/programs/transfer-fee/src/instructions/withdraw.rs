use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    withdraw_withheld_tokens_from_mint, Mint, Token2022, TokenAccount,
    WithdrawWithheldTokensFromMint,
};

#[derive(Accounts)]
pub struct WithdrawAccountConstraints {
    pub authority: Signer,

    #[account(mut)]
    pub mint_account: InterfaceAccount<Mint>,
    #[account(mut)]
    pub token_account: InterfaceAccount<TokenAccount>,
    pub token_program: Program<Token2022>,
}

// transfer fees "harvested" to the mint account can then be withdraw by the withdraw authority
// this transfers fees on the mint account to the specified token account
pub fn handle_process_withdraw(context: &mut Context<WithdrawAccountConstraints>) -> Result<()> {
    withdraw_withheld_tokens_from_mint(CpiContext::new(
        context.accounts.token_program.address(),
        WithdrawWithheldTokensFromMint {
            mint: context.accounts.mint_account.cpi_handle_mut(),
            destination: context.accounts.token_account.cpi_handle_mut(),
            authority: context.accounts.authority.cpi_handle(),
        },
    ))?;
    Ok(())
}
