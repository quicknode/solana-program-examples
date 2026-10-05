//! Helpers that bridge Quasar's zero-copy accounts and the integer math in
//! [`crate::math`]. Account scalar getters return Pod types, so these read each
//! field into a native-typed `*Inner` snapshot that math operates on and
//! `set_inner` writes back.

use quasar_lang::{prelude::*, sysvars::Sysvar};

use crate::{
    constants::{FIXED_POINT_SCALE, MAX_PRICE_STALENESS_SLOTS},
    error::LendingError,
    last_restart::LastRestartSlot,
    math::{accrue_factor, current_debt, mul_div_ceil, price_mantissa_to_scaled},
    state::{Obligation, ObligationInner, PriceFeed, Reserve, ReserveInner},
};

use crate::constants::BPS_DENOMINATOR;

/// The Clock's slot and `unix_timestamp`, as native types. The slot measures
/// price freshness; the timestamp measures interest.
pub fn now() -> Result<(u64, i64), ProgramError> {
    let clock = Clock::get()?;
    Ok((u64::from(clock.slot), i64::from(clock.unix_timestamp)))
}

/// Read a reserve into a native-typed, mutable snapshot.
pub fn snapshot_reserve(reserve: &Account<Reserve>) -> ReserveInner {
    ReserveInner {
        lending_market: reserve.lending_market,
        liquidity_mint: reserve.liquidity_mint,
        liquidity_vault: reserve.liquidity_vault,
        share_mint: reserve.share_mint,
        price_feed: reserve.price_feed,
        available_liquidity: u64::from(reserve.available_liquidity),
        share_mint_supply: u64::from(reserve.share_mint_supply),
        accumulated_program_fees: u64::from(reserve.accumulated_program_fees),
        borrowed_principal: u128::from(reserve.borrowed_principal),
        borrow_accumulation_factor: u128::from(reserve.borrow_accumulation_factor),
        last_update_slot: u64::from(reserve.last_update_slot),
        last_accrual_timestamp: i64::from(reserve.last_accrual_timestamp),
        liquidity_decimals: reserve.liquidity_decimals,
        loan_to_value_bps: u16::from(reserve.loan_to_value_bps),
        liquidation_threshold_bps: u16::from(reserve.liquidation_threshold_bps),
        liquidation_bonus_bps: u16::from(reserve.liquidation_bonus_bps),
        close_factor_bps: u16::from(reserve.close_factor_bps),
        reserve_factor_bps: u16::from(reserve.reserve_factor_bps),
        optimal_utilization_bps: u16::from(reserve.optimal_utilization_bps),
        min_borrow_rate_bps: u16::from(reserve.min_borrow_rate_bps),
        optimal_borrow_rate_bps: u16::from(reserve.optimal_borrow_rate_bps),
        max_borrow_rate_bps: u16::from(reserve.max_borrow_rate_bps),
        max_confidence_bps: u16::from(reserve.max_confidence_bps),
        bump: reserve.bump,
    }
}

/// Read an obligation into a native-typed, mutable snapshot.
pub fn snapshot_obligation(obligation: &Account<Obligation>) -> ObligationInner {
    ObligationInner {
        lending_market: obligation.lending_market,
        owner: obligation.owner,
        collateral_reserve: obligation.collateral_reserve,
        deposited_shares: u64::from(obligation.deposited_shares),
        borrow_reserve: obligation.borrow_reserve,
        borrowed_principal: u128::from(obligation.borrowed_principal),
        bump: obligation.bump,
    }
}

/// Advance a reserve snapshot's accumulation factor for the seconds elapsed
/// since the last accrual, and record `current_slot` as the slot of this one.
/// `new_factor = factor * (1 + rate_per_second * elapsed_seconds)`, a single
/// multiply per call that compounds across calls (Solend's approach, on the
/// wall clock rather than the slot count).
///
/// The timestamp is written by each block's leader. The runtime rejects a
/// block whose time goes backwards, but a timestamp at or before the stored
/// one is still treated as no time elapsed, and the stored stamp is left
/// where it is, so no second is ever charged twice or skipped.
pub fn accrue(
    reserve: &mut ReserveInner,
    current_slot: u64,
    current_timestamp: i64,
) -> Result<(), ProgramError> {
    let elapsed = if current_timestamp > reserve.last_accrual_timestamp {
        current_timestamp
            .checked_sub(reserve.last_accrual_timestamp)
            .ok_or(LendingError::MathOverflow)? as u128
    } else {
        0
    };

    let borrowed_before = current_debt(
        reserve.borrowed_principal,
        reserve.borrow_accumulation_factor,
    )?;
    reserve.borrow_accumulation_factor = accrue_factor(
        reserve.borrow_accumulation_factor,
        reserve.borrowed_principal,
        reserve.available_liquidity,
        elapsed,
        reserve.optimal_utilization_bps,
        reserve.min_borrow_rate_bps,
        reserve.optimal_borrow_rate_bps,
        reserve.max_borrow_rate_bps,
    )?;
    // The program keeps `reserve_factor_bps` of the newly accrued interest; the
    // rest lifts the supplier exchange rate. The fee rounds up, in the owner's
    // favour: a fee is the program's cut and so rounds against the user, and
    // the suppliers take what is left, so the two parts sum to the interest and
    // never exceed it.
    let borrowed_after = current_debt(
        reserve.borrowed_principal,
        reserve.borrow_accumulation_factor,
    )?;
    let interest = borrowed_after.saturating_sub(borrowed_before);
    let fee = mul_div_ceil(
        interest as u128,
        reserve.reserve_factor_bps as u128,
        BPS_DENOMINATOR,
    )?;
    reserve.accumulated_program_fees = reserve
        .accumulated_program_fees
        .checked_add(u64::try_from(fee).map_err(|_| LendingError::MathOverflow)?)
        .ok_or(LendingError::MathOverflow)?;
    if elapsed > 0 {
        reserve.last_accrual_timestamp = current_timestamp;
    }
    reserve.last_update_slot = current_slot;
    Ok(())
}

/// The feed's price scaled by `FIXED_POINT_SCALE`, after asserting the feed
/// is fresh, positive, and no less certain than `max_confidence_bps` of the
/// price allows.
pub fn price_scaled(
    feed: &Account<PriceFeed>,
    slot: u64,
    max_confidence_bps: u16,
) -> Result<u128, ProgramError> {
    let last_updated = u64::from(feed.last_updated_slot);
    let age = slot
        .checked_sub(last_updated)
        .ok_or(LendingError::MathOverflow)?;
    require!(age <= MAX_PRICE_STALENESS_SLOTS, LendingError::StalePrice);

    // Restart handling. A cluster halt stops the slot count but not the wall
    // clock, so after a restart a feed can look fresh in slots while its
    // price is hours old. Reject any price stamped at or before the restart
    // slot; the market then pauses valuation until the publisher posts again,
    // rather than lending against a pre-halt price. Zero means the cluster
    // has never restarted.
    let last_restart = u64::from(LastRestartSlot::get()?.last_restart_slot);
    require!(
        last_restart == 0 || last_updated > last_restart,
        LendingError::PricePredatesRestart
    );

    let mantissa = i128::from(feed.price_mantissa);
    require!(mantissa > 0, LendingError::InvalidOraclePrice);

    // Reject a price the oracle itself is unsure of: the confidence band, as
    // a fraction of the price, must not exceed the reserve's limit.
    // `confidence` shares the mantissa's exponent, so the comparison needs no
    // scaling; it is `confidence / price <= max_confidence_bps / 10_000` with
    // both sides multiplied out so no division truncates.
    let band_scaled = (u64::from(feed.confidence) as u128)
        .checked_mul(BPS_DENOMINATOR)
        .ok_or(LendingError::MathOverflow)?;
    let limit_scaled = (mantissa as u128)
        .checked_mul(max_confidence_bps as u128)
        .ok_or(LendingError::MathOverflow)?;
    require!(
        band_scaled <= limit_scaled,
        LendingError::OracleConfidenceTooWide
    );

    price_mantissa_to_scaled(mantissa as u128, i32::from(feed.exponent))
}

/// `FIXED_POINT_SCALE` re-export for handlers that scale borrow principal.
pub const SCALE: u128 = FIXED_POINT_SCALE;
