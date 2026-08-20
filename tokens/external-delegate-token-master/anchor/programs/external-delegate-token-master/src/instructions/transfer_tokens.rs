use anchor_lang::prelude::*;
use anchor_spl::token_interface::{
    transfer_checked, Mint, TokenAccount, TokenInterface, TransferChecked,
};

use crate::{
    build_transfer_authorization_message, verify_ethereum_signature, ErrorCode, UserAccount,
};

#[derive(Accounts)]
pub struct TransferTokensAccountConstraints {
    #[account(mut)]
    pub user_account: BorshAccount<UserAccount>,

    #[account(address = user_account.authority)]
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
    context: &mut Context<TransferTokensAccountConstraints>,
    amount: u64,
    signature: [u8; 65],
) -> Result<()> {
    // Copy out what the message needs, so the shared borrow ends before the
    // nonce bump below takes a mutable one.
    let user_account_key = *context.accounts.user_account.address();
    let user_account = &context.accounts.user_account;

    // Rebuild the authorized message onchain so the signature commits to
    // this exact transfer (amount, recipient, and the current nonce).
    let message = build_transfer_authorization_message(
        &user_account_key,
        amount,
        context.accounts.recipient_token_account.address(),
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
        from: context.accounts.user_token_account.cpi_handle_mut(),
        mint: context.accounts.mint.cpi_handle(),
        to: context.accounts.recipient_token_account.cpi_handle_mut(),
        authority: context.accounts.user_pda.cpi_handle(),
    };

    transfer_checked(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            transfer_accounts,
            &[&[user_account_key.as_ref(), &[context.bumps.user_pda]]],
        ),
        amount,
        context.accounts.mint.decimals(),
    )?;

    Ok(())
}
