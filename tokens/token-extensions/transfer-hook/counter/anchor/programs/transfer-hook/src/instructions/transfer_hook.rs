use anchor_lang::prelude::*;
use anchor_spl::token;
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::{check_is_transferring, CounterAccount, TransferError};

// Order of accounts matters for this struct.
// The first 4 accounts are the accounts required for token transfer (source, mint, destination, owner)
// Remaining accounts are the extra accounts required from the ExtraAccountMetaList account
// These accounts are provided via CPI to this program from the token2022 program
#[derive(Accounts)]
pub struct TransferHookAccountConstraints {
    #[account(token::mint = mint, token::authority = owner)]
    pub source_token: InterfaceAccount<TokenAccount>,
    pub mint: InterfaceAccount<Mint>,
    #[account(token::mint = mint)]
    pub destination_token: InterfaceAccount<TokenAccount>,
    /// CHECK: source token account owner, can be SystemAccount or PDA owned by another program
    pub owner: UncheckedAccount,
    /// CHECK: ExtraAccountMetaList Account,
    #[account(seeds = [b"extra-account-metas", mint.address().as_ref()], bump)]
    pub extra_account_meta_list: UncheckedAccount,
    #[account(mut, seeds = [b"counter"], bump)]
    pub counter_account: BorshAccount<CounterAccount>,
}

pub fn handler(context: &mut Context<TransferHookAccountConstraints>, amount: u64) -> Result<()> {
    // Fail this instruction if it is not called from within a transfer hook
    check_is_transferring(context)?;

    // A hook can reject a transfer by returning an error. This one only logs,
    // so the example stays runnable: `amount` arrives in minor units, so any
    // transfer of a token with decimals clears 50 immediately. Return
    // `err!(TransferError::AmountTooBig)` here to make the limit binding, and
    // pick a threshold in the mint's own minor units.
    if amount > 50 {
        msg!("The amount is too big: {}", amount);
    }

    // Increment the transfer count safely and write it back, so the count
    // survives the transfer that produced it.
    let count = context
        .accounts
        .counter_account
        .counter
        .checked_add(1)
        .ok_or(TransferError::CounterOverflow)?;
    context.accounts.counter_account.counter = count;

    msg!("This token has been transferred {} times", count);

    Ok(())
}
