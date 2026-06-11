use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, MintTo, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct MintTokenAccountConstraints<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub receiver: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(context: Context<MintTokenAccountConstraints>, amount: u64) -> Result<()> {
    let cpi_accounts = MintTo {
        mint: context.accounts.mint.to_account_info().clone(),
        to: context.accounts.receiver.to_account_info().clone(),
        authority: context.accounts.signer.to_account_info(),
    };
    let cpi_program = context.accounts.token_program.key();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    token_interface::mint_to(cpi_context, amount)?;
    msg!("Mint Token");
    Ok(())
}
