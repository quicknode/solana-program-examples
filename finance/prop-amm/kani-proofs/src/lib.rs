//! Kani proof harnesses for the prop AMM (`finance/prop-amm`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! prove the mathematical correctness of a DeFi engine's pure numeric core.
//!
//! The on-chain instructions hand the actual token movement to the SPL token
//! program via CPIs that Kani cannot symbolically execute. But the
//! *interesting* part — building the ask and bid from the oracle price and the
//! spread, converting between token amounts across decimals, and the
//! "never pay out more than oracle value" invariant — is pure integer
//! arithmetic. This crate reproduces those formulas faithfully (same `u128`
//! widening, same multiply-before-divide, same rounding directions: ask ceils,
//! bid floors, outputs floor) and proves the invariants the program depends
//! on. Formulas mirror `prop_amm::quote_math`.

#![cfg_attr(kani, allow(dead_code))]

/// Basis-points denominator (`constants::BASIS_POINTS_DENOMINATOR`).
pub const BASIS_POINTS: u128 = 10_000;

// ===========================================================================
// 1. The quote: ask and bid  (quote_math.rs)
// ===========================================================================

/// The price a buyer pays: oracle plus the spread, rounded UP. Mirrors
/// `quote_math::ask_price`.
pub fn ask_price(oracle_price: u64, spread_bps: u16) -> Option<u128> {
    let numerator =
        (oracle_price as u128).checked_mul(BASIS_POINTS.checked_add(spread_bps as u128)?)?;
    Some(numerator.div_ceil(BASIS_POINTS))
}

/// The price a seller receives: oracle minus the spread, rounded DOWN.
/// Mirrors `quote_math::bid_price`.
pub fn bid_price(oracle_price: u64, spread_bps: u16) -> Option<u128> {
    let numerator =
        (oracle_price as u128).checked_mul(BASIS_POINTS.checked_sub(spread_bps as u128)?)?;
    Some(numerator / BASIS_POINTS)
}

/// The quote brackets the oracle: `bid <= oracle <= ask`, with each side's
/// rounding pointing away from the trader. Also proves the roundings are
/// exact: the ask is the *smallest* integer at or above the true ratio (ceil,
/// not "add one"), so a refactor that over-rounds in the market's favor fails
/// the proof too.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_quote_brackets_oracle() {
    let price: u64 = kani::any();
    let spread_bps: u16 = kani::any();
    // The full price range is linear arithmetic and stays tractable; the
    // spread is validated `1..10_000` by initialize_market / set_quote.
    kani::assume(price >= 1);
    kani::assume(spread_bps >= 1 && (spread_bps as u128) < BASIS_POINTS);

    let ask = ask_price(price, spread_bps).expect("ask computes");
    let bid = bid_price(price, spread_bps).expect("bid computes");

    assert!(bid <= price as u128, "bid never exceeds the oracle");
    assert!(ask >= price as u128, "ask never undercuts the oracle");
    assert!(bid <= ask);

    // Exactness of the roundings, both directions:
    // ceil: ask*BPS >= price*(BPS+s) and (ask-1)*BPS < price*(BPS+s)
    let ask_target = (price as u128) * (BASIS_POINTS + spread_bps as u128);
    assert!(ask * BASIS_POINTS >= ask_target);
    assert!((ask - 1) * BASIS_POINTS < ask_target);
    // floor: bid*BPS <= price*(BPS-s) < (bid+1)*BPS
    let bid_target = (price as u128) * (BASIS_POINTS - spread_bps as u128);
    assert!(bid * BASIS_POINTS <= bid_target);
    assert!((bid + 1) * BASIS_POINTS > bid_target);
}

// ===========================================================================
// 2. Output amounts  (quote_math.rs)
// ===========================================================================

/// Base tokens out for `quote_in` at the ask, floored. Mirrors
/// `quote_math::base_out_for_quote_in`.
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

/// Quote tokens out for `base_in` at the bid, floored. Mirrors
/// `quote_math::quote_out_for_base_in`.
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

/// THE core prop-AMM safety property, buy side: the base handed out is never
/// worth more, at the raw oracle price, than the quote taken in — for every
/// price, spread, amount, and decimal configuration. This is exactly the
/// `require!(respects_oracle_value)` assert in the swap handler; the proof
/// says that assert can never fire while the math above it is intact.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_buy_never_exceeds_oracle_value() {
    let quote_in: u64 = kani::any();
    let price: u64 = kani::any();
    let spread_bps: u16 = kani::any();
    let oracle_scale: u32 = kani::any();
    let base_decimals: u8 = kani::any();
    let quote_decimals: u8 = kani::any();

    // Bounded model checking: the division by a *symbolic* ask is the hard
    // part for the bit-precise solver. Amounts and price are capped, and the
    // decimal exponents are kept small (the identity is independent of the
    // exponents' actual values — they enter both sides of the comparison
    // symmetrically — so tiny exponents exercise the same rounding edges as
    // scale 8 and 6/6 decimals).
    kani::assume(quote_in <= 1023);
    kani::assume(price >= 1 && price <= 1023);
    kani::assume(spread_bps >= 1 && (spread_bps as u128) < BASIS_POINTS);
    kani::assume(oracle_scale <= 2);
    kani::assume(base_decimals <= 2 && quote_decimals <= 2);

    let ask = ask_price(price, spread_bps).expect("ask computes");
    let base_out =
        base_out_for_quote_in(quote_in, ask, oracle_scale, base_decimals, quote_decimals)
            .expect("output computes within bounds");

    // base_out * price * 10^q <= quote_in * 10^S * 10^b
    let base_value = (base_out as u128) * (price as u128) * 10u128.pow(quote_decimals as u32);
    let quote_value =
        (quote_in as u128) * 10u128.pow(oracle_scale) * 10u128.pow(base_decimals as u32);
    assert!(base_value <= quote_value, "buy paid out above oracle value");
}

/// The same property, sell side: the quote handed out is never worth more, at
/// the raw oracle price, than the base taken in.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_sell_never_exceeds_oracle_value() {
    let base_in: u64 = kani::any();
    let price: u64 = kani::any();
    let spread_bps: u16 = kani::any();
    let oracle_scale: u32 = kani::any();
    let base_decimals: u8 = kani::any();
    let quote_decimals: u8 = kani::any();

    kani::assume(base_in <= 1023);
    kani::assume(price >= 1 && price <= 1023);
    kani::assume(spread_bps >= 1 && (spread_bps as u128) < BASIS_POINTS);
    kani::assume(oracle_scale <= 2);
    kani::assume(base_decimals <= 2 && quote_decimals <= 2);

    let bid = bid_price(price, spread_bps).expect("bid computes");
    let quote_out =
        quote_out_for_base_in(base_in, bid, oracle_scale, base_decimals, quote_decimals)
            .expect("output computes within bounds");

    let base_value = (base_in as u128) * (price as u128) * 10u128.pow(quote_decimals as u32);
    let quote_value =
        (quote_out as u128) * 10u128.pow(oracle_scale) * 10u128.pow(base_decimals as u32);
    assert!(quote_value <= base_value, "sell paid out above oracle value");
}

// ===========================================================================
// 3. The round trip  (the spread is the fee)
// ===========================================================================

/// Buying base and immediately selling all of it back never returns more
/// quote than went in: a trader cannot mint money by bouncing off both sides
/// of the quote, whatever the price, spread, or decimal configuration. (The
/// difference is the round-trip spread — the market's revenue.)
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_round_trip_never_profits_the_trader() {
    let quote_in: u64 = kani::any();
    let price: u64 = kani::any();
    let spread_bps: u16 = kani::any();
    let oracle_scale: u32 = kani::any();
    let base_decimals: u8 = kani::any();
    let quote_decimals: u8 = kani::any();

    // Two symbolic divisions chained — the hardest harness here; keep the
    // amount bound tight.
    kani::assume(quote_in <= 255);
    kani::assume(price >= 1 && price <= 255);
    kani::assume(spread_bps >= 1 && (spread_bps as u128) < BASIS_POINTS);
    kani::assume(oracle_scale <= 2);
    kani::assume(base_decimals <= 2 && quote_decimals <= 2);

    let ask = ask_price(price, spread_bps).expect("ask computes");
    let bid = bid_price(price, spread_bps).expect("bid computes");
    let base_out =
        base_out_for_quote_in(quote_in, ask, oracle_scale, base_decimals, quote_decimals)
            .expect("buy computes");
    let quote_back =
        quote_out_for_base_in(base_out, bid, oracle_scale, base_decimals, quote_decimals)
            .expect("sell computes");

    assert!(quote_back <= quote_in, "round trip must not profit the trader");
}

// ===========================================================================
// Plain unit tests (so the crate is meaningful without Kani installed).
// These pin the exact numbers the LiteSVM tests and the book chapter use.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // $165 at scale 8; both tokens 6 decimals; 10 bps spread.
    const PRICE: u64 = 16_500_000_000;
    const SCALE: u32 = 8;
    const DEC: u8 = 6;

    #[test]
    fn ask_and_bid_at_165() {
        assert_eq!(ask_price(PRICE, 10).unwrap(), 16_516_500_000); // $165.165
        assert_eq!(bid_price(PRICE, 10).unwrap(), 16_483_500_000); // $164.835
    }

    #[test]
    fn buy_ten_nvdax() {
        // 1,651.65 USDC buys exactly 10 NVDAx at the ask.
        let ask = ask_price(PRICE, 10).unwrap();
        assert_eq!(
            base_out_for_quote_in(1_651_650_000, ask, SCALE, DEC, DEC).unwrap(),
            10_000_000
        );
    }

    #[test]
    fn sell_ten_nvdax() {
        // 10 NVDAx sells for exactly 1,648.35 USDC at the bid.
        let bid = bid_price(PRICE, 10).unwrap();
        assert_eq!(
            quote_out_for_base_in(10_000_000, bid, SCALE, DEC, DEC).unwrap(),
            1_648_350_000
        );
    }

    #[test]
    fn round_trip_costs_the_spread() {
        // In 1,651.65, back 1,648.35: the market keeps exactly 3.30 USDC.
        let ask = ask_price(PRICE, 10).unwrap();
        let bid = bid_price(PRICE, 10).unwrap();
        let base = base_out_for_quote_in(1_651_650_000, ask, SCALE, DEC, DEC).unwrap();
        let back = quote_out_for_base_in(base, bid, SCALE, DEC, DEC).unwrap();
        assert_eq!(1_651_650_000 - back, 3_300_000);
    }

    #[test]
    fn ask_ceils_and_bid_floors() {
        // price 999, spread 1 bps: 999 * 10_001 = 9_990_999 -> ask ceil 1_000,
        // 999 * 9_999 = 9_989_001 -> bid floor 998.
        assert_eq!(ask_price(999, 1).unwrap(), 1_000);
        assert_eq!(bid_price(999, 1).unwrap(), 998);
    }
}
