//! Kani harnesses for the managed-fund program (`finance/managed-fund`).
//!
//! Inspired by aeyakovenko/percolator, which uses the Kani model checker to
//! check the arithmetic of a DeFi engine's pure numeric core. Kani marks a
//! harness with `#[kani::proof]`, which is why the crate is `kani-proofs` and
//! the harnesses are named `proof_*`; each one is a model check over every value
//! of the inputs it declares.
//!
//! The program is an ERC4626-style share vault: depositors mint share tokens
//! against the fund's net asset value, and withdrawals burn shares for a
//! proportional slice of every vault balance. A manager fee mints a small slice
//! of shares over time. Token movement is via SPL CPIs Kani cannot symbolically
//! execute, but the share math (`deposit`, `withdraw`, `collect_fees`) is pure
//! integer arithmetic. This crate reproduces it faithfully and checks the
//! invariants the fund's solvency rests on.
//!
//! The program prices shares and pays withdrawals from the holdings it has
//! recorded (`Fund::usdc_holdings` and `asset_holdings`), never from the
//! vaults' token balances, so tokens donated straight into a vault are outside
//! the fund. The harnesses model a vault as a (recorded, balance) pair and check
//! what that buys: payouts never exceed the real balance, and a donation cannot
//! dilute the next depositor.
//!
//! Valuation converts between an asset's minor units and USDC minor units
//! through the asset's decimals and the Pyth feed's exponent, and `rebalance`
//! sizes its trade from those values. The harnesses check that the conversion
//! never sells more than the trade it was asked for, and that a rebalance
//! trade never pushes either asset past its target.
//!
//! Nonlinear 128-bit harnesses use bounded model checking (small symbolic
//! inputs), as percolator does; the share identities are scale-invariant.

#![cfg_attr(kani, allow(dead_code))]

/// `floor((a*b)/d)`, `None` on overflow / zero divisor.
pub fn mul_div_floor(a: u128, b: u128, d: u128) -> Option<u128> {
    if d == 0 {
        return None;
    }
    a.checked_mul(b)?.checked_div(d)
}

/// Proportional withdrawal from one vault's recorded holding:
/// `floor(holding * shares / total)` — the formula `handle_withdraw` applies to
/// the USDC leg and to every basket asset.
pub fn withdraw_amount(balance: u64, shares_burned: u64, total_shares: u64) -> Option<u64> {
    mul_div_floor(balance as u128, shares_burned as u128, total_shares as u128)?
        .try_into()
        .ok()
}

/// Shares minted for a deposit: `floor(usdc_amount * total_shares / nav)`, where
/// `nav` is valued from recorded holdings (`handle_deposit`; the first deposit,
/// `total_shares == 0`, mints 1:1).
pub fn deposit_shares(usdc_amount: u64, total_shares: u64, nav: u64) -> Option<u64> {
    if total_shares == 0 {
        return Some(usdc_amount);
    }
    mul_div_floor(usdc_amount as u128, total_shares as u128, nav as u128)?
        .try_into()
        .ok()
}

/// `numerator * 10^power / denominator`, floored, for a power of either sign
/// (`mul_pow10_div` in `oracle.rs`).
pub fn mul_pow10_div(numerator: u128, power: i32, denominator: u128) -> Option<u128> {
    let scale = 10u128.checked_pow(power.unsigned_abs())?;
    if power >= 0 {
        numerator.checked_mul(scale)?.checked_div(denominator)
    } else {
        numerator.checked_div(denominator.checked_mul(scale)?)
    }
}

/// USDC minor units that `amount` asset minor units are worth:
/// `amount * price * 10^(usdc_decimals + exponent - asset_decimals)`, floored
/// (`asset_value_in_usdc`).
pub fn asset_value_in_usdc(
    amount: u128,
    price: u128,
    exponent: i32,
    asset_decimals: u8,
    usdc_decimals: u8,
) -> Option<u128> {
    let power = usdc_decimals as i32 + exponent - asset_decimals as i32;
    mul_pow10_div(amount.checked_mul(price)?, power, 1)
}

/// Asset minor units that `usdc_amount` USDC minor units buys at the oracle
/// price, floored (`usdc_to_asset_amount`).
pub fn usdc_to_asset_amount(
    usdc_amount: u128,
    price: u128,
    exponent: i32,
    asset_decimals: u8,
    usdc_decimals: u8,
) -> Option<u128> {
    let power = asset_decimals as i32 - exponent - usdc_decimals as i32;
    mul_pow10_div(usdc_amount, power, price)
}

/// The value `handle_rebalance` trades: the smaller of how far the sell asset
/// sits above its target and how far the buy asset sits below its own.
/// `None` when either gap is zero, where the handler refuses.
pub fn rebalance_trade_value(
    sell_value: u128,
    sell_target: u128,
    buy_value: u128,
    buy_target: u128,
) -> Option<u128> {
    let excess = sell_value.saturating_sub(sell_target);
    let shortfall = buy_target.saturating_sub(buy_value);
    if excess == 0 || shortfall == 0 {
        return None;
    }
    Some(excess.min(shortfall))
}

// ===========================================================================
// 1. Withdrawal solvency
// ===========================================================================

/// A withdrawal can never take more of any vault balance than it holds. Because
/// the burned shares are at most the total supply, the proportional slice
/// `floor(balance * shares / total)` is `<= balance`. This holds for the USDC
/// leg and for every in-kind asset leg, so a withdrawal can never overdraw a
/// vault — the core solvency property.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_withdraw_within_balance() {
    let balance: u64 = kani::any();
    let shares_burned: u64 = kani::any();
    let total_shares: u64 = kani::any();

    // Bounded model checking (nonlinear `balance * shares`, symbolic divisor
    // `total_shares`).
    kani::assume(balance as u128 <= 255);
    kani::assume(total_shares >= 1 && total_shares <= 255);
    kani::assume(shares_burned <= total_shares); // can't burn more than supply

    let out = withdraw_amount(balance, shares_burned, total_shares).expect("computes");
    assert!(out <= balance);
    // And withdrawing the entire supply takes exactly the whole balance.
    if shares_burned == total_shares {
        assert_eq!(out, balance);
    }
}

// ===========================================================================
// 2. Deposit -> withdraw round-trip cannot extract value
// ===========================================================================

/// In a USDC-only vault (NAV == vault USDC, no basket assets), depositing and
/// immediately withdrawing the minted shares never returns more USDC than was
/// deposited. Both legs floor in the program's favour, so a deposit/withdraw
/// round-trip is never profitable — there is no rounding attack that mints
/// shares worth more than they cost.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_deposit_withdraw_cannot_extract() {
    let amount: u64 = kani::any();
    let total_shares: u64 = kani::any();
    let nav: u64 = kani::any(); // == vault USDC balance for a USDC-only vault

    // Bounded model checking; an established vault has positive supply and NAV.
    kani::assume(amount <= 31);
    kani::assume(total_shares >= 1 && total_shares <= 31);
    kani::assume(nav >= 1 && nav <= 31);

    let minted = deposit_shares(amount, total_shares, nav).expect("computes");

    // State after the deposit.
    let new_total = (total_shares as u128) + (minted as u128);
    let new_vault = (nav as u128) + (amount as u128);

    // Withdraw exactly the freshly minted shares.
    let back = mul_div_floor(new_vault, minted as u128, new_total).expect("computes");
    assert!(back <= amount as u128); // round-trip never profitable
}

// ===========================================================================
// 3. Recorded holdings never exceed the vault's balance
// ===========================================================================

/// One vault as the program sees it (`recorded`) and as the token program does
/// (`balance`). A deposit adds to both, a donation adds to the balance only, and
/// a withdrawal pays `withdraw_amount` of the recorded holding out of both. If
/// `recorded <= balance` holds before each step it holds after it, and every
/// payout is covered by the real balance: the program can never promise tokens
/// the vault does not hold, however much is donated.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_recorded_holdings_never_exceed_balance() {
    let recorded: u64 = kani::any();
    let balance: u64 = kani::any();
    let deposit: u64 = kani::any();
    let donation: u64 = kani::any();
    let shares_burned: u64 = kani::any();
    let total_shares: u64 = kani::any();

    kani::assume(recorded <= balance && balance <= 255);
    kani::assume(deposit <= 255 && donation <= 255);
    kani::assume(total_shares >= 1 && total_shares <= 255);
    kani::assume(shares_burned <= total_shares);

    // Deposit: the swap output is recorded and lands in the vault.
    let recorded = recorded + deposit;
    let balance = balance + deposit;
    assert!(recorded <= balance);

    // Donation: tokens land in the vault and nothing is recorded.
    let balance = balance + donation;
    assert!(recorded <= balance);

    // Withdrawal: paid from the recorded holding, out of the real balance.
    let payout = withdraw_amount(recorded, shares_burned, total_shares).expect("computes");
    assert!(payout <= recorded);
    assert!(payout <= balance);
    assert!(recorded - payout <= balance - payout);
}

// ===========================================================================
// 4. A donation cannot dilute the next deposit
// ===========================================================================

/// The inflation attack against recorded holdings, in a USDC-only vault: an
/// attacker's first deposit mints one share per minor unit, a donation of any
/// size lands in the vault, and then the victim deposits. The donation is not in
/// the recorded NAV, so the victim's shares are exactly their deposit, the same
/// as with no donation, and withdrawing them returns every minor unit.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_donation_cannot_dilute_next_deposit() {
    let attacker_deposit: u64 = kani::any();
    let donation: u64 = kani::any();
    let victim_deposit: u64 = kani::any();

    kani::assume(attacker_deposit >= 1 && attacker_deposit <= 31);
    kani::assume(victim_deposit <= 31);

    let attacker_shares = deposit_shares(attacker_deposit, 0, 0).expect("computes");
    let recorded = attacker_deposit;
    let _balance = recorded as u128 + donation as u128; // counted nowhere below

    let victim_shares =
        deposit_shares(victim_deposit, attacker_shares, recorded).expect("computes");
    assert_eq!(victim_shares, victim_deposit);

    let total = attacker_shares + victim_shares;
    let back = withdraw_amount(recorded + victim_deposit, victim_shares, total).expect("computes");
    assert_eq!(back, victim_deposit);
}

// ===========================================================================
// 5. Manager fee dilution is bounded
// ===========================================================================

/// The time-based manager fee mints
/// `fee_shares = floor(total_shares * fee_bps * elapsed / (10_000 * SECONDS_PER_YEAR))`.
/// Over at most one year (`elapsed <= SECONDS_PER_YEAR`) with a valid fee rate
/// (`fee_bps <= 10_000`), the combined numerator factor `fee_bps * elapsed` is
/// `<= 10_000 * SECONDS_PER_YEAR`, so `fee_shares <= total_shares`: the manager
/// can never mint more than a 100%-per-year dilution. Modelled with the combined
/// `numerator_factor <= denominator` (the constraint the two bounds imply).
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_fee_shares_bounded_by_supply() {
    let total_shares: u64 = kani::any();
    let numerator_factor: u128 = kani::any(); // fee_bps * elapsed
    let denominator: u128 = kani::any(); // 10_000 * SECONDS_PER_YEAR

    kani::assume(total_shares as u128 <= 255);
    kani::assume(denominator >= 1 && denominator <= 255);
    // fee_bps <= 10_000 and elapsed <= SECONDS_PER_YEAR together give:
    kani::assume(numerator_factor <= denominator);

    let fee_shares =
        mul_div_floor(total_shares as u128, numerator_factor, denominator).expect("computes");
    assert!(fee_shares <= total_shares as u128); // <= 100%/year dilution
}

// ===========================================================================
// 6. Unit conversion never sells more than the trade
// ===========================================================================

/// `rebalance` converts its trade value into an amount of the sell asset, then
/// floors the sale's minimum output by valuing that amount back. Across every
/// combination of decimals and exponent in range, the amount sold is never
/// worth more than the trade value, so flooring cannot make a rebalance sell
/// past the asset's target.
#[cfg(kani)]
#[kani::proof]
#[kani::solver(cadical)]
fn proof_sell_amount_never_worth_more_than_trade() {
    let trade: u128 = kani::any();
    let price: u128 = kani::any();
    let exponent: i32 = kani::any();
    let asset_decimals: u8 = kani::any();
    let usdc_decimals: u8 = kani::any();

    kani::assume(trade <= 255);
    kani::assume(price >= 1 && price <= 255);
    kani::assume(exponent >= -3 && exponent <= 0);
    kani::assume(asset_decimals <= 3 && usdc_decimals <= 3);

    let amount = usdc_to_asset_amount(trade, price, exponent, asset_decimals, usdc_decimals)
        .expect("computes");
    let worth = asset_value_in_usdc(amount, price, exponent, asset_decimals, usdc_decimals)
        .expect("computes");
    assert!(worth <= trade);
}

// ===========================================================================
// 7. A rebalance trade never overshoots a target
// ===========================================================================

/// The trade is the smaller of the two gaps, so after it the sell asset is
/// still at or above its target and the buy asset at or below its own, valued
/// at the same prices. A rebalance therefore only ever closes gaps, and once it
/// has closed one there is nothing left to trade on that side: a caller cannot
/// trade the fund back and forth.
#[cfg(kani)]
#[kani::proof]
fn proof_rebalance_trade_never_overshoots() {
    let sell_value: u128 = kani::any();
    let sell_target: u128 = kani::any();
    let buy_value: u128 = kani::any();
    let buy_target: u128 = kani::any();

    kani::assume(sell_value <= u64::MAX as u128 && sell_target <= u64::MAX as u128);
    kani::assume(buy_value <= u64::MAX as u128 && buy_target <= u64::MAX as u128);

    if let Some(trade) = rebalance_trade_value(sell_value, sell_target, buy_value, buy_target) {
        assert!(trade > 0);
        assert!(sell_value - trade >= sell_target);
        assert!(buy_value + trade <= buy_target);
        // Valued again, at least one side has no gap left.
        let again = rebalance_trade_value(
            sell_value - trade,
            sell_target,
            buy_value + trade,
            buy_target,
        );
        assert!(again.is_none());
    }
}

// ===========================================================================
// Plain unit tests.
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn withdraw_proportional() {
        // Burn half the supply -> get half the balance.
        assert_eq!(withdraw_amount(1000, 50, 100).unwrap(), 500);
        // Burn all -> get all.
        assert_eq!(withdraw_amount(1000, 100, 100).unwrap(), 1000);
    }

    #[test]
    fn deposit_first_is_one_to_one() {
        assert_eq!(deposit_shares(500, 0, 0).unwrap(), 500);
    }

    #[test]
    fn donation_does_not_dilute_next_deposit() {
        // The attack from the book: one minor unit deposited, 1,000 USDC donated
        // (and not recorded), then a 1,000 USDC deposit.
        let attacker = deposit_shares(1, 0, 0).unwrap();
        let victim = deposit_shares(1_000_000_000, attacker, 1).unwrap();
        assert_eq!(victim, 1_000_000_000);
        let back = withdraw_amount(1 + 1_000_000_000, victim, attacker + victim).unwrap();
        assert_eq!(back, 1_000_000_000);
    }

    #[test]
    fn valuation_scales_by_decimals_and_exponent() {
        // 1.44 TSLAx at eight decimals, $250 on a Pyth equity feed (exponent -5),
        // is 360 USDC.
        assert_eq!(
            asset_value_in_usdc(144_000_000, 25_000_000, -5, 8, 6).unwrap(),
            360_000_000
        );
        // 3 NVDAx at six decimals, $180 at exponent -8, is 540 USDC.
        assert_eq!(
            asset_value_in_usdc(3_000_000, 18_000_000_000, -8, 6, 6).unwrap(),
            540_000_000
        );
        // 24 USDC buys 0.096 TSLAx at eight decimals.
        assert_eq!(
            usdc_to_asset_amount(24_000_000, 25_000_000, -5, 8, 6).unwrap(),
            9_600_000
        );
    }

    #[test]
    fn rebalance_trades_the_smaller_gap() {
        // The book's rebalance: NVDAx $24 over its $576 target, TSLAx $24 under.
        assert_eq!(rebalance_trade_value(600, 576, 360, 384), Some(24));
        // A fund at target has nothing to trade.
        assert_eq!(rebalance_trade_value(576, 576, 384, 384), None);
    }

    #[test]
    fn round_trip_not_profitable() {
        let minted = deposit_shares(100, 200, 150).unwrap();
        let back =
            mul_div_floor((150 + 100) as u128, minted as u128, (200 + minted) as u128).unwrap();
        assert!(back <= 100);
    }
}
