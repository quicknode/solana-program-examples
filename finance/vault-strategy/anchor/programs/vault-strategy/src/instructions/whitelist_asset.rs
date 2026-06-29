use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::state::{Registry, WhitelistEntry};

#[derive(Accounts)]
pub struct WhitelistAssetAccountConstraints<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        has_one = authority,
        seeds = [b"registry", authority.key().as_ref()],
        bump = registry.bump
    )]
    pub registry: Account<'info, Registry>,

    pub asset_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = authority,
        space = WhitelistEntry::DISCRIMINATOR.len() + WhitelistEntry::INIT_SPACE,
        seeds = [b"whitelist", registry.key().as_ref(), asset_mint.key().as_ref()],
        bump
    )]
    pub whitelist_entry: Account<'info, WhitelistEntry>,

    pub system_program: Program<'info, System>,
}

pub fn handle_whitelist_asset(
    context: Context<WhitelistAssetAccountConstraints>,
    price_feed: Pubkey,
) -> Result<()> {
    context.accounts.whitelist_entry.set_inner(WhitelistEntry {
        registry: context.accounts.registry.key(),
        mint: context.accounts.asset_mint.key(),
        price_feed,
        bump: context.bumps.whitelist_entry,
    });
    Ok(())
}
