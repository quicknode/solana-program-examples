use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, MintTo, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct MintTokenAccountConstraints {
    #[account(mut)]
    pub signer: Signer,
    #[account(mut)]
    pub mint: InterfaceAccount<Mint>,
    #[account(mut)]
    pub receiver: InterfaceAccount<TokenAccount>,
    pub token_program: Interface<'static, TokenInterface>,
}

pub fn handler(context: &mut Context<MintTokenAccountConstraints>, amount: u64) -> Result<()> {
    let cpi_accounts = MintTo {
        mint: context.accounts.mint.cpi_handle_mut(),
        to: context.accounts.receiver.cpi_handle_mut(),
        authority: context.accounts.signer.cpi_handle(),
    };
    let cpi_program = context.accounts.token_program.address();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    token_interface::mint_to(cpi_context, amount)?;
    msg!("Mint Token");
    Ok(())
}
