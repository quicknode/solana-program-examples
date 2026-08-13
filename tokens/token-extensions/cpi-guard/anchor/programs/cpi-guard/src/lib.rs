use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::{
    token_2022::{transfer_checked, TransferChecked},
    token_interface::{Mint, Token2022, TokenAccount},
};

// Note that you cannot initialize or update the CpiGuard extension through a CPI
// https://github.com/solana-labs/solana-program-library/blob/6968859e2ee0a1764da572de340cdb58e2b4586f/token/program-2022/src/extension/cpi_guard/processor.rs#L44-L46
declare_id!("6tU3MEowU6oxxeDZLSxEwzcEZsZrhBJsfUR6xECvShid");

#[program]
pub mod cpi_guard {
    use super::*;

    pub fn cpi_transfer(context: &mut Context<CpiTransferAccountConstraints>) -> Result<()> {
        transfer_checked(
            CpiContext::new(
                context.accounts.token_program.address(),
                TransferChecked {
                    from: context.accounts.sender_token_account.cpi_handle_mut(),
                    mint: context.accounts.mint_account.cpi_handle(),
                    to: context.accounts.recipient_token_account.cpi_handle_mut(),
                    authority: context.accounts.sender.cpi_handle(),
                },
            ),
            1,
            context.accounts.mint_account.decimals(),
        )?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct CpiTransferAccountConstraints {
    #[account(mut)]
    pub sender: Signer,

    #[account(
        mut,
        token::mint = mint_account
    )]
    pub sender_token_account: InterfaceAccount<TokenAccount>,
    #[account(
        init_if_needed,
        payer = sender,
        seeds = [b"pda"],
        bump,
        token::mint = mint_account,
        token::authority = recipient_token_account,
        token::token_program = token_program
    )]
    pub recipient_token_account: InterfaceAccount<TokenAccount>,
    pub mint_account: InterfaceAccount<Mint>,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}
