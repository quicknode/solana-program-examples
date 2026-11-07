use anchor_lang::prelude::*;

use crate::{AllowList, AllowedWallet, ALLOWED_WALLET_SEED, ALLOW_LIST_SEED};

#[derive(Accounts)]
pub struct AllowWalletAccountConstraints<'info> {
    #[account(mut, address = allow_list.authority)]
    pub authority: Signer<'info>,
    /// CHECK: only its address is used, as a seed; the allow list's own seeds
    /// tie it to this mint.
    pub mint: UncheckedAccount<'info>,
    #[account(seeds = [ALLOW_LIST_SEED, mint.key().as_ref()], bump = allow_list.bump)]
    pub allow_list: Account<'info, AllowList>,
    /// CHECK: the wallet being allowed to receive the token. Any address: a
    /// person's wallet or a PDA owned by another program.
    pub wallet: UncheckedAccount<'info>,
    #[account(
        init,
        seeds = [ALLOWED_WALLET_SEED, mint.key().as_ref(), wallet.key().as_ref()],
        bump,
        payer = authority,
        space = AllowedWallet::DISCRIMINATOR.len() + AllowedWallet::INIT_SPACE
    )]
    pub allowed_wallet: Account<'info, AllowedWallet>,
    pub system_program: Program<'info, System>,
}

pub fn handler(context: Context<AllowWalletAccountConstraints>) -> Result<()> {
    context.accounts.allowed_wallet.bump = context.bumps.allowed_wallet;
    Ok(())
}
