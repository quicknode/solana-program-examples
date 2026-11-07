use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount};

use crate::{check_is_transferring, AllowListError, AllowedWallet, ALLOWED_WALLET_SEED};

// Order of accounts matters for this struct.
// The first 4 accounts are the accounts required for token transfer (source, mint, destination, owner)
// Remaining accounts are the extra accounts required from the ExtraAccountMetaList account
// These accounts are provided via CPI to this program from the token2022 program
#[derive(Accounts)]
pub struct TransferHookAccountConstraints<'info> {
    #[account(token::mint = mint, token::authority = owner)]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(token::mint = mint)]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: source token account owner, can be SystemAccount or PDA owned by another program
    pub owner: UncheckedAccount<'info>,
    /// CHECK: ExtraAccountMetaList Account,
    #[account(seeds = [b"extra-account-metas", mint.key().as_ref()], bump)]
    pub extra_account_meta_list: UncheckedAccount<'info>,
    /// CHECK: the recipient's AllowedWallet PDA. It may not exist: a missing
    /// account is how a wallet that is not allowed shows up, so the handler
    /// reads it by hand instead of letting a typed account fail to load.
    #[account(seeds = [
        ALLOWED_WALLET_SEED,
        mint.key().as_ref(),
        destination_token.owner.as_ref(),
    ], bump)]
    pub allowed_wallet: UncheckedAccount<'info>,
}

pub fn handler(context: Context<TransferHookAccountConstraints>, _amount: u64) -> Result<()> {
    // Fail this instruction if it is not called from within a transfer hook
    check_is_transferring(&context)?;

    // Only this program can put data at one of its own PDAs, so an account at
    // this address that carries the AllowedWallet discriminator was created by
    // `allow_wallet` and not since closed by `disallow_wallet`.
    let data = context.accounts.allowed_wallet.try_borrow_data()?;
    let discriminator = AllowedWallet::DISCRIMINATOR;
    if data.len() < discriminator.len() || &data[..discriminator.len()] != discriminator {
        return err!(AllowListError::RecipientNotAllowed);
    }

    Ok(())
}
