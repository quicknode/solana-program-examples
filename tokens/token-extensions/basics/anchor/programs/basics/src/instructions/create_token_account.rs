use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct CreateTokenAccountAccountConstraints {
    #[account(mut)]
    pub signer: Signer,
    pub mint: InterfaceAccount<Mint>,
    #[account(
        init,
        token::mint = mint,
        token::authority = signer,
        // Required when the token program is an `Interface`: without it
        // the init CPI is rejected with InvalidArgument.
        token::token_program = token_program,
        payer = signer,
        seeds = [b"token-2022-token-account", signer.address().as_ref(), mint.address().as_ref()],
        bump,
    )]
    pub token_account: InterfaceAccount<TokenAccount>,
    pub system_program: Program<System>,
    pub token_program: Interface<'static, TokenInterface>,
}

pub fn handler(_context: &mut Context<CreateTokenAccountAccountConstraints>) -> Result<()> {
    msg!("Create Token Account");
    Ok(())
}
