use anchor_lang::prelude::*;

use crate::{AllowList, AllowedWallet, ALLOWED_WALLET_SEED, ALLOW_LIST_SEED};

#[derive(Accounts)]
pub struct AllowWalletAccountConstraints {
    #[account(mut, address = allow_list.authority)]
    pub authority: Signer,
    /// CHECK: only its address is used, as a seed; the allow list's own seeds
    /// tie it to this mint.
    pub mint: UncheckedAccount,
    #[account(seeds = [ALLOW_LIST_SEED, mint.address().as_ref()], bump = allow_list.bump)]
    pub allow_list: BorshAccount<AllowList>,
    /// CHECK: the wallet being allowed to receive the token. Any address: a
    /// person's wallet or a PDA owned by another program.
    pub wallet: UncheckedAccount,
    #[account(
        init,
        seeds = [ALLOWED_WALLET_SEED, mint.address().as_ref(), wallet.address().as_ref()],
        bump,
        payer = authority,
        space = AllowedWallet::DISCRIMINATOR.len() + AllowedWallet::INIT_SPACE
    )]
    pub allowed_wallet: BorshAccount<AllowedWallet>,
    pub system_program: Program<System>,
}

pub fn handler(context: &mut Context<AllowWalletAccountConstraints>) -> Result<()> {
    context.accounts.allowed_wallet.bump = context.bumps.allowed_wallet;
    Ok(())
}
