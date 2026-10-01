use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::FundError;
use crate::state::{ApprovedAsset, AssetConfig, Fund, Registry, MAX_ASSETS};

#[derive(Accounts)]
pub struct AddAssetAccountConstraints<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    #[account(
        mut,
        has_one = manager,
        has_one = registry @ FundError::InvalidRegistry,
        seeds = [b"fund", fund.index.to_le_bytes().as_ref()],
        bump = fund.bump
    )]
    pub fund: Box<Account<'info, Fund>>,

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
        seeds = [b"asset", fund.key().as_ref(), &[fund.asset_count]],
        bump
    )]
    pub asset_config: Box<Account<'info, AssetConfig>>,

    /// Fund-owned vault for this asset.
    #[account(
        init,
        payer = manager,
        associated_token::mint = asset_mint,
        associated_token::authority = fund,
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
    let fund = &mut context.accounts.fund;

    require!(fund.asset_count < MAX_ASSETS, FundError::TooManyAssets);

    let new_total = (fund.total_weight_bps as u32)
        .checked_add(weight_bps as u32)
        .ok_or(FundError::MathOverflow)?;
    require!(new_total <= 10_000, FundError::WeightOverflow);

    let index = fund.asset_count;

    context.accounts.asset_config.set_inner(AssetConfig {
        fund: fund.key(),
        index,
        mint: context.accounts.asset_mint.key(),
        decimals: context.accounts.asset_mint.decimals,
        // Copied from the registry entry, never supplied by the manager.
        price_feed: context.accounts.approved_asset.price_feed,
        vault: context.accounts.vault_asset.key(),
        weight_bps,
        bump: context.bumps.asset_config,
    });

    fund.asset_count = index.checked_add(1).ok_or(FundError::MathOverflow)?;
    fund.total_weight_bps = new_total as u16;

    Ok(())
}
