use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::UserAccount;

#[derive(Accounts)]
pub struct AuthorityTransferAccountConstraints {
    #[account(has_one = authority)]
    pub user_account: BorshAccount<UserAccount>,

    pub authority: Signer,

    pub mint: InterfaceAccount<Mint>,

    #[account(mut)]
    pub user_token_account: InterfaceAccount<TokenAccount>,

    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<TokenAccount>,

    #[account(
        seeds = [user_account.address().as_ref()],
        bump,
    )]
    pub user_pda: SystemAccount,

    pub token_program: Interface<'static, TokenInterface>,
}

pub fn handler(
    context: &mut Context<AuthorityTransferAccountConstraints>,
    amount: u64,
) -> Result<()> {
    let transfer_accounts = TransferChecked {
        from: context.accounts.user_token_account.cpi_handle_mut(),
        mint: context.accounts.mint.cpi_handle(),
        to: context.accounts.recipient_token_account.cpi_handle_mut(),
        authority: context.accounts.user_pda.cpi_handle(),
    };

    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            transfer_accounts,
            &[&[
                context.accounts.user_account.address().as_ref(),
                &[context.bumps.user_pda],
            ]],
        ),
        amount,
        context.accounts.mint.decimals(),
    )?;

    Ok(())
}
