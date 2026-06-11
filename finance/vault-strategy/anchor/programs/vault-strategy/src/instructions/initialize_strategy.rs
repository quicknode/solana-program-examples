use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::VaultError;
use crate::state::Strategy;

/// Highest annual management fee a manager may set, in basis points (10%).
/// `collect_fees` mints shares to the manager and dilutes every depositor,
/// so an uncapped fee would let a manager drain the vault by configuration;
/// 10% per year is already far above typical fund management fees.
pub const MAX_FEE_BPS: u16 = 1_000;

#[derive(Accounts)]
pub struct InitializeStrategyAccountConstraints<'info> {
    #[account(mut)]
    pub manager: Signer<'info>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,

    pub asset_mint_a: InterfaceAccount<'info, Mint>,

    pub asset_mint_b: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = manager,
        space = Strategy::DISCRIMINATOR.len() + Strategy::INIT_SPACE,
        seeds = [b"strategy", manager.key().as_ref()],
        bump
    )]
    pub strategy: Account<'info, Strategy>,

    #[account(
        init,
        payer = manager,
        mint::decimals = 6,
        mint::authority = strategy,
        mint::freeze_authority = strategy,
        mint::token_program = token_program,
        seeds = [b"share_mint", strategy.key().as_ref()],
        bump
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    /// Vault's USDC token account - strategy PDA is the authority
    #[account(
        init,
        payer = manager,
        associated_token::mint = usdc_mint,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_usdc: InterfaceAccount<'info, TokenAccount>,

    /// Vault's asset_a token account - strategy PDA is the authority
    #[account(
        init,
        payer = manager,
        associated_token::mint = asset_mint_a,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset_a: InterfaceAccount<'info, TokenAccount>,

    /// Vault's asset_b token account - strategy PDA is the authority
    #[account(
        init,
        payer = manager,
        associated_token::mint = asset_mint_b,
        associated_token::authority = strategy,
        associated_token::token_program = token_program
    )]
    pub vault_asset_b: InterfaceAccount<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_initialize_strategy(
    context: Context<InitializeStrategyAccountConstraints>,
    weight_bps_a: u16,
    weight_bps_b: u16,
    fee_bps: u16,
    swap_router: Pubkey,
    price_feed_a: Pubkey,
    price_feed_b: Pubkey,
) -> Result<()> {
    require!(
        weight_bps_a
            .checked_add(weight_bps_b)
            .ok_or(VaultError::InvalidWeights)?
            == 10_000,
        VaultError::InvalidWeights
    );

    require!(fee_bps <= MAX_FEE_BPS, VaultError::FeeTooHigh);

    let clock = Clock::get()?;

    context.accounts.strategy.set_inner(Strategy {
        manager: context.accounts.manager.key(),
        share_mint: context.accounts.share_mint.key(),
        usdc_mint: context.accounts.usdc_mint.key(),
        asset_mint_a: context.accounts.asset_mint_a.key(),
        asset_mint_b: context.accounts.asset_mint_b.key(),
        weight_bps_a,
        weight_bps_b,
        fee_bps,
        total_shares: 0,
        last_fee_accrual_timestamp: clock.unix_timestamp,
        swap_router,
        price_feed_a,
        price_feed_b,
        bump: context.bumps.strategy,
    });

    Ok(())
}
