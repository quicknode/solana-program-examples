#![cfg_attr(not(test), no_std)]
// Quasar's `#[account]` / `#[derive(Accounts)]` macros drive account validation
// and CPIs from struct fields that handler code never reads directly, which
// rustc flags as dead code. The shipped Quasar examples allow it crate-wide.
#![allow(dead_code)]

//! A Kamino/Solend-style borrow/lend program, ported to Quasar.
//!
//! Quasar accounts are fixed-size and zero-copy, so this port models an isolated
//! single-collateral, single-borrow position per obligation (mirroring how the
//! shipped Quasar `escrow`/`vault` examples use fixed-size accounts), and accrues
//! interest inline rather than via a separate `refresh` instruction. It keeps
//! every core lending technique: share-token deposits, a kinked-curve interest
//! index, oracle-priced health, and close-factor liquidation with a bonus.

use quasar_lang::prelude::*;

mod constants;
mod error;
mod instructions;
mod last_restart;
mod logic;
mod math;
mod state;

#[cfg(test)]
mod tests;

use instructions::*;

declare_id!("RDZr26xXfPx8wqQfxcvJLWccp5ep7jQpnxcbCWPiPQq");

#[program]
mod quasar_lending {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn initialize_lending_market(
        ctx: Ctx<InitializeLendingMarket>,
        market_id: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(market_id, &ctx.bumps)
    }

    #[instruction(discriminator = 1)]
    #[allow(clippy::too_many_arguments)]
    pub fn initialize_reserve(
        ctx: Ctx<InitializeReserve>,
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
        ctx.accounts.run(
            loan_to_value_bps,
            liquidation_threshold_bps,
            liquidation_bonus_bps,
            close_factor_bps,
            reserve_factor_bps,
            optimal_utilization_bps,
            min_borrow_rate_bps,
            optimal_borrow_rate_bps,
            max_borrow_rate_bps,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 2)]
    pub fn set_price(
        ctx: Ctx<SetPrice>,
        price_mantissa: i128,
        exponent: i32,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(price_mantissa, exponent, &ctx.bumps)
    }

    #[instruction(discriminator = 3)]
    pub fn deposit_reserve_liquidity(
        ctx: Ctx<DepositReserveLiquidity>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(amount)
    }

    #[instruction(discriminator = 4)]
    pub fn redeem_reserve_collateral(
        ctx: Ctx<RedeemReserveCollateral>,
        shares: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(shares)
    }

    #[instruction(discriminator = 5)]
    pub fn initialize_obligation(ctx: Ctx<InitializeObligation>) -> Result<(), ProgramError> {
        ctx.accounts.run(&ctx.bumps)
    }

    #[instruction(discriminator = 6)]
    pub fn deposit_obligation_collateral(
        ctx: Ctx<DepositObligationCollateral>,
        shares: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(shares)
    }

    #[instruction(discriminator = 7)]
    pub fn withdraw_obligation_collateral(
        ctx: Ctx<WithdrawObligationCollateral>,
        shares: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(shares)
    }

    #[instruction(discriminator = 8)]
    pub fn borrow_obligation_liquidity(
        ctx: Ctx<BorrowObligationLiquidity>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(amount)
    }

    #[instruction(discriminator = 9)]
    pub fn repay_obligation_liquidity(
        ctx: Ctx<RepayObligationLiquidity>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(amount)
    }

    #[instruction(discriminator = 10)]
    pub fn liquidate_obligation(
        ctx: Ctx<LiquidateObligation>,
        amount: u64,
    ) -> Result<(), ProgramError> {
        ctx.accounts.run(amount)
    }

    #[instruction(discriminator = 11)]
    pub fn collect_protocol_fees(ctx: Ctx<CollectProtocolFees>) -> Result<(), ProgramError> {
        ctx.accounts.run()
    }
}
