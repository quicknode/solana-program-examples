use anchor_lang::prelude::*;

use crate::{AllowList, AllowedWallet, ALLOWED_WALLET_SEED, ALLOW_LIST_SEED};

#[derive(Accounts)]
pub struct DisallowWalletAccountConstraints {
    #[account(mut, address = allow_list.authority)]
    pub authority: Signer,
    /// CHECK: only its address is used, as a seed; the allow list's own seeds
    /// tie it to this mint.
    pub mint: UncheckedAccount,
    #[account(seeds = [ALLOW_LIST_SEED, mint.address().as_ref()], bump = allow_list.bump)]
    pub allow_list: BorshAccount<AllowList>,
    /// CHECK: the wallet losing its permission; only its address is used.
    pub wallet: UncheckedAccount,
    /// Closing the account is the whole operation: rent goes back to the authority.
    #[account(
        mut,
        close = authority,
        seeds = [ALLOWED_WALLET_SEED, mint.address().as_ref(), wallet.address().as_ref()],
        bump = allowed_wallet.bump
    )]
    pub allowed_wallet: BorshAccount<AllowedWallet>,
}

pub fn handler(_context: &mut Context<DisallowWalletAccountConstraints>) -> Result<()> {
    Ok(())
}
