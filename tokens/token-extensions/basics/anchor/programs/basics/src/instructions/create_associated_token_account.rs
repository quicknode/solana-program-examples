use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct CreateAssociatedTokenAccountAccountConstraints {
    #[account(mut)]
    pub signer: Signer,
    pub mint: InterfaceAccount<Mint>,
    #[account(
        init,
        associated_token::mint = mint,
        payer = signer,
        associated_token::authority = signer,
    )]
    pub token_account: InterfaceAccount<TokenAccount>,
    pub system_program: Program<System>,
    pub token_program: Interface<'static, TokenInterface>,
    pub associated_token_program: Program<AssociatedToken>,
}

pub fn handler(
    _context: &mut Context<CreateAssociatedTokenAccountAccountConstraints>,
) -> Result<()> {
    msg!("Create Associated Token Account");
    Ok(())
}
