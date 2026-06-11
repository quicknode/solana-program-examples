use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{build_transfer_authorization_message, verify_ethereum_signature, ErrorCode, UserAccount};

#[derive(Accounts)]
pub struct TransferTokensAccountConstraints<'info> {
    #[account(mut, has_one = authority)]
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

pub fn handler(
    context: Context<TransferTokensAccountConstraints>,
    amount: u64,
    signature: [u8; 65],
) -> Result<()> {
    let user_account = &context.accounts.user_account;
    let user_account_key = user_account.key();

    // Rebuild the authorized message onchain so the signature commits to
    // this exact transfer (amount, recipient, and the current nonce).
    let message = build_transfer_authorization_message(
        &user_account_key,
        amount,
        &context.accounts.recipient_token_account.key(),
        user_account.nonce,
    );

    require!(
        verify_ethereum_signature(&user_account.ethereum_address, &message, &signature),
        ErrorCode::InvalidSignature
    );

    // Consume the nonce before the transfer CPI (checks-effects-interactions),
    // so this signature can never authorize a second execution.
    let user_account = &mut context.accounts.user_account;
    user_account.nonce = user_account
        .nonce
        .checked_add(1)
        .ok_or(ErrorCode::NonceOverflow)?;

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
            &[&[user_account_key.as_ref(), &[context.bumps.user_pda]]],
        ),
        amount,
        context.accounts.mint.decimals,
    )?;

    Ok(())
}
