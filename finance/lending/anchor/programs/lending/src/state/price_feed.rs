use anchor_lang::prelude::*;

use crate::constants::MAX_PRICE_STALENESS_SLOTS;
use crate::errors::LendingError;
use crate::math::price_mantissa_to_scaled;

/// A price for one token, denominated in the market's quote currency.
/// PDA seeds: `[b"price_feed", market, mint]` — scoped to a market (not to any
/// individual), so each market prices its own assets and one market can never
/// write another's feed. Only the market's `owner` may write it (`set_price`).
///
/// The layout mirrors a Switchboard On-Demand pull feed: a signed mantissa plus
/// an exponent (`price = price_mantissa * 10^exponent`) and the slot the value
/// was written. In production this account would be the real Switchboard feed
/// and the program would decode it with the `switchboard-on-demand` crate
/// (`PullFeedAccountData`): `price_mantissa = current_result.value`,
/// `exponent = -18`, `last_updated_slot = current_result.slot`. Here the
/// `set_price` handler writes it directly so LiteSVM tests are deterministic.
/// A production read should also reject results whose confidence interval is
/// too wide; this stand-in has no confidence field to check.
#[account]
#[derive(InitSpace)]
pub struct PriceFeed {
    /// The lending market this feed serves; part of the PDA seeds.
    pub market: Pubkey,

    pub mint: Pubkey,

    pub price_mantissa: i128,

    pub exponent: i32,

    pub last_updated_slot: u64,

    pub bump: u8,
}

impl PriceFeed {
    /// The price multiplied by FIXED_POINT_SCALE, after asserting the feed is
    /// fresh and positive. Combining the price exponent with the fixed-point
    /// scale (see `price_mantissa_to_scaled`) keeps the conversion overflow-safe.
    pub fn price_scaled(&self, current_slot: u64) -> Result<u128> {
        let age = current_slot
            .checked_sub(self.last_updated_slot)
            .ok_or(LendingError::MathOverflow)?;
        require!(age <= MAX_PRICE_STALENESS_SLOTS, LendingError::StalePriceFeed);
        require!(self.price_mantissa > 0, LendingError::InvalidOraclePrice);

        price_mantissa_to_scaled(self.price_mantissa as u128, self.exponent)
    }
}
