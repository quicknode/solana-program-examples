use anchor_lang::prelude::*;

use crate::constants::{BPS_DENOMINATOR, MAX_PRICE_STALENESS_SLOTS};
use crate::errors::LendingError;
use crate::math::price_mantissa_to_scaled;

/// A price for one token, denominated in the market's quote currency.
/// PDA seeds: `[b"price_feed", market, mint]` — scoped to a market (not to any
/// individual), so each market prices its own assets and one market can never
/// write another's feed. Only the market's `owner` may write it (`set_price`).
///
/// The layout mirrors an oracle price feed such as Pyth's: a signed mantissa
/// plus an exponent (`price = price_mantissa * 10^exponent`), the publisher's
/// confidence interval in the same units as the mantissa, and the slot the
/// value was written. In production this account would be a Pyth
/// `PriceUpdateV2` owned by the Pyth Receiver program (`basics/pyth` reads
/// one), mapped as `price_mantissa = price_message.price`,
/// `exponent = price_message.exponent`, `confidence = price_message.conf` and
/// `last_updated_slot = posted_slot`, after checking the update's `feed_id`.
/// Here the `set_price` handler writes it directly so LiteSVM tests are
/// deterministic.
#[account(borsh)]
#[derive(InitSpace)]
pub struct PriceFeed {
    /// The lending market this feed serves; part of the PDA seeds.
    pub market: Address,

    pub mint: Address,

    pub price_mantissa: i128,

    pub exponent: i32,

    /// How far the publisher's price sources disagree, as half the width of
    /// the interval around `price_mantissa`, in the mantissa's units (Pyth's
    /// `conf`). A reserve refuses a price whose band is wider than its
    /// `max_confidence_bps` of the price, so a market that has stopped
    /// trading, or whose sources disagree, cannot be valued against.
    pub confidence: u64,

    pub last_updated_slot: u64,

    pub bump: u8,
}

impl PriceFeed {
    /// The price multiplied by FIXED_POINT_SCALE, after asserting the feed is
    /// fresh, positive, and no less certain than `max_confidence_bps` of the
    /// price allows. Combining the price exponent with the fixed-point scale
    /// (see `price_mantissa_to_scaled`) keeps the conversion overflow-safe.
    pub fn price_scaled(&self, current_slot: u64, max_confidence_bps: u16) -> Result<u128> {
        let age = current_slot
            .checked_sub(self.last_updated_slot)
            .ok_or(LendingError::MathOverflow)?;
        require!(
            age <= MAX_PRICE_STALENESS_SLOTS,
            LendingError::StalePriceFeed
        );

        // Restart handling. A cluster halt stops the slot count but not the
        // wall clock, so after a restart a feed can look fresh in slots while
        // its price is hours old. Reject any price stamped at or before the
        // restart slot; the market then pauses valuation until the publisher
        // posts again, rather than lending against a pre-halt price. Zero
        // means the cluster has never restarted.
        let last_restart_slot = crate::last_restart::LastRestartSlot::get()?.last_restart_slot();
        require!(
            last_restart_slot == 0 || self.last_updated_slot > last_restart_slot,
            LendingError::PricePredatesRestart
        );

        require!(self.price_mantissa > 0, LendingError::InvalidOraclePrice);

        // Reject a price the oracle itself is unsure of: the confidence band,
        // as a fraction of the price, must not exceed the reserve's limit.
        // `confidence` shares the mantissa's exponent, so the comparison needs
        // no scaling; it is `confidence / price <= max_confidence_bps / 10_000`
        // with both sides multiplied out so no division truncates.
        let band_scaled = (self.confidence as u128)
            .checked_mul(BPS_DENOMINATOR)
            .ok_or(LendingError::MathOverflow)?;
        let limit_scaled = (self.price_mantissa as u128)
            .checked_mul(max_confidence_bps as u128)
            .ok_or(LendingError::MathOverflow)?;
        require!(
            band_scaled <= limit_scaled,
            LendingError::OracleConfidenceTooWide
        );

        price_mantissa_to_scaled(self.price_mantissa as u128, self.exponent)
    }
}
