//! Helpers that bridge Quasar's zero-copy accounts and the integer math in
//! [`crate::math`]. Account scalar getters return Pod types, so these read each
//! field into a native-typed `*Inner` snapshot that math operates on and
//! `set_inner` writes back.

use quasar_lang::{prelude::*, sysvars::Sysvar};

use crate::{
    constants::{FIXED_POINT_SCALE, MAX_PRICE_STALENESS_SLOTS},
    error::LendingError,
    last_restart::LastRestartSlot,
    math::{accrue_factor, current_debt, mul_div_floor, price_mantissa_to_scaled},
    state::{Obligation, ObligationInner, PriceFeed, Reserve, ReserveInner},
};

use crate::constants::BPS_DENOMINATOR;

/// Current slot as a native `u64`.
pub fn now() -> Result<u64, ProgramError> {
    Ok(u64::from(Clock::get()?.slot))
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
        accumulated_protocol_fees: u64::from(reserve.accumulated_protocol_fees),
        borrowed_principal: u128::from(reserve.borrowed_principal),
        borrow_accumulation_factor: u128::from(reserve.borrow_accumulation_factor),
        last_update_slot: u64::from(reserve.last_update_slot),
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

/// Advance a reserve snapshot's accumulation factor to `slot` (a single
/// `factor *= 1 + rate_per_slot * elapsed` per call, compounding across calls).
pub fn accrue(reserve: &mut ReserveInner, slot: u64) -> Result<(), ProgramError> {
    let borrowed_before = current_debt(
        reserve.borrowed_principal,
        reserve.borrow_accumulation_factor,
    )?;
    reserve.borrow_accumulation_factor = accrue_factor(
        reserve.borrow_accumulation_factor,
        reserve.borrowed_principal,
        reserve.available_liquidity,
        reserve.last_update_slot,
        slot,
        reserve.optimal_utilization_bps,
        reserve.min_borrow_rate_bps,
        reserve.optimal_borrow_rate_bps,
        reserve.max_borrow_rate_bps,
    )?;
    // The protocol keeps `reserve_factor_bps` of the newly accrued interest; the
    // rest lifts the supplier exchange rate. Flooring rounds the owner's cut down.
    let borrowed_after = current_debt(
        reserve.borrowed_principal,
        reserve.borrow_accumulation_factor,
    )?;
    let interest = borrowed_after.saturating_sub(borrowed_before);
    let fee = mul_div_floor(
        interest as u128,
        reserve.reserve_factor_bps as u128,
        BPS_DENOMINATOR,
    )?;
    reserve.accumulated_protocol_fees = reserve
        .accumulated_protocol_fees
        .checked_add(u64::try_from(fee).map_err(|_| LendingError::MathOverflow)?)
        .ok_or(LendingError::MathOverflow)?;
    reserve.last_update_slot = slot;
    Ok(())
}

/// The feed's price scaled by `FIXED_POINT_SCALE`, after staleness + positivity checks.
pub fn price_scaled(feed: &Account<PriceFeed>, slot: u64) -> Result<u128, ProgramError> {
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
    price_mantissa_to_scaled(mantissa as u128, i32::from(feed.exponent))
}

/// `FIXED_POINT_SCALE` re-export for handlers that scale borrow principal.
pub const SCALE: u128 = FIXED_POINT_SCALE;
