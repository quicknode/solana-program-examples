//! Integer-only money math (no floats, no fixed-point crates), shared by the
//! handlers. Ratios are scaled by `FIXED_POINT_SCALE`; conversions round in the
//! protocol's favour.

use quasar_lang::prelude::*;

use crate::{
    constants::{BPS_DENOMINATOR, FIXED_POINT_SCALE, FIXED_POINT_SCALE_DECIMALS, SLOTS_PER_YEAR},
    error::LendingError,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    Down,
    Up,
}

pub fn ten_pow(exponent: u32) -> Result<u128, ProgramError> {
    10u128.checked_pow(exponent).ok_or(LendingError::MathOverflow.into())
}

pub fn mul_div_floor(a: u128, b: u128, denominator: u128) -> Result<u128, ProgramError> {
    require!(denominator > 0, LendingError::MathOverflow);
    let product = a.checked_mul(b).ok_or(LendingError::MathOverflow)?;
    Ok(product.checked_div(denominator).ok_or(LendingError::MathOverflow)?)
}

pub fn mul_div_ceil(a: u128, b: u128, denominator: u128) -> Result<u128, ProgramError> {
    require!(denominator > 0, LendingError::MathOverflow);
    let product = a.checked_mul(b).ok_or(LendingError::MathOverflow)?;
    let rounding = denominator.checked_sub(1).ok_or(LendingError::MathOverflow)?;
    Ok(product
        .checked_add(rounding)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(denominator)
        .ok_or(LendingError::MathOverflow)?)
}

fn mul_div(a: u128, b: u128, denominator: u128, rounding: Rounding) -> Result<u128, ProgramError> {
    match rounding {
        Rounding::Down => mul_div_floor(a, b, denominator),
        Rounding::Up => mul_div_ceil(a, b, denominator),
    }
}

/// `price_scaled = real_price * FIXED_POINT_SCALE`, where
/// `real_price = mantissa * 10^exponent`. The exponent and the fixed-point scale
/// are folded into one power of ten to stay overflow-safe for high prices.
pub fn price_mantissa_to_scaled(mantissa: u128, exponent: i32) -> Result<u128, ProgramError> {
    let net = exponent
        .checked_add(FIXED_POINT_SCALE_DECIMALS)
        .ok_or(LendingError::MathOverflow)?;
    if net >= 0 {
        Ok(mantissa
            .checked_mul(ten_pow(net as u32)?)
            .ok_or(LendingError::MathOverflow)?)
    } else {
        Ok(mantissa
            .checked_div(ten_pow((-net) as u32)?)
            .ok_or(LendingError::MathOverflow)?)
    }
}

/// Quote-currency value (FIXED_POINT_SCALE-scaled) of `amount` base units of a
/// token with `decimals`, given `price_scaled`.
pub fn market_value(
    amount: u64,
    decimals: u8,
    price_scaled: u128,
    rounding: Rounding,
) -> Result<u128, ProgramError> {
    mul_div(amount as u128, price_scaled, ten_pow(decimals as u32)?, rounding)
}

/// Inverse of [`market_value`]: base units of a token worth `value_scaled`.
pub fn value_to_amount(
    value_scaled: u128,
    decimals: u8,
    price_scaled: u128,
    rounding: Rounding,
) -> Result<u64, ProgramError> {
    let amount = mul_div(value_scaled, ten_pow(decimals as u32)?, price_scaled, rounding)?;
    u64::try_from(amount).map_err(|_| LendingError::MathOverflow.into())
}

// --- reserve interest / share helpers (free functions over reserve fields) ---

/// Live total debt owed to the pool, rounded up (protocol-favourable).
pub fn current_debt(borrowed_scaled: u128, index: u128) -> Result<u64, ProgramError> {
    let debt = mul_div_ceil(borrowed_scaled, index, FIXED_POINT_SCALE)?;
    u64::try_from(debt).map_err(|_| LendingError::MathOverflow.into())
}

/// Available liquidity plus live debt, before the protocol fee is removed. Used
/// for the utilization ratio (about how much of the pool is lent out).
pub fn total_liquidity(
    available: u64,
    borrowed_scaled: u128,
    index: u128,
) -> Result<u128, ProgramError> {
    (available as u128)
        .checked_add(current_debt(borrowed_scaled, index)? as u128)
        .ok_or(LendingError::MathOverflow.into())
}

/// What the share token is a claim on: gross liquidity minus the protocol fees
/// owed to the owner, which belong to no supplier.
pub fn net_total_liquidity(
    available: u64,
    borrowed_scaled: u128,
    index: u128,
    protocol_fees: u64,
) -> Result<u128, ProgramError> {
    total_liquidity(available, borrowed_scaled, index)?
        .checked_sub(protocol_fees as u128)
        .ok_or(LendingError::MathOverflow.into())
}

/// Borrowed fraction of the pool in basis points (0..=10_000).
pub fn utilization_bps(
    available: u64,
    borrowed_scaled: u128,
    index: u128,
) -> Result<u128, ProgramError> {
    let total = total_liquidity(available, borrowed_scaled, index)?;
    if total == 0 {
        return Ok(0);
    }
    mul_div_floor(current_debt(borrowed_scaled, index)? as u128, BPS_DENOMINATOR, total)
}

/// Per-slot borrow rate (FIXED_POINT_SCALE-scaled) from the kinked curve.
#[allow(clippy::too_many_arguments)]
pub fn borrow_rate_per_slot(
    utilization: u128,
    optimal_utilization_bps: u16,
    min_rate_bps: u16,
    optimal_rate_bps: u16,
    max_rate_bps: u16,
) -> Result<u128, ProgramError> {
    let optimal_utilization = optimal_utilization_bps as u128;
    let apr_bps = if utilization <= optimal_utilization {
        let range = (optimal_rate_bps as u128)
            .checked_sub(min_rate_bps as u128)
            .ok_or(LendingError::MathOverflow)?;
        (min_rate_bps as u128)
            .checked_add(mul_div_floor(range, utilization, optimal_utilization.max(1))?)
            .ok_or(LendingError::MathOverflow)?
    } else {
        let range = (max_rate_bps as u128)
            .checked_sub(optimal_rate_bps as u128)
            .ok_or(LendingError::MathOverflow)?;
        let above = utilization
            .checked_sub(optimal_utilization)
            .ok_or(LendingError::MathOverflow)?;
        let span = BPS_DENOMINATOR
            .checked_sub(optimal_utilization)
            .ok_or(LendingError::MathOverflow)?;
        (optimal_rate_bps as u128)
            .checked_add(mul_div_floor(range, above, span.max(1))?)
            .ok_or(LendingError::MathOverflow)?
    };
    let denominator = BPS_DENOMINATOR
        .checked_mul(SLOTS_PER_YEAR)
        .ok_or(LendingError::MathOverflow)?;
    mul_div_floor(apr_bps, FIXED_POINT_SCALE, denominator)
}

/// Advance the interest index for elapsed slots:
/// `new_index = index * (1 + rate_per_slot * elapsed)`.
#[allow(clippy::too_many_arguments)]
pub fn accrue_index(
    index: u128,
    borrowed_scaled: u128,
    available: u64,
    last_update_slot: u64,
    now: u64,
    optimal_utilization_bps: u16,
    min_rate_bps: u16,
    optimal_rate_bps: u16,
    max_rate_bps: u16,
) -> Result<u128, ProgramError> {
    let elapsed = now
        .checked_sub(last_update_slot)
        .ok_or(LendingError::MathOverflow)?;
    if elapsed == 0 || borrowed_scaled == 0 {
        return Ok(index);
    }
    let utilization = utilization_bps(available, borrowed_scaled, index)?;
    let rate = borrow_rate_per_slot(
        utilization,
        optimal_utilization_bps,
        min_rate_bps,
        optimal_rate_bps,
        max_rate_bps,
    )?;
    let growth = FIXED_POINT_SCALE
        .checked_add(rate.checked_mul(elapsed as u128).ok_or(LendingError::MathOverflow)?)
        .ok_or(LendingError::MathOverflow)?;
    mul_div_floor(index, growth, FIXED_POINT_SCALE)
}

#[allow(clippy::too_many_arguments)]
pub fn validate_config(
    loan_to_value_bps: u16,
    liquidation_threshold_bps: u16,
    liquidation_bonus_bps: u16,
    close_factor_bps: u16,
    reserve_factor_bps: u16,
    optimal_utilization_bps: u16,
    min_borrow_rate_bps: u16,
    optimal_borrow_rate_bps: u16,
    max_borrow_rate_bps: u16,
) -> Result<(), ProgramError> {
    let within = |value: u16| (value as u128) <= BPS_DENOMINATOR;
    require!(
        within(loan_to_value_bps)
            && within(liquidation_threshold_bps)
            && within(liquidation_bonus_bps)
            && within(close_factor_bps)
            && within(reserve_factor_bps)
            && within(optimal_utilization_bps),
        LendingError::InvalidConfig
    );
    require!(close_factor_bps > 0, LendingError::InvalidConfig);
    require!(
        optimal_utilization_bps > 0 && (optimal_utilization_bps as u128) < BPS_DENOMINATOR,
        LendingError::InvalidConfig
    );
    require!(
        loan_to_value_bps <= liquidation_threshold_bps,
        LendingError::InvalidConfig
    );
    require!(
        min_borrow_rate_bps <= optimal_borrow_rate_bps
            && optimal_borrow_rate_bps <= max_borrow_rate_bps,
        LendingError::InvalidConfig
    );
    Ok(())
}
