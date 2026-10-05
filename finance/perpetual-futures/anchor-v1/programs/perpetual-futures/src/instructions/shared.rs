use anchor_lang::prelude::*;

use crate::constants::{
    BASIS_POINTS_DENOMINATOR, FUNDING_PRECISION, HAIRCUT_PRECISION, PRICE_AVERAGE_WINDOW_SECONDS,
    SIZE_PRECISION,
};
use crate::errors::PerpError;
use crate::state::{Pool, Position, Side};

/// Result of removing a position from the pool's aggregates. All figures are in
/// collateral base units; `equity` is what the trader's position is worth
/// before any close or liquidation fee.
pub struct Settlement {
    pub profit_and_loss: i128,
    pub funding: i128,
    pub equity: i128,
}

/// Settle a position against the current `price`: compute its profit/loss,
/// funding owed, and equity, then remove its open interest and collateral from
/// the pool's aggregates. Does not touch `pool.liquidity` or move tokens — the
/// caller applies the side that differs between closing and liquidating.
pub fn settle_position(pool: &mut Pool, position: &Position, price: u64) -> Result<Settlement> {
    let profit_and_loss = position_pnl(position.side, position.size, position.entry_price, price)?;
    let funding = position_funding(
        position.side,
        position.size,
        position.entry_funding,
        pool.cumulative_funding,
    )?;

    let equity = (position.collateral as i128)
        .checked_add(profit_and_loss)
        .ok_or(PerpError::MathOverflow)?
        .checked_sub(funding)
        .ok_or(PerpError::MathOverflow)?;

    match position.side {
        Side::Long => {
            pool.long_size = pool
                .long_size
                .checked_sub(position.size as u128)
                .ok_or(PerpError::MathOverflow)?;
            pool.long_size_scaled = pool
                .long_size_scaled
                .checked_sub(position.size_scaled)
                .ok_or(PerpError::MathOverflow)?;
        }
        Side::Short => {
            pool.short_size = pool
                .short_size
                .checked_sub(position.size as u128)
                .ok_or(PerpError::MathOverflow)?;
            pool.short_size_scaled = pool
                .short_size_scaled
                .checked_sub(position.size_scaled)
                .ok_or(PerpError::MathOverflow)?;
        }
    }

    pool.total_collateral = pool
        .total_collateral
        .checked_sub(position.collateral)
        .ok_or(PerpError::MathOverflow)?;

    Ok(Settlement {
        profit_and_loss,
        funding,
        equity,
    })
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
pub fn accrue_funding(pool: &mut Pool, current_timestamp: i64) -> Result<()> {
    if current_timestamp <= pool.last_funding_timestamp {
        return Ok(());
    }
    let elapsed = current_timestamp
        .checked_sub(pool.last_funding_timestamp)
        .ok_or(PerpError::MathOverflow)?;

    if pool.long_size != 0 || pool.short_size != 0 {
        let magnitude = (pool.funding_rate_per_second as i128)
            .checked_mul(elapsed as i128)
            .ok_or(PerpError::MathOverflow)?;
        let delta = if pool.long_size >= pool.short_size {
            magnitude
        } else {
            -magnitude
        };
        pool.cumulative_funding = pool
            .cumulative_funding
            .checked_add(delta)
            .ok_or(PerpError::MathOverflow)?;
    }

    pool.last_funding_timestamp = current_timestamp;
    Ok(())
}

/// A position's contribution to the pool's `*_size_scaled` accumulator.
/// `entry_price` is always positive (oracle prices are validated `> 0`).
pub fn scale_size(size: u64, entry_price: u64) -> Result<u128> {
    (size as u128)
        .checked_mul(SIZE_PRECISION)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(entry_price as u128)
        .ok_or(PerpError::MathOverflow.into())
}

/// Signed profit/loss of one position at `price`, in collateral base units.
/// Longs profit when price rises, shorts when it falls.
pub fn position_pnl(side: Side, size: u64, entry_price: u64, price: u64) -> Result<i128> {
    let size = size as i128;
    let entry = entry_price as i128;
    let price = price as i128;

    let price_change = match side {
        Side::Long => price.checked_sub(entry),
        Side::Short => entry.checked_sub(price),
    }
    .ok_or(PerpError::MathOverflow)?;

    // Multiply before dividing to keep precision; `entry > 0` is guaranteed.
    size.checked_mul(price_change)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(entry)
        .ok_or(PerpError::MathOverflow.into())
}

/// Aggregate unrealized profit/loss of every open trader at `price`, derived
/// from the pool's running accumulators rather than iterating positions.
/// Positive means traders are collectively up (and the pool is down).
///
/// Profit is marked in full, before any haircut: while `haircut_ratio` is
/// below one, winners will be paid less than this, so assets-under-management
/// reads low by the withheld part until they close.
pub fn traders_unrealized_pnl(pool: &Pool, price: u64) -> Result<i128> {
    let price = price as i128;
    let size_precision = SIZE_PRECISION as i128;

    let long_value = price
        .checked_mul(pool.long_size_scaled as i128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(size_precision)
        .ok_or(PerpError::MathOverflow)?;
    let long_pnl = long_value
        .checked_sub(pool.long_size as i128)
        .ok_or(PerpError::MathOverflow)?;

    let short_value = price
        .checked_mul(pool.short_size_scaled as i128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(size_precision)
        .ok_or(PerpError::MathOverflow)?;
    let short_pnl = (pool.short_size as i128)
        .checked_sub(short_value)
        .ok_or(PerpError::MathOverflow)?;

    long_pnl
        .checked_add(short_pnl)
        .ok_or(PerpError::MathOverflow.into())
}

/// Liquidity-provider assets-under-management at `price`: pool liquidity minus
/// what traders are collectively owed. This is what liquidity-provider shares
/// are priced against, so it marks open positions to the current price and an
/// exiting provider cannot dodge an in-progress trader profit.
pub fn liquidity_provider_aum(pool: &Pool, price: u64) -> Result<i128> {
    let traders = traders_unrealized_pnl(pool, price)?;
    (pool.liquidity as i128)
        .checked_sub(traders)
        .ok_or(PerpError::MathOverflow.into())
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
pub fn haircut_ratio(pool: &Pool, price: u64, closing_profit: i128) -> Result<u128> {
    let liability: u128 = traders_unrealized_pnl(pool, price)?
        .max(closing_profit)
        .max(0)
        .try_into()
        .map_err(|_| PerpError::MathOverflow)?;
    if liability == 0 {
        return Ok(HAIRCUT_PRECISION);
    }
    let backing = (pool.liquidity as u128)
        .checked_add(pool.insurance_fund as u128)
        .ok_or(PerpError::MathOverflow)?;
    if backing >= liability {
        return Ok(HAIRCUT_PRECISION);
    }
    backing
        .checked_mul(HAIRCUT_PRECISION)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(liability)
        .ok_or(PerpError::MathOverflow.into())
}

/// `profit * haircut / HAIRCUT_PRECISION`, rounded down: the part of a
/// winning position's profit the pool pays. `profit` is positive; a loss is
/// never haircut.
pub fn apply_haircut(profit: i128, haircut: u128) -> Result<i128> {
    let profit: u128 = profit.try_into().map_err(|_| PerpError::MathOverflow)?;
    profit
        .checked_mul(haircut)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(HAIRCUT_PRECISION)
        .ok_or(PerpError::MathOverflow)?
        .try_into()
        .map_err(|_| PerpError::MathOverflow.into())
}

/// Split an open or close fee into `(insurance_cut, program_cut)`. The
/// insurance cut is `insurance_fee_bps` of the fee, rounded down, and the
/// program keeps the rest, so the two always add up to the whole fee.
pub fn split_fee(fee: u64, insurance_fee_bps: u16) -> Result<(u64, u64)> {
    let insurance_cut = basis_points_of_rounded_down(fee, insurance_fee_bps)?;
    let program_cut = fee
        .checked_sub(insurance_cut)
        .ok_or(PerpError::MathOverflow)?;
    Ok((insurance_cut, program_cut))
}

/// Credit an open or close fee: `insurance_fee_bps` of it to the insurance
/// fund and the rest to program fees.
pub fn credit_fee(pool: &mut Pool, fee: u64) -> Result<()> {
    let (insurance_cut, program_cut) = split_fee(fee, pool.insurance_fee_bps)?;
    pool.insurance_fund = pool
        .insurance_fund
        .checked_add(insurance_cut)
        .ok_or(PerpError::MathOverflow)?;
    pool.program_fees = pool
        .program_fees
        .checked_add(program_cut)
        .ok_or(PerpError::MathOverflow)?;
    Ok(())
}

/// Funding a position owes since it opened, in collateral base units. Positive
/// means the trader pays the pool; negative means the pool pays the trader.
pub fn position_funding(
    side: Side,
    size: u64,
    entry_funding: i128,
    pool_funding: i128,
) -> Result<i128> {
    let funding_change = pool_funding
        .checked_sub(entry_funding)
        .ok_or(PerpError::MathOverflow)?;
    let long_owed = (size as i128)
        .checked_mul(funding_change)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(FUNDING_PRECISION)
        .ok_or(PerpError::MathOverflow)?;

    Ok(match side {
        Side::Long => long_owed,
        Side::Short => -long_owed,
    })
}

/// `basis_points` of `amount`, rounded up: the open, close and liquidation
/// fees and the maintenance requirement a position is liquidated at, each of
/// which rounds in the pool's favour, so a fee is never a minor unit short and
/// a position is never a minor unit too healthy to liquidate. Widened to
/// `u128` so a large amount cannot overflow the intermediate product.
pub fn basis_points_of(amount: u64, basis_points: u16) -> Result<u64> {
    let denominator = BASIS_POINTS_DENOMINATOR as u128;
    (amount as u128)
        .checked_mul(basis_points as u128)
        .ok_or(PerpError::MathOverflow)?
        .checked_add(denominator - 1)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(denominator)
        .ok_or(PerpError::MathOverflow)?
        .try_into()
        .map_err(|_| PerpError::MathOverflow.into())
}

/// `basis_points` of `amount`, rounded down: the insurance fund's cut of a fee
/// the pool has already collected, where the rounding moves nothing between
/// the pool and a trader. `split_fee` gives the program the remainder.
pub fn basis_points_of_rounded_down(amount: u64, basis_points: u16) -> Result<u64> {
    (amount as u128)
        .checked_mul(basis_points as u128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(PerpError::MathOverflow)?
        .try_into()
        .map_err(|_| PerpError::MathOverflow.into())
}

/// Fold the elapsed interval into the pool's `average_price`, then record
/// `price` as the latest observation.
///
/// The interval since the last fold is credited to the price observed at
/// that fold, `last_oracle_price`, on the assumption that it held throughout:
///
/// `average += (last_oracle_price - average) * min(elapsed, PRICE_AVERAGE_WINDOW_SECONDS) / PRICE_AVERAGE_WINDOW_SECONDS`
///
/// The price read now only starts counting from now: at the next read it is
/// credited for the seconds between the two reads, whatever price that read
/// sees, and then replaced as the latest observation. So a pool left idle for
/// a window or more cannot have its average set by one read. As with funding,
/// a timestamp at or before the stored one is treated as no time elapsed: the
/// average and the stored stamp stay where they are, and only
/// `last_oracle_price` is updated.
pub fn fold_price_into_average(pool: &mut Pool, price: u64, current_timestamp: i64) -> Result<()> {
    if current_timestamp <= pool.average_price_timestamp {
        pool.last_oracle_price = price;
        return Ok(());
    }
    let elapsed = current_timestamp
        .checked_sub(pool.average_price_timestamp)
        .ok_or(PerpError::MathOverflow)?;
    let weight = elapsed.min(PRICE_AVERAGE_WINDOW_SECONDS);

    let average = pool.average_price as i128;
    // Multiply before dividing; the gap is signed, so the average moves down
    // as readily as up.
    let movement = (pool.last_oracle_price as i128)
        .checked_sub(average)
        .ok_or(PerpError::MathOverflow)?
        .checked_mul(weight as i128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(PRICE_AVERAGE_WINDOW_SECONDS as i128)
        .ok_or(PerpError::MathOverflow)?;
    pool.average_price = average
        .checked_add(movement)
        .ok_or(PerpError::MathOverflow)?
        .try_into()
        .map_err(|_| PerpError::MathOverflow)?;
    pool.last_oracle_price = price;
    pool.average_price_timestamp = current_timestamp;
    Ok(())
}

/// Refuse an oracle `price` more than `max_price_deviation_bps` away from the
/// pool's stored `average_price`:
/// `|price - average_price| * 10_000 <= average_price * max_price_deviation_bps`.
pub fn require_price_within_band(pool: &Pool, price: u64) -> Result<()> {
    let deviation_scaled = (price.abs_diff(pool.average_price) as u128)
        .checked_mul(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(PerpError::MathOverflow)?;
    let band_scaled = (pool.average_price as u128)
        .checked_mul(pool.max_price_deviation_bps as u128)
        .ok_or(PerpError::MathOverflow)?;
    require!(deviation_scaled <= band_scaled, PerpError::PriceOutsideBand);
    Ok(())
}

/// The preamble `liquidate_position` and `update_price_average` run: read a
/// validated oracle price, bring the pool's funding index up to the current
/// time, and fold the interval since the previous read into the pool's average
/// (see `fold_price_into_average`), so the settlement that follows uses fresh
/// numbers. Centralized so no handler can settle a position
/// against a stale funding index.
///
/// No band check: liquidation has to keep working through a genuine price
/// move, because that is when positions go underwater, and
/// `update_price_average` is how the average catches up with one.
pub fn refresh_price_and_funding(pool: &mut Pool, oracle_feed: &AccountInfo) -> Result<u64> {
    let price = read_pool_oracle_price(pool, oracle_feed)?;
    apply_price_and_funding(pool, price)?;
    Ok(price)
}

/// The preamble for every handler that opens or closes a position or moves
/// liquidity: the same as `refresh_price_and_funding`, but first refuses a
/// price outside the band around the stored average, before anything is
/// folded in or the price is recorded. A single oracle print far from the
/// average therefore cannot open, close, deposit, or withdraw at that price.
pub fn refresh_price_and_funding_within_band(
    pool: &mut Pool,
    oracle_feed: &AccountInfo,
) -> Result<u64> {
    let price = read_pool_oracle_price(pool, oracle_feed)?;
    require_price_within_band(pool, price)?;
    apply_price_and_funding(pool, price)?;
    Ok(price)
}

fn read_pool_oracle_price(pool: &Pool, oracle_feed: &AccountInfo) -> Result<u64> {
    crate::state::oracle::read_oracle_price(
        oracle_feed,
        &pool.price_feed_program,
        pool.oracle_scale,
        pool.max_confidence_bps,
    )
}

fn apply_price_and_funding(pool: &mut Pool, price: u64) -> Result<()> {
    let current_timestamp = Clock::get()?.unix_timestamp;
    accrue_funding(pool, current_timestamp)?;
    fold_price_into_average(pool, price, current_timestamp)
}
