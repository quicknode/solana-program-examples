use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::constants::PRICE_FEED_SEED;
use crate::state::PriceFeed;

/// Test stand-in for a Switchboard On-Demand feed: writes a price directly so
/// LiteSVM tests are deterministic. In production the reserve points at a real
/// Switchboard feed instead and this handler is unused.
///
/// The feed PDA is seeded by `[b"price_feed", authority, mint]`, so each
/// authority can only ever write its own feed — there is no shared per-mint
/// feed to race for. A reserve trusts exactly one feed account: the one the
/// market owner passed to `init_reserve`.
pub fn handle_set_price(
    context: Context<SetPrice>,
    price_mantissa: i128,
    exponent: i32,
) -> Result<()> {
    let feed = &mut context.accounts.price_feed;
    feed.authority = context.accounts.authority.key();
    feed.mint = context.accounts.mint.key();
    feed.bump = context.bumps.price_feed;
    feed.price_mantissa = price_mantissa;
    feed.exponent = exponent;
    feed.last_updated_slot = Clock::get()?.slot;
    Ok(())
}

#[derive(Accounts)]
pub struct SetPrice<'info> {
    // The authority is part of the seeds: a signer can only ever address (and
    // therefore write) the feed derived from their own key.
    #[account(
        init_if_needed,
        payer = authority,
        space = PriceFeed::DISCRIMINATOR.len() + PriceFeed::INIT_SPACE,
        seeds = [PRICE_FEED_SEED, authority.key().as_ref(), mint.key().as_ref()],
        bump,
    )]
    pub price_feed: Account<'info, PriceFeed>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
}
