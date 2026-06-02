use anchor_lang::prelude::*;

use crate::constants::MAX_PRICE_STALENESS_SLOTS;
use crate::errors::LendingError;
use crate::math::price_mantissa_to_scaled;

/// A price for one token, denominated in the market's quote currency.
///
/// The layout mirrors a Switchboard On-Demand pull feed: a signed mantissa plus
/// an exponent (`price = price_mantissa * 10^exponent`) and the slot the value
/// was written. In production this account would be the real Switchboard feed
/// and the program would decode it with the `switchboard-on-demand` crate
/// (`PullFeedAccountData`): `price_mantissa = current_result.value`,
/// `exponent = -18`, `last_updated_slot = current_result.slot`. Here the
/// `set_price` handler writes it directly so LiteSVM tests are deterministic.
#[account]
#[derive(InitSpace)]
pub struct PriceFeed {
    pub mint: Pubkey,

    pub price_mantissa: i128,

    pub exponent: i32,

    pub last_updated_slot: u64,

    /// Account permitted to call `set_price`. In production this field is unused
    /// because the feed is owned by Switchboard, not this program.
    pub authority: Pubkey,

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
