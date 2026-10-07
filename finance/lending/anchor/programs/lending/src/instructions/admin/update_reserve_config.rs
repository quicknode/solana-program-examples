use anchor_lang::prelude::*;

use crate::errors::LendingError;
use crate::state::{LendingMarket, Reserve, ReserveConfig};

/// Replace a reserve's risk and interest-rate config. Only the market's owner
/// may call it. The new config must pass `ReserveConfig::validate` (which
/// includes the `BORROW_RATE_CEILING_BPS` cap on every rate field and the
/// bound that keeps the liquidation bonus payable at the threshold), and it may
/// not lower `liquidation_threshold_bps` below the reserve's current value
/// (`RiskLimitLowered`), so an update cannot turn an open borrow liquidatable.
/// `loan_to_value_bps` may be lowered, down to 0 to stop new borrowing against
/// the asset: it limits only new borrows and withdrawals by an indebted
/// borrower, never whether an open borrow is liquidatable.
pub fn handle_update_reserve_config(
    context: &mut Context<UpdateReserveConfig>,
    config: ReserveConfig,
) -> Result<()> {
    config.validate()?;
    // The liquidation threshold only ever rises: lowering it could make an
    // open borrow liquidatable the moment the update lands. The loan-to-value
    // is not ratcheted, because health is measured against the threshold, so
    // lowering it touches no open borrow and is how the owner stops new
    // borrowing against an asset that has become dangerous.
    let current = &context.accounts.reserve.config;
    require!(
        config.liquidation_threshold_bps >= current.liquidation_threshold_bps,
        LendingError::RiskLimitLowered
    );
    // Accrue at the old curve first, so the seconds since the last refresh are
    // charged at the rates that applied to them rather than repriced by the new
    // ones.
    let clock = Clock::get()?;
    let reserve = &mut context.accounts.reserve;
    reserve.accrue_interest(clock.slot, clock.unix_timestamp)?;
    reserve.config = config;
    Ok(())
}

#[derive(Accounts)]
pub struct UpdateReserveConfig {
    // The market is identified by `address = reserve.lending_market`; we
    // only need to prove the signer owns it, not re-derive its address.
    #[account(address = reserve.lending_market)]
    pub lending_market: BorshAccount<LendingMarket>,

    #[account(address = lending_market.owner)]
    pub owner: Signer,

    #[account(mut)]
    pub reserve: BorshAccount<Reserve>,
}
