use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::token;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

use crate::constants::{
    FIXED_POINT_SCALE, LIQUIDITY_VAULT_SEED, PRICE_FEED_SEED, RESERVE_SEED, SHARE_MINT_SEED,
};
use crate::state::{LendingMarket, PriceFeed, Reserve, ReserveConfig};

pub fn handle_initialize_reserve(
    context: &mut Context<InitializeReserve>,
    config: ReserveConfig,
) -> Result<()> {
    config.validate()?;

    let reserve = &mut context.accounts.reserve;
    reserve.lending_market = *context.accounts.lending_market.address();
    reserve.liquidity_mint = *context.accounts.liquidity_mint.address();
    reserve.liquidity_vault = *context.accounts.liquidity_vault.address();
    reserve.share_mint = *context.accounts.share_mint.address();
    reserve.price_feed = *context.accounts.price_feed.address();
    reserve.liquidity_decimals = context.accounts.liquidity_mint.decimals();
    reserve.available_liquidity = 0;
    reserve.share_mint_supply = 0;
    reserve.borrowed_principal = 0;
    reserve.borrow_accumulation_factor = FIXED_POINT_SCALE;
    reserve.last_update_slot = Clock::get()?.slot;
    reserve.accumulated_protocol_fees = 0;
    reserve.config = config;
    reserve.bump = context.bumps.reserve;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeReserve {
    // The reserve PDA below is seeded by this market's address, so the market is
    // pinned by that seed; we only need to prove the signer owns it.
    pub lending_market: BorshAccount<LendingMarket>,

    #[account(mut, address = lending_market.owner)]
    pub owner: Signer,

    #[account(
        init,
        payer = owner,
        space = Reserve::DISCRIMINATOR.len() + Reserve::INIT_SPACE,
        seeds = [RESERVE_SEED, lending_market.address().as_ref(), liquidity_mint.address().as_ref()],
        bump,
    )]
    pub reserve: BorshAccount<Reserve>,

    pub liquidity_mint: InterfaceAccount<Mint>,

    #[account(
        init,
        payer = owner,
        token::mint = liquidity_mint,
        token::authority = reserve,
        seeds = [LIQUIDITY_VAULT_SEED, reserve.address().as_ref()],
        bump,
    )]
    pub liquidity_vault: InterfaceAccount<TokenAccount>,

    #[account(
        init,
        payer = owner,
        mint::decimals = liquidity_mint.decimals(),
        mint::authority = reserve,
        seeds = [SHARE_MINT_SEED, reserve.address().as_ref()],
        bump,
    )]
    pub share_mint: InterfaceAccount<Mint>,

    // Bound by seeds to this market's feed for this mint — the reserve can only
    // trust the price its own market publishes.
    #[account(
        seeds = [PRICE_FEED_SEED, lending_market.address().as_ref(), liquidity_mint.address().as_ref()],
        bump = price_feed.bump,
    )]
    pub price_feed: BorshAccount<PriceFeed>,

    pub token_program: Interface<'static, TokenInterface>,

    pub system_program: Program<System>,
}
