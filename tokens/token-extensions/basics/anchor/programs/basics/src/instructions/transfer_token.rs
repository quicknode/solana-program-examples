use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

#[derive(Accounts)]
pub struct TransferTokenAccountConstraints {
    #[account(mut)]
    pub signer: Signer,
    #[account(mut)]
    pub from: InterfaceAccount<TokenAccount>,
    pub to: SystemAccount,
    #[account(
        init,
        associated_token::mint = mint,
        payer = signer,
        associated_token::authority = to,
        // Required when the token program is an `Interface`: without it the
        // init CPI is rejected with InvalidArgument.
        associated_token::token_program = token_program,
    )]
    pub to_ata: InterfaceAccount<TokenAccount>,
    #[account(mut)]
    pub mint: InterfaceAccount<Mint>,
    pub token_program: Interface<'static, TokenInterface>,
    pub system_program: Program<System>,
    pub associated_token_program: Program<AssociatedToken>,
}

pub fn handler(context: &mut Context<TransferTokenAccountConstraints>, amount: u64) -> Result<()> {
    let cpi_accounts = TransferChecked {
        from: context.accounts.from.cpi_handle_mut(),
        mint: context.accounts.mint.cpi_handle(),
        to: context.accounts.to_ata.cpi_handle_mut(),
        authority: context.accounts.signer.cpi_handle(),
    };
    let cpi_program = context.accounts.token_program.address();
    let cpi_context = CpiContext::new(cpi_program, cpi_accounts);
    token_interface::transfer_checked(cpi_context, amount, context.accounts.mint.decimals())?;
    msg!("Transfer Token");
    Ok(())
}
