use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::UserAccount;

#[derive(Accounts)]
pub struct AuthorityTransferAccountConstraints<'info> {
    #[account(has_one = authority)]
    pub user_account: Account<'info, UserAccount>,

    pub authority: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    #[account(mut)]
    pub user_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(mut)]
    pub recipient_token_account: InterfaceAccount<'info, TokenAccount>,

    #[account(
        seeds = [user_account.key().as_ref()],
        bump,
    )]
    pub user_pda: SystemAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handler(context: Context<AuthorityTransferAccountConstraints>, amount: u64) -> Result<()> {
    let transfer_accounts = TransferChecked {
        from: context.accounts.user_token_account.to_account_info(),
        mint: context.accounts.mint.to_account_info(),
        to: context.accounts.recipient_token_account.to_account_info(),
        authority: context.accounts.user_pda.to_account_info(),
    };

    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.key(),
            transfer_accounts,
            &[&[
                context.accounts.user_account.key().as_ref(),
                &[context.bumps.user_pda],
            ]],
        ),
        amount,
        context.accounts.mint.decimals,
    )?;

    Ok(())
}
