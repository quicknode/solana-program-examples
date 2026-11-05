use anchor_lang::prelude::*;

use crate::{AllowList, AllowedWallet, ALLOWED_WALLET_SEED, ALLOW_LIST_SEED};

#[derive(Accounts)]
pub struct DisallowWalletAccountConstraints<'info> {
    #[account(mut, address = allow_list.authority)]
    pub authority: Signer<'info>,
    /// CHECK: only its address is used, as a seed; the allow list's own seeds
    /// tie it to this mint.
    pub mint: UncheckedAccount<'info>,
    #[account(seeds = [ALLOW_LIST_SEED, mint.key().as_ref()], bump = allow_list.bump)]
    pub allow_list: Account<'info, AllowList>,
    /// CHECK: the wallet losing its permission; only its address is used.
    pub wallet: UncheckedAccount<'info>,
    /// Closing the account is the whole operation: rent goes back to the authority.
    #[account(
        mut,
        close = authority,
        seeds = [ALLOWED_WALLET_SEED, mint.key().as_ref(), wallet.key().as_ref()],
        bump = allowed_wallet.bump
    )]
    pub allowed_wallet: Account<'info, AllowedWallet>,
}

pub fn handler(_context: Context<DisallowWalletAccountConstraints>) -> Result<()> {
    Ok(())
}
