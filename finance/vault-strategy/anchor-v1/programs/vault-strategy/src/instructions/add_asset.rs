use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::VaultError;
use crate::state::{ApprovedAsset, AssetConfig, Registry, Strategy, MAX_ASSETS};

#[derive(Accounts)]
pub struct AddAssetAccountConstraints<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        mut,
        has_one = manager,
        has_one = registry @ VaultError::InvalidRegistry,
        seeds = [b"strategy", strategy.index.to_le_bytes().as_ref()],
        bump = strategy.bump
    )]
    pub strategy: Box<Account<'info, Strategy>>,

    pub registry: Box<Account<'info, Registry>>,

    pub asset_mint: Box<InterfaceAccount<'info, Mint>>,

    /// Proof the mint is approved, and the source of its official price feed.
    /// Seeds tie it to this registry and this mint; existence means approved.
    #[account(
        seeds = [b"approved_asset", registry.key().as_ref(), asset_mint.key().as_ref()],
        bump = approved_asset.bump
    )]
    pub approved_asset: Box<Account<'info, ApprovedAsset>>,

    #[account(
        init,
        payer = manager,
        space = AssetConfig::DISCRIMINATOR.len() + AssetConfig::INIT_SPACE,
        seeds = [b"asset", strategy.key().as_ref(), &[strategy.asset_count]],
        bump
    )]
    pub asset_config: Box<Account<'info, AssetConfig>>,

    /// Strategy-owned vault for this asset.
    #[account(
        init,
        payer = manager,
        associated_token::mint = asset_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset: Box<InterfaceAccount<'info, TokenAccount>>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_add_asset(
    context: Context<AddAssetAccountConstraints>,
    weight_bps: u16,
) -> Result<()> {
    let strategy = &mut context.accounts.strategy;

    require!(strategy.asset_count < MAX_ASSETS, VaultError::TooManyAssets);

    let new_total = (strategy.total_weight_bps as u32)
        .checked_add(weight_bps as u32)
        .ok_or(VaultError::MathOverflow)?;
    require!(new_total <= 10_000, VaultError::WeightOverflow);

    let index = strategy.asset_count;

    context.accounts.asset_config.set_inner(AssetConfig {
        strategy: strategy.key(),
        index,
        mint: context.accounts.asset_mint.key(),
        // Copied from the registry entry, never supplied by the manager.
        price_feed: context.accounts.approved_asset.price_feed,
        vault: context.accounts.vault_asset.key(),
        weight_bps,
        bump: context.bumps.asset_config,
    });

    strategy.asset_count = index.checked_add(1).ok_or(VaultError::MathOverflow)?;
    strategy.total_weight_bps = new_total as u16;

    Ok(())
}
