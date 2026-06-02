use anchor_lang::prelude::*;

use crate::constants::{FIXED_POINT_SCALE_DECIMALS, RESERVE_SEED};
use crate::errors::LendingError;

/// Which way to break ties when a division truncates. Deposits/redeems and
/// collateral valuations round the user's favourable quantity DOWN; debt and
/// protocol-owed quantities round UP. The protocol never loses a base unit to
/// rounding, so dust cannot be extracted by repeated round-trips.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Rounding {
    Down,
    Up,
}

/// 10^exponent as a u128, erroring instead of wrapping.
pub fn ten_pow(exponent: u32) -> Result<u128> {
    Ok(10u128
        .checked_pow(exponent)
        .ok_or(LendingError::MathOverflow)?)
}

/// floor((a * b) / denominator), computed in u128.
pub fn mul_div_floor(a: u128, b: u128, denominator: u128) -> Result<u128> {
    require!(denominator > 0, LendingError::MathOverflow);
    let product = a.checked_mul(b).ok_or(LendingError::MathOverflow)?;
    Ok(product
        .checked_div(denominator)
        .ok_or(LendingError::MathOverflow)?)
}

/// ceil((a * b) / denominator), computed in u128.
pub fn mul_div_ceil(a: u128, b: u128, denominator: u128) -> Result<u128> {
    require!(denominator > 0, LendingError::MathOverflow);
    let product = a.checked_mul(b).ok_or(LendingError::MathOverflow)?;
    let rounding = denominator
        .checked_sub(1)
        .ok_or(LendingError::MathOverflow)?;
    Ok(product
        .checked_add(rounding)
        .ok_or(LendingError::MathOverflow)?
        .checked_div(denominator)
        .ok_or(LendingError::MathOverflow)?)
}

fn mul_div(a: u128, b: u128, denominator: u128, rounding: Rounding) -> Result<u128> {
    match rounding {
        Rounding::Down => mul_div_floor(a, b, denominator),
        Rounding::Up => mul_div_ceil(a, b, denominator),
    }
}

/// Quote-currency value (in FIXED_POINT_SCALE-scaled units) of `amount` base
/// units of a token with `decimals`, given `price_scaled` from a price feed.
///
/// `price_scaled` already carries the FIXED_POINT_SCALE factor (it is the real
/// price multiplied by FIXED_POINT_SCALE, see `PriceFeed::price_scaled`), so the
/// value is `amount * price_scaled / 10^decimals`.
pub fn market_value(
    amount: u64,
    decimals: u8,
    price_scaled: u128,
    rounding: Rounding,
) -> Result<u128> {
    let divisor = ten_pow(decimals as u32)?;
    mul_div(amount as u128, price_scaled, divisor, rounding)
}

/// Inverse of [`market_value`]: how many base units of a token with `decimals`
/// are worth `value_scaled` quote-currency value at `price_scaled`.
pub fn value_to_amount(
    value_scaled: u128,
    decimals: u8,
    price_scaled: u128,
    rounding: Rounding,
) -> Result<u64> {
    let multiplier = ten_pow(decimals as u32)?;
    let amount = mul_div(value_scaled, multiplier, price_scaled, rounding)?;
    u64::try_from(amount).map_err(|_| LendingError::MathOverflow.into())
}

/// Combine a price feed's exponent with the fixed-point scale into a single net
/// power of ten. `price_scaled = real_price * FIXED_POINT_SCALE`, and
/// `real_price = mantissa * 10^exponent`, so
/// `price_scaled = mantissa * 10^(exponent + FIXED_POINT_SCALE_DECIMALS)`.
/// Folding the two powers avoids forming a 10^18 intermediate that would
/// overflow for high-priced assets.
pub fn price_mantissa_to_scaled(mantissa: u128, exponent: i32) -> Result<u128> {
    let net_exponent = exponent
        .checked_add(FIXED_POINT_SCALE_DECIMALS)
        .ok_or(LendingError::MathOverflow)?;
    if net_exponent >= 0 {
        Ok(mantissa
            .checked_mul(ten_pow(net_exponent as u32)?)
            .ok_or(LendingError::MathOverflow)?)
    } else {
        Ok(mantissa
            .checked_div(ten_pow((-net_exponent) as u32)?)
            .ok_or(LendingError::MathOverflow)?)
    }
}

/// Signer seeds for a reserve PDA, which is the authority over its liquidity
/// vault and the mint authority of its share token.
pub fn reserve_signer_seeds<'a>(
    lending_market: &'a Pubkey,
    liquidity_mint: &'a Pubkey,
    bump: &'a [u8; 1],
) -> [&'a [u8]; 4] {
    [
        RESERVE_SEED,
        lending_market.as_ref(),
        liquidity_mint.as_ref(),
        bump,
    ]
}
