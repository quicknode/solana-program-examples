use anchor_lang::prelude::*;

use crate::constants::{BASIS_POINTS_DENOMINATOR, FUNDING_PRECISION, SIZE_PRECISION};
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

/// Advance the pool's cumulative funding index to `current_slot`.
///
/// The heavier open-interest side pays funding to the pool: while longs are
/// larger the index rises (longs owe), while shorts are larger it falls (shorts
/// owe). No positions means no one to charge, so the index is left untouched and
/// only the timestamp moves forward.
pub fn accrue_funding(pool: &mut Pool, current_slot: u64) -> Result<()> {
    let elapsed = current_slot.saturating_sub(pool.last_funding_slot);
    if elapsed == 0 {
        return Ok(());
    }

    if pool.long_size != 0 || pool.short_size != 0 {
        let magnitude = (pool.funding_rate_per_slot as i128)
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

    pool.last_funding_slot = current_slot;
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
/// Profit is marked uncapped here: a position already past the reserved-profit
/// cap is carried at more than the pool will actually pay out, so
/// assets-under-management reads slightly low until that position closes.
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

/// `basis_points` of `amount`, rounded down — used for fees and for the
/// maintenance-margin threshold alike. Widened to `u128` so a large amount
/// cannot overflow the intermediate product.
pub fn basis_points_of(amount: u64, basis_points: u16) -> Result<u64> {
    (amount as u128)
        .checked_mul(basis_points as u128)
        .ok_or(PerpError::MathOverflow)?
        .checked_div(BASIS_POINTS_DENOMINATOR as u128)
        .ok_or(PerpError::MathOverflow)?
        .try_into()
        .map_err(|_| PerpError::MathOverflow.into())
}

/// The preamble every price-sensitive handler runs: read a validated oracle
/// price, then bring the pool's funding index up to the current slot, so the
/// settlement that follows uses fresh numbers for both. Centralized so no
/// handler can settle a position against a stale funding index.
pub fn refresh_price_and_funding(pool: &mut Pool, oracle_feed: &AccountInfo) -> Result<u64> {
    let price = crate::state::oracle::read_oracle_price(
        oracle_feed,
        pool.oracle_scale,
        pool.max_confidence_bps,
    )?;
    accrue_funding(pool, Clock::get()?.slot)?;
    Ok(price)
}
