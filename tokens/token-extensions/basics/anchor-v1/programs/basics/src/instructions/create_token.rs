use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenInterface};

#[derive(Accounts)]
#[instruction(token_name: String)]
pub struct CreateTokenAccountConstraints<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(
        init,
        payer = signer,
        mint::decimals = 6,
        mint::authority = signer.key(),
        seeds = [b"token-2022-token", signer.key().as_ref(), token_name.as_bytes()],
        bump,
    )]
    pub mint: InterfaceAccount<'info, Mint>,
    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(_context: Context<CreateTokenAccountConstraints>, _token_name: String) -> Result<()> {
    msg!("Create Token");
    Ok(())
}
