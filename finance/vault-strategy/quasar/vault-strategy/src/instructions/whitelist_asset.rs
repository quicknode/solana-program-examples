use quasar_lang::prelude::*;
use quasar_spl::prelude::*;

use crate::state::{Registry, WhitelistEntry, WhitelistEntryInner};

#[derive(Accounts)]
pub struct WhitelistAssetAccountConstraints {
    #[account(mut)]
    pub authority: Signer,

    #[account(address = Registry::seeds(authority.address()), has_one(authority))]
    pub registry: Account<Registry>,

    pub asset_mint: Account<Mint>,

    #[account(
        init,
        payer = authority,
        address = WhitelistEntry::seeds(registry.address(), asset_mint.address()),
    )]
    pub whitelist_entry: Account<WhitelistEntry>,

    pub rent: Sysvar<Rent>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_whitelist_asset(
    accounts: &mut WhitelistAssetAccountConstraints,
    price_feed: Address,
    bumps: &WhitelistAssetAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    accounts.whitelist_entry.set_inner(WhitelistEntryInner {
        registry: *accounts.registry.address(),
        mint: *accounts.asset_mint.address(),
        price_feed,
        bump: bumps.whitelist_entry,
    });
    Ok(())
}
