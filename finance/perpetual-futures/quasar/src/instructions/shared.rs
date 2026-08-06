//! Money math and the oracle decode, ported verbatim from the Anchor sibling.
//! All integer, all `checked_*`, multiply-before-divide, rounding toward the
//! protocol. Errors are `ProgramError::Custom(code)`; the codes are listed here.

use quasar_lang::{prelude::*, sysvars::Sysvar};

use crate::last_restart::LastRestartSlot;

use crate::constants::{
    BASIS_POINTS_DENOMINATOR, FUNDING_PRECISION, MAX_PRICE_STALENESS_SLOTS, SIDE_LONG,
    SIZE_PRECISION,
};
use crate::state::Pool;

pub mod error {
    pub const ZERO_AMOUNT: u32 = 0;
    pub const LEVERAGE_TOO_HIGH: u32 = 2;
    pub const INVALID_PARAMETER: u32 = 3;
    pub const STALE_PRICE: u32 = 4;
    pub const NON_POSITIVE_PRICE: u32 = 5;
    pub const ORACLE_SCALE_MISMATCH: u32 = 6;
    pub const ORACLE_DATA_TOO_SHORT: u32 = 7;
    pub const SLIPPAGE_EXCEEDED: u32 = 8;
    pub const INSUFFICIENT_LIQUIDITY: u32 = 9;
    pub const POOL_INSOLVENT: u32 = 10;
    pub const POSITION_HEALTHY: u32 = 11;
    pub const POSITION_NOT_HEALTHY: u32 = 12;
    pub const NOTHING_TO_CLAIM: u32 = 13;
    pub const DEPOSIT_TOO_SMALL: u32 = 14;
    pub const AMOUNT_ROUNDS_TO_ZERO: u32 = 15;
    pub const ORACLE_CONFIDENCE_TOO_WIDE: u32 = 16;
    pub const INSUFFICIENT_COLLATERAL: u32 = 17;
    pub const PRICE_PREDATES_RESTART: u32 = 18;
}

#[inline(always)]
pub fn err(code: u32) -> ProgramError {
    ProgramError::Custom(code)
}

#[inline(always)]
fn overflow() -> ProgramError {
    ProgramError::ArithmeticOverflow
}

// Byte layout of the oracle feed account: price (i128), scale (u32),
// last_update_slot (u64), confidence (u64). The tests craft this directly; in
// production it would be a real Switchboard On-Demand feed parsed with signature
// verification.
//
// Like the Anchor sibling, this validates freshness, positivity, and the
// confidence band (`confidence / price`), rejecting a price whose band is too
// wide. A production reader may also prefer the feed's EMA over the spot price;
// the mock omits the EMA to stay minimal. The feed account's owning program is
// NOT checked here — the pool trusts whatever feed its creator configured; a
// production reader must verify the owner is the oracle program.
const PRICE_OFFSET: usize = 0;
const SCALE_OFFSET: usize = PRICE_OFFSET + 16;
const LAST_UPDATE_SLOT_OFFSET: usize = SCALE_OFFSET + 4;
const CONFIDENCE_OFFSET: usize = LAST_UPDATE_SLOT_OFFSET + 8;
const FEED_MINIMUM_LENGTH: usize = CONFIDENCE_OFFSET + 8;

/// Read and validate the oracle price from raw feed bytes. Returns the price as
/// a `u64` in `expected_scale` fixed point. Rejects a price whose confidence
/// band exceeds `max_confidence_bps` of the price.
pub fn read_oracle_price(
    data: &[u8],
    expected_scale: u32,
    current_slot: u64,
    max_confidence_bps: u16,
) -> Result<u64, ProgramError> {
    if data.len() < FEED_MINIMUM_LENGTH {
        return Err(err(error::ORACLE_DATA_TOO_SHORT));
    }

    let price = i128::from_le_bytes(
        data[PRICE_OFFSET..PRICE_OFFSET + 16]
            .try_into()
            .map_err(|_| err(error::ORACLE_DATA_TOO_SHORT))?,
    );
    let scale = u32::from_le_bytes(
        data[SCALE_OFFSET..SCALE_OFFSET + 4]
            .try_into()
            .map_err(|_| err(error::ORACLE_DATA_TOO_SHORT))?,
    );
    let last_update_slot = u64::from_le_bytes(
        data[LAST_UPDATE_SLOT_OFFSET..LAST_UPDATE_SLOT_OFFSET + 8]
            .try_into()
            .map_err(|_| err(error::ORACLE_DATA_TOO_SHORT))?,
    );
    let confidence = u64::from_le_bytes(
        data[CONFIDENCE_OFFSET..CONFIDENCE_OFFSET + 8]
            .try_into()
            .map_err(|_| err(error::ORACLE_DATA_TOO_SHORT))?,
    );

    if price <= 0 {
        return Err(err(error::NON_POSITIVE_PRICE));
    }
    if scale != expected_scale {
        return Err(err(error::ORACLE_SCALE_MISMATCH));
    }
    if current_slot.saturating_sub(last_update_slot) > MAX_PRICE_STALENESS_SLOTS {
        return Err(err(error::STALE_PRICE));
    }

    // Restart handling. A cluster halt stops the slot count but not the wall
    // clock, so after a restart a feed can look fresh in slots while its
    // price is hours old. With leverage a stale price is amplified into a
    // market-wide equity error, so reject any price stamped at or before the
    // restart slot; the pool pauses valuation until the publisher posts
    // again. Zero means the cluster has never restarted.
    let last_restart = u64::from(LastRestartSlot::get()?.last_restart_slot);
    if last_restart != 0 && last_update_slot <= last_restart {
        return Err(err(error::PRICE_PREDATES_RESTART));
    }

    // Confidence band as a fraction of price, in basis points, must stay within
    // the pool's limit. Widen to u128 so the product cannot overflow.
    let confidence_bps = (confidence as u128)
        .checked_mul(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or_else(overflow)?
        .checked_div(price as u128)
        .ok_or_else(overflow)?;
    if confidence_bps > max_confidence_bps as u128 {
        return Err(err(error::ORACLE_CONFIDENCE_TOO_WIDE));
    }

    u64::try_from(price).map_err(|_| overflow())
}

/// New cumulative funding index after advancing to `current_slot`. The heavier
/// side pays: the index rises while longs lead, falls while shorts lead.
pub fn advance_funding(
    cumulative_funding: i128,
    last_funding_slot: u64,
    current_slot: u64,
    funding_rate_per_slot: u64,
    long_size: u128,
    short_size: u128,
) -> Result<i128, ProgramError> {
    let elapsed = current_slot.saturating_sub(last_funding_slot);
    if elapsed == 0 || (long_size == 0 && short_size == 0) {
        return Ok(cumulative_funding);
    }
    let magnitude = (funding_rate_per_slot as i128)
        .checked_mul(elapsed as i128)
        .ok_or_else(overflow)?;
    let delta = if long_size >= short_size {
        magnitude
    } else {
        -magnitude
    };
    cumulative_funding.checked_add(delta).ok_or_else(overflow)
}

pub fn scale_size(size: u64, entry_price: u64) -> Result<u128, ProgramError> {
    (size as u128)
        .checked_mul(SIZE_PRECISION)
        .ok_or_else(overflow)?
        .checked_div(entry_price as u128)
        .ok_or_else(overflow)
}

pub fn position_pnl(
    side: u8,
    size: u64,
    entry_price: u64,
    price: u64,
) -> Result<i128, ProgramError> {
    let size = size as i128;
    let entry = entry_price as i128;
    let price = price as i128;
    let price_change = if side == SIDE_LONG {
        price.checked_sub(entry)
    } else {
        entry.checked_sub(price)
    }
    .ok_or_else(overflow)?;
    size.checked_mul(price_change)
        .ok_or_else(overflow)?
        .checked_div(entry)
        .ok_or_else(overflow)
}

pub fn traders_unrealized_pnl(
    long_size: u128,
    long_size_scaled: u128,
    short_size: u128,
    short_size_scaled: u128,
    price: u64,
) -> Result<i128, ProgramError> {
    let price = price as i128;
    let size_precision = SIZE_PRECISION as i128;

    let long_value = price
        .checked_mul(long_size_scaled as i128)
        .ok_or_else(overflow)?
        .checked_div(size_precision)
        .ok_or_else(overflow)?;
    let long_pnl = long_value
        .checked_sub(long_size as i128)
        .ok_or_else(overflow)?;

    let short_value = price
        .checked_mul(short_size_scaled as i128)
        .ok_or_else(overflow)?
        .checked_div(size_precision)
        .ok_or_else(overflow)?;
    let short_pnl = (short_size as i128)
        .checked_sub(short_value)
        .ok_or_else(overflow)?;

    long_pnl.checked_add(short_pnl).ok_or_else(overflow)
}

pub fn position_funding(
    side: u8,
    size: u64,
    entry_funding: i128,
    pool_funding: i128,
) -> Result<i128, ProgramError> {
    let funding_change = pool_funding
        .checked_sub(entry_funding)
        .ok_or_else(overflow)?;
    let long_owed = (size as i128)
        .checked_mul(funding_change)
        .ok_or_else(overflow)?
        .checked_div(FUNDING_PRECISION)
        .ok_or_else(overflow)?;
    Ok(if side == SIDE_LONG {
        long_owed
    } else {
        -long_owed
    })
}

/// `basis_points` of `amount`, rounded down — used for fees and for the
/// maintenance-margin threshold alike.
pub fn basis_points_of(amount: u64, basis_points: u16) -> Result<u64, ProgramError> {
    let fraction = (amount as u128)
        .checked_mul(basis_points as u128)
        .ok_or_else(overflow)?
        .checked_div(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or_else(overflow)?;
    u64::try_from(fraction).map_err(|_| overflow())
}

/// The preamble every price-sensitive handler runs: read a validated oracle
/// price from the feed, then bring the pool's funding index up to `slot`, so
/// the settlement that follows uses fresh numbers for both. Centralized so no
/// handler can settle a position against a stale funding index.
pub fn refresh_price_and_funding(
    pool: &mut Account<Pool>,
    oracle_feed: &UncheckedAccount,
    slot: u64,
) -> Result<u64, ProgramError> {
    let price = {
        let view = oracle_feed.to_account_view();
        let data = view
            .try_borrow()
            .map_err(|_| err(error::ORACLE_DATA_TOO_SHORT))?;
        read_oracle_price(
            &data,
            pool.oracle_scale.get(),
            slot,
            pool.max_confidence_bps.get(),
        )?
    };

    let new_funding = advance_funding(
        pool.cumulative_funding.get(),
        pool.last_funding_slot.get(),
        slot,
        pool.funding_rate_per_slot.get(),
        pool.long_size.get(),
        pool.short_size.get(),
    )?;
    pool.cumulative_funding.set(new_funding);
    pool.last_funding_slot.set(slot);
    Ok(price)
}
