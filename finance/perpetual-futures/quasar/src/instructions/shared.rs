//! Arithmetic and the oracle decode, ported verbatim from the Anchor sibling.
//! All integer, all `checked_*`, multiply-before-divide, rounding toward the
//! program. Errors are `ProgramError::Custom(code)`; the codes are listed here.

use quasar_lang::{prelude::*, sysvars::Sysvar};

use crate::last_restart::LastRestartSlot;

use crate::constants::{
    BASIS_POINTS_DENOMINATOR, FUNDING_PRECISION, HAIRCUT_PRECISION, MAX_PRICE_STALENESS_SLOTS,
    PRICE_AVERAGE_WINDOW_SECONDS, SIDE_LONG, SIZE_PRECISION,
};
use crate::state::Pool;

pub mod error {
    pub const ZERO_AMOUNT: u32 = 0;
    pub const INITIAL_MARGIN_NOT_MET: u32 = 2;
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
    pub const INITIAL_MARGIN_NOT_ABOVE_MAINTENANCE: u32 = 19;
    pub const INVALID_PRICE_DEVIATION: u32 = 20;
    pub const PRICE_OUTSIDE_BAND: u32 = 21;
    pub const PROFIT_NOT_MATURED: u32 = 22;
    pub const PRICE_FEED_NOT_FROM_ORACLE: u32 = 23;
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
// production it would be a Pyth `PriceUpdateV2` account, which the Pyth
// Receiver program writes only after verifying the update's signatures.
//
// Like the Anchor sibling, this validates freshness, positivity, and the
// confidence band (`confidence / price`), rejecting a price whose band is too
// wide. A production reader may also prefer the feed's EMA over the spot price;
// the mock omits the EMA to stay minimal.
//
// The feed account must be owned by the oracle program the pool recorded at
// creation (`Pool::price_feed_program`, read from the feed's `owner` at that
// moment). `read_feed_price` checks that before a byte is decoded. The layout
// above says nothing about who wrote the bytes, so without the owner check
// any account laid out like a feed would be accepted as a price. The pool's
// creator picks the feed, so the pool trusts the program it recorded and
// refuses a feed account from any other.
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

/// Advance the pool's cumulative funding index to `current_timestamp`, the
/// Clock's `unix_timestamp`.
///
/// The heavier open-interest side pays funding to the pool: while longs are
/// larger the index rises (longs owe), while shorts are larger it falls (shorts
/// owe). No positions means no one to charge, so the index is left untouched and
/// only the timestamp moves forward.
///
/// The timestamp is written by each block's leader. A timestamp at or before
/// the stored one is treated as no time elapsed and leaves the stored stamp
/// where it is, so no second is charged twice or skipped.
pub fn accrue_funding(
    pool: &mut Account<Pool>,
    current_timestamp: i64,
) -> Result<(), ProgramError> {
    let last_funding_timestamp = pool.last_funding_timestamp.get();
    if current_timestamp <= last_funding_timestamp {
        return Ok(());
    }
    let elapsed = current_timestamp
        .checked_sub(last_funding_timestamp)
        .ok_or_else(overflow)?;

    let long_size = pool.long_size.get();
    let short_size = pool.short_size.get();
    if long_size != 0 || short_size != 0 {
        let magnitude = (pool.funding_rate_per_second.get() as i128)
            .checked_mul(elapsed as i128)
            .ok_or_else(overflow)?;
        let delta = if long_size >= short_size {
            magnitude
        } else {
            -magnitude
        };
        let new_funding = pool
            .cumulative_funding
            .get()
            .checked_add(delta)
            .ok_or_else(overflow)?;
        pool.cumulative_funding.set(new_funding);
    }

    pool.last_funding_timestamp.set(current_timestamp);
    Ok(())
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

/// The haircut ratio `h` at `price`, scaled by `HAIRCUT_PRECISION`: the
/// fraction of its profit a winning position is paid when it closes.
///
/// `h = min(1, (liquidity + insurance_fund) / max(liability, closing_profit))`
///
/// The liability is the traders' aggregate unrealized profit from the per-side
/// accumulators, floored at zero, so the caller computes `h` before the closing
/// position leaves them, and every winner closing at that moment is paid the
/// same fraction. While the backing covers it `h` is one. When a move leaves
/// traders owed more than the backing, `h` is the backing divided by the
/// liability, floored, and rises again as losing positions settle into
/// `liquidity`.
///
/// Open losing positions offset winners in the aggregate, so one winner's
/// `closing_profit` can be larger than the liability. Dividing by the larger of
/// the two means a winner who closes while open losers still offset them is
/// paid at most the pool's backing, and is never refused; when the liability is
/// the larger, every other winner's fraction is unchanged.
pub fn haircut_ratio(
    pool: &Account<Pool>,
    price: u64,
    closing_profit: i128,
) -> Result<u128, ProgramError> {
    let traders = traders_unrealized_pnl(
        pool.long_size.get(),
        pool.long_size_scaled.get(),
        pool.short_size.get(),
        pool.short_size_scaled.get(),
        price,
    )?;
    let liability = u128::try_from(traders.max(closing_profit).max(0)).map_err(|_| overflow())?;
    if liability == 0 {
        return Ok(HAIRCUT_PRECISION);
    }
    let backing = (pool.liquidity.get() as u128)
        .checked_add(pool.insurance_fund.get() as u128)
        .ok_or_else(overflow)?;
    if backing >= liability {
        return Ok(HAIRCUT_PRECISION);
    }
    backing
        .checked_mul(HAIRCUT_PRECISION)
        .ok_or_else(overflow)?
        .checked_div(liability)
        .ok_or_else(overflow)
}

/// `profit * haircut / HAIRCUT_PRECISION`, rounded down: the part of a
/// winning position's profit the pool pays. `profit` is positive; a loss is
/// never haircut.
pub fn apply_haircut(profit: i128, haircut: u128) -> Result<i128, ProgramError> {
    let profit = u128::try_from(profit).map_err(|_| overflow())?;
    let paid = profit
        .checked_mul(haircut)
        .ok_or_else(overflow)?
        .checked_div(HAIRCUT_PRECISION)
        .ok_or_else(overflow)?;
    i128::try_from(paid).map_err(|_| overflow())
}

/// Split an open or close fee into `(insurance_cut, program_cut)`. The
/// insurance cut is `insurance_fee_bps` of the fee, rounded down, and the
/// program keeps the rest, so the two always add up to the whole fee.
pub fn split_fee(fee: u64, insurance_fee_bps: u16) -> Result<(u64, u64), ProgramError> {
    let insurance_cut = basis_points_of(fee, insurance_fee_bps)?;
    let program_cut = fee.checked_sub(insurance_cut).ok_or_else(overflow)?;
    Ok((insurance_cut, program_cut))
}

/// Credit an open or close fee: `insurance_fee_bps` of it to the insurance
/// fund and the rest to program fees.
pub fn credit_fee(pool: &mut Account<Pool>, fee: u64) -> Result<(), ProgramError> {
    let (insurance_cut, program_cut) = split_fee(fee, pool.insurance_fee_bps.get())?;
    let insurance_fund = pool
        .insurance_fund
        .get()
        .checked_add(insurance_cut)
        .ok_or_else(overflow)?;
    pool.insurance_fund.set(insurance_fund);
    let program_fees = pool
        .program_fees
        .get()
        .checked_add(program_cut)
        .ok_or_else(overflow)?;
    pool.program_fees.set(program_fees);
    Ok(())
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

/// Fold the elapsed interval into the pool's `average_price`, then record
/// `price` as the latest observation.
///
/// The interval since the last fold is credited to the price observed at
/// that fold, `last_oracle_price`, on the assumption that it held throughout:
///
/// `average += (last_oracle_price - average) * min(elapsed, PRICE_AVERAGE_WINDOW_SECONDS) / PRICE_AVERAGE_WINDOW_SECONDS`
///
/// The price read now only starts counting from now, so it moves the average
/// only if it is still the oracle's price at a later read, weighted by the
/// seconds between the two reads; a read of a different price in between
/// replaces it. A pool left idle for a window or more therefore cannot have
/// its average set by one read. As with funding, a timestamp at or before the
/// stored one is treated as no time elapsed: the average and the stored stamp
/// stay where they are, and only `last_oracle_price` is updated.
pub fn fold_price_into_average(
    pool: &mut Account<Pool>,
    price: u64,
    current_timestamp: i64,
) -> Result<(), ProgramError> {
    let average_price_timestamp = pool.average_price_timestamp.get();
    if current_timestamp <= average_price_timestamp {
        pool.last_oracle_price.set(price);
        return Ok(());
    }
    let elapsed = current_timestamp
        .checked_sub(average_price_timestamp)
        .ok_or_else(overflow)?;
    let weight = elapsed.min(PRICE_AVERAGE_WINDOW_SECONDS);

    let average = pool.average_price.get() as i128;
    // Multiply before dividing; the gap is signed, so the average moves down
    // as readily as up.
    let movement = (pool.last_oracle_price.get() as i128)
        .checked_sub(average)
        .ok_or_else(overflow)?
        .checked_mul(weight as i128)
        .ok_or_else(overflow)?
        .checked_div(PRICE_AVERAGE_WINDOW_SECONDS as i128)
        .ok_or_else(overflow)?;
    let new_average = average.checked_add(movement).ok_or_else(overflow)?;
    pool.average_price
        .set(u64::try_from(new_average).map_err(|_| overflow())?);
    pool.last_oracle_price.set(price);
    pool.average_price_timestamp.set(current_timestamp);
    Ok(())
}

/// Refuse an oracle `price` more than `max_price_deviation_bps` away from the
/// pool's stored `average_price`:
/// `|price - average_price| * 10_000 <= average_price * max_price_deviation_bps`.
pub fn require_price_within_band(pool: &Account<Pool>, price: u64) -> Result<(), ProgramError> {
    let average_price = pool.average_price.get();
    let deviation_scaled = (price.abs_diff(average_price) as u128)
        .checked_mul(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or_else(overflow)?;
    let band_scaled = (average_price as u128)
        .checked_mul(pool.max_price_deviation_bps.get() as u128)
        .ok_or_else(overflow)?;
    if deviation_scaled > band_scaled {
        return Err(err(error::PRICE_OUTSIDE_BAND));
    }
    Ok(())
}

/// Read and validate the oracle price from the feed account, checked for
/// freshness against `slot`. Rejects a feed account owned by any program other
/// than `oracle_program` before decoding a byte.
pub fn read_feed_price(
    oracle_feed: &UncheckedAccount,
    oracle_program: &Address,
    expected_scale: u32,
    slot: u64,
    max_confidence_bps: u16,
) -> Result<u64, ProgramError> {
    let view = oracle_feed.to_account_view();
    if view.owner() != oracle_program {
        return Err(err(error::PRICE_FEED_NOT_FROM_ORACLE));
    }
    let data = view
        .try_borrow()
        .map_err(|_| err(error::ORACLE_DATA_TOO_SHORT))?;
    read_oracle_price(&data, expected_scale, slot, max_confidence_bps)
}

/// The preamble `liquidate_position` and `update_price_average` run: read a
/// validated oracle price, checked for freshness against `slot`, bring the
/// pool's funding index up to `unix_timestamp`, and fold the interval since the
/// previous read into the pool's average (see `fold_price_into_average`), so
/// the settlement that follows uses fresh numbers.
/// Centralized so no handler can settle a position against a stale funding
/// index.
///
/// No band check: liquidation has to keep working through a genuine price
/// move, because that is when positions go underwater, and
/// `update_price_average` is how the average catches up with one.
pub fn refresh_price_and_funding(
    pool: &mut Account<Pool>,
    oracle_feed: &UncheckedAccount,
    slot: u64,
    unix_timestamp: i64,
) -> Result<u64, ProgramError> {
    let price = read_pool_oracle_price(pool, oracle_feed, slot)?;
    accrue_funding(pool, unix_timestamp)?;
    fold_price_into_average(pool, price, unix_timestamp)?;
    Ok(price)
}

/// The preamble for every handler that opens or closes a position or moves
/// liquidity: the same as `refresh_price_and_funding`, but first refuses a
/// price outside the band around the stored average, before anything is
/// folded in or the price is recorded. A single oracle print far from the
/// average therefore cannot open, close, deposit, or withdraw at that price.
pub fn refresh_price_and_funding_within_band(
    pool: &mut Account<Pool>,
    oracle_feed: &UncheckedAccount,
    slot: u64,
    unix_timestamp: i64,
) -> Result<u64, ProgramError> {
    let price = read_pool_oracle_price(pool, oracle_feed, slot)?;
    require_price_within_band(pool, price)?;
    accrue_funding(pool, unix_timestamp)?;
    fold_price_into_average(pool, price, unix_timestamp)?;
    Ok(price)
}

fn read_pool_oracle_price(
    pool: &Account<Pool>,
    oracle_feed: &UncheckedAccount,
    slot: u64,
) -> Result<u64, ProgramError> {
    read_feed_price(
        oracle_feed,
        &pool.price_feed_program,
        pool.oracle_scale.get(),
        slot,
        pool.max_confidence_bps.get(),
    )
}
