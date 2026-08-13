use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_interface::{transfer_checked, Mint, TokenAccount, TransferChecked},
};

use crate::{check_is_transferring, CounterAccount, TransferError};

// Order of accounts matters for this struct.
// The first 4 accounts are the accounts required for token transfer (source, mint, destination, owner)
// Remaining accounts are the extra accounts required from the ExtraAccountMetaList account
// These accounts are provided via CPI to this program from the token2022 program
//
// Box<InterfaceAccount> used for source_token, destination_token, wsol_mint,
// delegate_wsol_token_account, and sender_wsol_token_account to avoid exceeding
// the 4096-byte BPF stack frame limit in try_accounts deserialization.
// This struct has 12 accounts - without Box, the generated code uses ~4160 bytes of stack.
#[derive(Accounts)]
pub struct TransferHookAccountConstraints {
    #[account(token::mint = mint, token::authority = owner)]
    pub source_token: Box<InterfaceAccount<TokenAccount>>,
    pub mint: Box<InterfaceAccount<Mint>>,
    #[account(token::mint = mint)]
    pub destination_token: Box<InterfaceAccount<TokenAccount>>,
    /// CHECK: source token account owner, can be SystemAccount or PDA owned by another program
    pub owner: UncheckedAccount,
    /// CHECK: ExtraAccountMetaList Account,
    #[account(seeds = [b"extra-account-metas", mint.address().as_ref()], bump)]
    pub extra_account_meta_list: UncheckedAccount,
    pub wsol_mint: Box<InterfaceAccount<Mint>>,
    pub token_program: Program<Token>,
    pub associated_token_program: Program<AssociatedToken>,
    #[account(
        mut,
        seeds = [b"delegate"],
        bump
    )]
    pub delegate: SystemAccount,
    #[account(
        mut,
        token::mint = wsol_mint,
        token::authority = delegate,
    )]
    pub delegate_wsol_token_account: Box<InterfaceAccount<TokenAccount>>,
    #[account(
        mut,
        token::mint = wsol_mint,
        token::authority = owner,
    )]
    pub sender_wsol_token_account: Box<InterfaceAccount<TokenAccount>>,
    #[account(seeds = [b"counter"], bump)]
    pub counter_account: BorshAccount<CounterAccount>,
}

pub fn handler(context: &mut Context<TransferHookAccountConstraints>, amount: u64) -> Result<()> {
    // Fail this instruction if it is not called from within a transfer hook
    check_is_transferring(&context)?;

    if amount > 50 {
        msg!("The amount is too big {0}", amount);
    }

    context.accounts.counter_account.counter += 1;

    msg!(
        "This token has been transferred {0} times",
        context.accounts.counter_account.counter
    );

    msg!(
        "Is writable mint {0}",
        context.accounts.mint.cpi_handle_mut().is_writable
    );
    msg!(
        "Is destination mint {0}",
        context.accounts.destination_token.cpi_handle_mut().is_writable
    );
    msg!(
        "Is source mint {0}",
        context.accounts.source_token.cpi_handle_mut().is_writable
    );

    let signer_seeds: &[&[&[u8]]] = &[&[b"delegate", &[context.bumps.delegate]]];

    // Transfer WSOL from sender to delegate token account using delegate PDA
    // transfer lamports amount equal to token transfer amount
    transfer_checked(
        CpiContext::new(
            context.accounts.token_program.address(),
            TransferChecked {
                from: context.accounts.sender_wsol_token_account.cpi_handle_mut(),
                mint: context.accounts.wsol_mint.cpi_handle(),
                to: context.accounts.delegate_wsol_token_account.cpi_handle_mut(),
                authority: context.accounts.delegate.cpi_handle(),
            },
        )
        .with_signer(signer_seeds),
        amount,
        context.accounts.wsol_mint.decimals(),
    )?;
    Ok(())
}
