//! The quote math and the oracle decode, ported verbatim from the Anchor
//! sibling (`prop_amm::quote_math` and `prop_amm::state::oracle`). All
//! integer, all `checked_*`, multiply-before-divide, every rounding direction
//! favoring the market. Errors are `ProgramError::Custom(code)`; the codes are
//! listed here.

use quasar_lang::{prelude::*, sysvars::Sysvar};

use crate::constants::{BASIS_POINTS_DENOMINATOR, MAX_PRICE_STALENESS_SLOTS};
use crate::last_restart::LastRestartSlot;

pub mod error {
    pub const ZERO_AMOUNT: u32 = 0;
    pub const INVALID_PARAMETER: u32 = 1;
    pub const STALE_PRICE: u32 = 2;
    pub const NON_POSITIVE_PRICE: u32 = 3;
    pub const ORACLE_SCALE_MISMATCH: u32 = 4;
    pub const ORACLE_DATA_TOO_SHORT: u32 = 5;
    pub const ORACLE_CONFIDENCE_TOO_WIDE: u32 = 6;
    pub const SLIPPAGE_EXCEEDED: u32 = 7;
    pub const INSUFFICIENT_INVENTORY: u32 = 8;
    pub const MARKET_PAUSED: u32 = 9;
    pub const AMOUNT_ROUNDS_TO_ZERO: u32 = 10;
    pub const INVARIANT_VIOLATED: u32 = 11;
    pub const INVALID_DIRECTION: u32 = 12;
    pub const PRICE_PREDATES_RESTART: u32 = 13;
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
// production it would be a real Switchboard On-Demand feed parsed with
// signature verification. Like the Anchor sibling, this validates freshness,
// positivity, scale, and the confidence band; the feed account's owning
// program is NOT checked — the operator picks the oracle, and a bad choice
// loses the operator's money, not the traders'.
const PRICE_OFFSET: usize = 0;
const SCALE_OFFSET: usize = PRICE_OFFSET + 16;
const LAST_UPDATE_SLOT_OFFSET: usize = SCALE_OFFSET + 4;
const CONFIDENCE_OFFSET: usize = LAST_UPDATE_SLOT_OFFSET + 8;
const FEED_MINIMUM_LENGTH: usize = CONFIDENCE_OFFSET + 8;

/// Read and validate the oracle price from raw feed bytes. Returns the price
/// as a `u64` in `expected_scale` fixed point.
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
    // price is hours old. For a market maker that is a free option for
    // whoever trades first, so reject any price stamped at or before the
    // restart slot; the market refuses to quote until the publisher posts
    // again. Zero means the cluster has never restarted.
    let last_restart = u64::from(LastRestartSlot::get()?.last_restart_slot);
    if last_restart != 0 && last_update_slot <= last_restart {
        return Err(err(error::PRICE_PREDATES_RESTART));
    }

    // Confidence band as a fraction of price, in basis points, must stay
    // within the market's limit. Widen to u128 so the product cannot overflow.
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

const BASIS_POINTS: u128 = BASIS_POINTS_DENOMINATOR as u128;

/// The price a buyer of the base token pays: oracle plus the spread, rounded
/// UP so the rounding penny goes to the market, not the buyer.
pub fn ask_price(oracle_price: u64, spread_bps: u16) -> Result<u128, ProgramError> {
    let numerator = (oracle_price as u128)
        .checked_mul(
            BASIS_POINTS
                .checked_add(spread_bps as u128)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    Ok(numerator.div_ceil(BASIS_POINTS))
}

/// The price a seller of the base token receives: oracle minus the spread,
/// rounded DOWN — the same coin, the other face.
pub fn bid_price(oracle_price: u64, spread_bps: u16) -> Result<u128, ProgramError> {
    let numerator = (oracle_price as u128)
        .checked_mul(
            BASIS_POINTS
                .checked_sub(spread_bps as u128)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    Ok(numerator / BASIS_POINTS)
}

/// Base tokens out for `quote_in` at the ask, floored. Multiply everything
/// before the one division so the floor happens exactly once, at the end.
pub fn base_out_for_quote_in(
    quote_in: u64,
    ask: u128,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Result<u64, ProgramError> {
    let numerator = (quote_in as u128)
        .checked_mul(10u128.checked_pow(oracle_scale).ok_or_else(overflow)?)
        .ok_or_else(overflow)?
        .checked_mul(
            10u128
                .checked_pow(base_decimals as u32)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    let denominator = ask
        .checked_mul(
            10u128
                .checked_pow(quote_decimals as u32)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    if denominator == 0 {
        return Err(overflow());
    }
    u64::try_from(numerator / denominator).map_err(|_| overflow())
}

/// Quote tokens out for `base_in` at the bid, floored.
pub fn quote_out_for_base_in(
    base_in: u64,
    bid: u128,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Result<u64, ProgramError> {
    let numerator = (base_in as u128)
        .checked_mul(bid)
        .ok_or_else(overflow)?
        .checked_mul(
            10u128
                .checked_pow(quote_decimals as u32)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    let denominator = 10u128
        .checked_pow(oracle_scale)
        .ok_or_else(overflow)?
        .checked_mul(
            10u128
                .checked_pow(base_decimals as u32)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    if denominator == 0 {
        return Err(overflow());
    }
    u64::try_from(numerator / denominator).map_err(|_| overflow())
}

/// Both sides of a swap valued in the same fixed-point unit, cross-multiplied
/// so no division — and therefore no second rounding — is involved.
fn values_at_oracle(
    base_amount: u64,
    quote_amount: u64,
    oracle_price: u64,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Result<(u128, u128), ProgramError> {
    let base_value = (base_amount as u128)
        .checked_mul(oracle_price as u128)
        .ok_or_else(overflow)?
        .checked_mul(
            10u128
                .checked_pow(quote_decimals as u32)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    let quote_value = (quote_amount as u128)
        .checked_mul(10u128.checked_pow(oracle_scale).ok_or_else(overflow)?)
        .ok_or_else(overflow)?
        .checked_mul(
            10u128
                .checked_pow(base_decimals as u32)
                .ok_or_else(overflow)?,
        )
        .ok_or_else(overflow)?;
    Ok((base_value, quote_value))
}

/// Post-math invariant for a buy: the base tokens handed out must be worth no
/// more, at the raw oracle price (no spread), than the quote tokens taken in.
pub fn buy_respects_oracle_value(
    quote_in: u64,
    base_out: u64,
    oracle_price: u64,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Result<bool, ProgramError> {
    let (base_value, quote_value) = values_at_oracle(
        base_out,
        quote_in,
        oracle_price,
        oracle_scale,
        base_decimals,
        quote_decimals,
    )?;
    Ok(base_value <= quote_value)
}

/// The same invariant for a sell: the quote tokens handed out must be worth no
/// more, at the raw oracle price, than the base tokens taken in.
pub fn sell_respects_oracle_value(
    base_in: u64,
    quote_out: u64,
    oracle_price: u64,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Result<bool, ProgramError> {
    let (base_value, quote_value) = values_at_oracle(
        base_in,
        quote_out,
        oracle_price,
        oracle_scale,
        base_decimals,
        quote_decimals,
    )?;
    Ok(quote_value <= base_value)
}
