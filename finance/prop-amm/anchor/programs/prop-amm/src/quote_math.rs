//! The market's pure pricing arithmetic, separated from account handling so it
//! can be unit-tested and model-checked (see `finance/prop-amm/kani-proofs`)
//! without the Solana machinery.
//!
//! Every function returns `None` on the paths the program maps to
//! `PropAmmError::MathOverflow`. Rounding always favors the market: the ask is
//! rounded up, the bid is rounded down, and both output amounts floor. The
//! trader-favoring direction never appears, so the invariant "output value at
//! the oracle price never exceeds input value" holds by construction — and the
//! swap handler still asserts it after the fact.
//!
//! Unit conventions: `oracle_price` is quote tokens per whole base token, as a
//! fixed-point integer with `oracle_scale` decimals (e.g. scale 8 makes $165
//! into 165 * 10^8). Amounts in and out are minor units of their own token,
//! with `base_decimals` / `quote_decimals` decimal places. Nothing assumes the
//! two tokens have the same decimals.

/// Basis-point denominator, mirroring `constants::BASIS_POINTS_DENOMINATOR`.
const BASIS_POINTS: u128 = 10_000;

/// The price a buyer of the base token pays: oracle plus the spread, rounded
/// UP so the rounding penny goes to the market, not the buyer.
pub fn ask_price(oracle_price: u64, spread_bps: u16) -> Option<u128> {
    let numerator =
        (oracle_price as u128).checked_mul(BASIS_POINTS.checked_add(spread_bps as u128)?)?;
    Some(numerator.div_ceil(BASIS_POINTS))
}

/// The price a seller of the base token receives: oracle minus the spread,
/// rounded DOWN — the same coin, the other face.
pub fn bid_price(oracle_price: u64, spread_bps: u16) -> Option<u128> {
    let numerator =
        (oracle_price as u128).checked_mul(BASIS_POINTS.checked_sub(spread_bps as u128)?)?;
    Some(numerator / BASIS_POINTS)
}

/// Base tokens out for `quote_in` at the ask, floored.
///
/// Derivation: whole base out = (quote_in / 10^quote_decimals) / (ask / 10^scale),
/// then scaled to base minor units by 10^base_decimals. Multiply everything
/// before the one division so the floor happens exactly once, at the end.
pub fn base_out_for_quote_in(
    quote_in: u64,
    ask: u128,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Option<u64> {
    let numerator = (quote_in as u128)
        .checked_mul(10u128.checked_pow(oracle_scale)?)?
        .checked_mul(10u128.checked_pow(base_decimals as u32)?)?;
    let denominator = ask.checked_mul(10u128.checked_pow(quote_decimals as u32)?)?;
    if denominator == 0 {
        return None;
    }
    u64::try_from(numerator / denominator).ok()
}

/// Quote tokens out for `base_in` at the bid, floored.
pub fn quote_out_for_base_in(
    base_in: u64,
    bid: u128,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Option<u64> {
    let numerator = (base_in as u128)
        .checked_mul(bid)?
        .checked_mul(10u128.checked_pow(quote_decimals as u32)?)?;
    let denominator =
        10u128.checked_pow(oracle_scale)?.checked_mul(10u128.checked_pow(base_decimals as u32)?)?;
    if denominator == 0 {
        return None;
    }
    u64::try_from(numerator / denominator).ok()
}

/// Both sides of a swap valued in the same fixed-point unit
/// (quote minor units × 10^scale × 10^base_decimals), cross-multiplied so no
/// division — and therefore no second rounding — is involved. `None` on
/// overflow.
fn values_at_oracle(
    base_amount: u64,
    quote_amount: u64,
    oracle_price: u64,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Option<(u128, u128)> {
    let base_value = (base_amount as u128)
        .checked_mul(oracle_price as u128)?
        .checked_mul(10u128.checked_pow(quote_decimals as u32)?)?;
    let quote_value = (quote_amount as u128)
        .checked_mul(10u128.checked_pow(oracle_scale)?)?
        .checked_mul(10u128.checked_pow(base_decimals as u32)?)?;
    Some((base_value, quote_value))
}

/// The swap handler's post-math invariant for a buy: the base tokens handed
/// out must be worth no more, at the raw oracle price (no spread), than the
/// quote tokens taken in. Whatever the quoting arithmetic above produced, the
/// market never pays out more value than it received.
pub fn buy_respects_oracle_value(
    quote_in: u64,
    base_out: u64,
    oracle_price: u64,
    oracle_scale: u32,
    base_decimals: u8,
    quote_decimals: u8,
) -> Option<bool> {
    let (base_value, quote_value) = values_at_oracle(
        base_out,
        quote_in,
        oracle_price,
        oracle_scale,
        base_decimals,
        quote_decimals,
    )?;
    Some(base_value <= quote_value)
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
) -> Option<bool> {
    let (base_value, quote_value) = values_at_oracle(
        base_in,
        quote_out,
        oracle_price,
        oracle_scale,
        base_decimals,
        quote_decimals,
    )?;
    Some(quote_value <= base_value)
}
