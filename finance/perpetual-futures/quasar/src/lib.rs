#![cfg_attr(not(test), no_std)]

//! Quasar port of the perpetual-futures example. The design, math, and
//! behaviour match the Anchor sibling at `finance/perpetual-futures/anchor`; see
//! its README for the full walkthrough. This file wires up the program; the
//! per-instruction logic lives in `instructions/`.

use quasar_lang::prelude::*;

mod constants;
mod instructions;
pub mod state;
#[cfg(test)]
mod tests;

use instructions::*;

declare_id!("GaxH8967GVLxtst2SHCtXxqKQqGxgHyxqYvr9WGe1fmC");

/// Authority PDA at seeds = [b"authority", pool]. Signs vault and mint CPIs.
#[derive(Seeds)]
#[seeds(b"authority", pool: Address)]
pub struct PoolAuthorityPda;

/// Liquidity-provider mint PDA at seeds = [b"lp_mint", pool].
#[derive(Seeds)]
#[seeds(b"lp_mint", pool: Address)]
pub struct LpMintPda;

/// Collateral custody vault PDA at seeds = [b"vault", pool].
#[derive(Seeds)]
#[seeds(b"vault", pool: Address)]
pub struct VaultPda;

#[program]
mod quasar_perpetual_futures {
    use super::*;

    #[instruction(discriminator = 0)]
    #[allow(clippy::too_many_arguments)]
    pub fn initialize_pool(
        ctx: Ctx<InitializePool>,
        oracle_scale: u32,
        funding_rate_per_slot: u64,
        open_fee_bps: u16,
        close_fee_bps: u16,
        max_leverage: u16,
        maintenance_margin_bps: u16,
        liquidation_fee_bps: u16,
        max_confidence_bps: u16,
    ) -> Result<(), ProgramError> {
        instructions::handle_initialize_pool(
            &mut ctx.accounts,
            oracle_scale,
            funding_rate_per_slot,
            open_fee_bps,
            close_fee_bps,
            max_leverage,
            maintenance_margin_bps,
            liquidation_fee_bps,
            max_confidence_bps,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 1)]
    pub fn add_liquidity(
        ctx: Ctx<AddLiquidity>,
        amount: u64,
        minimum_shares_out: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_add_liquidity(
            &mut ctx.accounts,
            amount,
            minimum_shares_out,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 2)]
    pub fn remove_liquidity(
        ctx: Ctx<RemoveLiquidity>,
        shares: u64,
        minimum_amount_out: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_remove_liquidity(
            &mut ctx.accounts,
            shares,
            minimum_amount_out,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 3)]
    pub fn open_position(
        ctx: Ctx<OpenPosition>,
        side: u8,
        collateral_amount: u64,
        size: u64,
        acceptable_price: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_open_position(
            &mut ctx.accounts,
            side,
            collateral_amount,
            size,
            acceptable_price,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 4)]
    pub fn close_position(
        ctx: Ctx<ClosePosition>,
        minimum_payout: u64,
    ) -> Result<(), ProgramError> {
        instructions::handle_close_position(&mut ctx.accounts, minimum_payout, &ctx.bumps)
    }

    #[instruction(discriminator = 5)]
    pub fn liquidate_position(ctx: Ctx<LiquidatePosition>) -> Result<(), ProgramError> {
        instructions::handle_liquidate_position(&mut ctx.accounts, &ctx.bumps)
    }

    #[instruction(discriminator = 6)]
    pub fn collect_fees(ctx: Ctx<CollectFees>) -> Result<(), ProgramError> {
        instructions::handle_collect_fees(&mut ctx.accounts, &ctx.bumps)
    }
}
