use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::constants::PRICE_FEED_SEED;
use crate::state::{LendingMarket, PriceFeed};

/// Test stand-in for a Switchboard On-Demand feed: writes a price directly so
/// LiteSVM tests are deterministic. In production the reserve points at a real
/// Switchboard feed instead and this handler is unused.
///
/// The feed PDA is seeded by `[b"price_feed", market, mint]` and writing it
/// requires the market's `owner` to sign, so a market's prices can only be set
/// by that market and never squatted by an outsider.
pub fn handle_set_price(
    context: &mut Context<SetPrice>,
    price_mantissa: i128,
    exponent: i32,
) -> Result<()> {
    let feed = &mut context.accounts.price_feed;
    feed.market = *context.accounts.lending_market.address();
    feed.mint = *context.accounts.mint.address();
    feed.bump = context.bumps.price_feed;
    feed.price_mantissa = price_mantissa;
    feed.exponent = exponent;
    feed.last_updated_slot = Clock::get()?.slot;
    Ok(())
}

#[derive(Accounts)]
pub struct SetPrice {
    // Only the market's owner may publish its prices.
    #[account(has_one = owner)]
    pub lending_market: BorshAccount<LendingMarket>,

    #[account(mut)]
    pub owner: Signer,

    #[account(
        init_if_needed,
        payer = owner,
        space = PriceFeed::DISCRIMINATOR.len() + PriceFeed::INIT_SPACE,
        seeds = [PRICE_FEED_SEED, lending_market.address().as_ref(), mint.address().as_ref()],
        bump,
    )]
    pub price_feed: BorshAccount<PriceFeed>,

    pub mint: InterfaceAccount<Mint>,

    pub system_program: Program<System>,
}
