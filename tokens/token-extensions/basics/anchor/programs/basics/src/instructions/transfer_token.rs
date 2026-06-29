use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(Accounts)]
pub struct TransferTokenAccountConstraints<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,
    #[account(mut)]
    pub from: InterfaceAccount<'info, TokenAccount>,
    pub to: SystemAccount<'info>,
    #[account(
        init,
        associated_token::mint = mint,
        payer = signer,
        associated_token::authority = to
    )]
    pub to_ata: InterfaceAccount<'info, TokenAccount>,
    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub fn handler(context: Context<TransferTokenAccountConstraints>, amount: u64) -> Result<()> {
    let cpi_accounts = TransferChecked {
        from: context.accounts.from.to_account_info().clone(),
        mint: context.accounts.mint.to_account_info().clone(),
        to: context.accounts.to_ata.to_account_info().clone(),
        authority: context.accounts.signer.to_account_info(),
    };
    let cpi_program = context.accounts.token_program.key();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    token_interface::transfer_checked(cpi_context, amount, context.accounts.mint.decimals)?;
    msg!("Transfer Token");
    Ok(())
}
