use anchor_lang::prelude::*;
use anchor_spl::token_interface::Mint;

use crate::constants::PRICE_FEED_SEED;
use crate::errors::LendingError;
use crate::state::PriceFeed;

/// Test stand-in for a Switchboard On-Demand feed: writes a price directly so
/// LiteSVM tests are deterministic. In production the reserve points at a real
/// Switchboard feed instead and this handler is unused.
pub fn handle_set_price(
    context: Context<SetPrice>,
    price_mantissa: i128,
    exponent: i32,
) -> Result<()> {
    let feed = &mut context.accounts.price_feed;

    // On first creation the authority is unset (default Pubkey); claim it for
    // the signer. On later updates only that authority may write.
    if feed.authority == Pubkey::default() {
        feed.authority = context.accounts.authority.key();
        feed.mint = context.accounts.mint.key();
        feed.bump = context.bumps.price_feed;
    } else {
        require_keys_eq!(
            feed.authority,
            context.accounts.authority.key(),
            LendingError::UnauthorizedPriceFeed
        );
    }

    feed.price_mantissa = price_mantissa;
    feed.exponent = exponent;
    feed.last_updated_slot = Clock::get()?.slot;
    Ok(())
}

#[derive(Accounts)]
pub struct SetPrice<'info> {
    #[account(
        init_if_needed,
        payer = authority,
        space = PriceFeed::DISCRIMINATOR.len() + PriceFeed::INIT_SPACE,
        seeds = [PRICE_FEED_SEED, mint.key().as_ref()],
        bump,
    )]
    pub price_feed: Account<'info, PriceFeed>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
}
