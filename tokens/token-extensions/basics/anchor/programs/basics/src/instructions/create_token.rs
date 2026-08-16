use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token_interface::{Mint, TokenInterface};

#[derive(Accounts)]
#[instruction(token_name: String)]
pub struct CreateTokenAccountConstraints {
    #[account(mut)]
    pub signer: Signer,
    #[account(
        init,
        payer = signer,
        mint::decimals = 6,
        mint::authority = signer,
        // Required when the token program is an `Interface`: without it
        // the init CPI is rejected with InvalidArgument.
        mint::token_program = token_program,
        seeds = [b"token-2022-token", signer.address().as_ref(), token_name.as_bytes()],
        bump,
    )]
    pub mint: InterfaceAccount<Mint>,
    pub system_program: Program<System>,
    pub token_program: Interface<'static, TokenInterface>,
}

pub fn handler(
    _context: &mut Context<CreateTokenAccountConstraints>,
    _token_name: String,
) -> Result<()> {
    msg!("Create Token");
    Ok(())
}
