use anchor_lang::prelude::*;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{
    FIXED_POINT_SCALE, LENDING_MARKET_SEED, LIQUIDITY_VAULT_SEED, RESERVE_SEED, SHARE_MINT_SEED,
};
use crate::state::{LendingMarket, PriceFeed, Reserve, ReserveConfig};

pub fn handle_init_reserve(context: Context<InitReserve>, config: ReserveConfig) -> Result<()> {
    config.validate()?;

    let reserve = &mut context.accounts.reserve;
    reserve.lending_market = context.accounts.lending_market.key();
    reserve.liquidity_mint = context.accounts.liquidity_mint.key();
    reserve.liquidity_vault = context.accounts.liquidity_vault.key();
    reserve.share_mint = context.accounts.share_mint.key();
    reserve.price_feed = context.accounts.price_feed.key();
    reserve.liquidity_decimals = context.accounts.liquidity_mint.decimals;
    reserve.available_liquidity = 0;
    reserve.share_mint_supply = 0;
    reserve.borrowed_amount_scaled = 0;
    reserve.cumulative_borrow_rate_index = FIXED_POINT_SCALE;
    reserve.last_update_slot = Clock::get()?.slot;
    reserve.config = config;
    reserve.bump = context.bumps.reserve;
    Ok(())
}

#[derive(Accounts)]
pub struct InitReserve<'info> {
    #[account(
        has_one = owner,
        seeds = [LENDING_MARKET_SEED, owner.key().as_ref()],
        bump = lending_market.bump,
    )]
    pub lending_market: Account<'info, LendingMarket>,

    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer = owner,
        space = Reserve::DISCRIMINATOR.len() + Reserve::INIT_SPACE,
        seeds = [RESERVE_SEED, lending_market.key().as_ref(), liquidity_mint.key().as_ref()],
        bump,
    )]
    pub reserve: Account<'info, Reserve>,

    pub liquidity_mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = owner,
        token::mint = liquidity_mint,
        token::authority = reserve,
        seeds = [LIQUIDITY_VAULT_SEED, reserve.key().as_ref()],
        bump,
    )]
    pub liquidity_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init,
        payer = owner,
        mint::decimals = liquidity_mint.decimals,
        mint::authority = reserve,
        seeds = [SHARE_MINT_SEED, reserve.key().as_ref()],
        bump,
    )]
    pub share_mint: InterfaceAccount<'info, Mint>,

    #[account(constraint = price_feed.mint == liquidity_mint.key() @ crate::errors::LendingError::InvalidConfig)]
    pub price_feed: Account<'info, PriceFeed>,

    pub token_program: Interface<'info, TokenInterface>,

    pub system_program: Program<'info, System>,
}
