#![cfg_attr(not(test), no_std)]

//! Quasar port of the options example. The design, math, and behavior match
//! the Anchor sibling at `finance/options/anchor`; see its README for the
//! full walkthrough. This file wires up the program; the per-instruction
//! logic lives in `instructions/`.

use quasar_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;
#[cfg(test)]
mod tests;

use instructions::*;

declare_id!("2gmMGMmipfYypLxWsvQ5GQJT5AGnMWmk4Rb9vQMRo6ig");

#[program]
mod quasar_options {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn initialize_market(
        ctx: Ctx<InitializeMarketAccountConstraints>,
        fee_bps: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize_market(&mut ctx.accounts, fee_bps, &ctx.bumps)
    }

    #[instruction(discriminator = 1)]
    pub fn write_option(
        ctx: Ctx<WriteOptionAccountConstraints>,
        id: u64,
        kind: u8,
        contracts: u64,
        underlying_per_contract: u64,
        strike_per_contract: u64,
        premium: u64,
        expiry: i64,
    ) -> Result<(), ProgramError> {
        instructions::handle_write_option(
            &mut ctx.accounts,
            WriteOptionArguments {
                id,
                kind,
                contracts,
                underlying_per_contract,
                strike_per_contract,
                premium,
                expiry,
            },
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 2)]
    pub fn buy_option(ctx: Ctx<BuyOptionAccountConstraints>) -> Result<(), ProgramError> {
        instructions::handle_buy_option(&mut ctx.accounts)
    }

    #[instruction(discriminator = 3)]
    pub fn cancel_option(ctx: Ctx<CancelOptionAccountConstraints>) -> Result<(), ProgramError> {
        instructions::handle_cancel_option(&mut ctx.accounts)
    }

    #[instruction(discriminator = 4)]
    pub fn exercise_option(ctx: Ctx<ExerciseOptionAccountConstraints>) -> Result<(), ProgramError> {
        instructions::handle_exercise_option(&mut ctx.accounts)
    }

    #[instruction(discriminator = 5)]
    pub fn collect_proceeds(
        ctx: Ctx<CollectProceedsAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_collect_proceeds(&mut ctx.accounts)
    }

    #[instruction(discriminator = 6)]
    pub fn reclaim_collateral(
        ctx: Ctx<ReclaimCollateralAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::handle_reclaim_collateral(&mut ctx.accounts)
    }

    #[instruction(discriminator = 7)]
    pub fn collect_fees(ctx: Ctx<CollectFeesAccountConstraints>) -> Result<(), ProgramError> {
        instructions::handle_collect_fees(&mut ctx.accounts)
    }
}
