#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

#[cfg(test)]
mod tests;

declare_id!("SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm");

/// A mock constant-rate swap router used by the vault-strategy example. It
/// swaps an approved asset against USDC at an admin-set fixed rate: buying an
/// asset mints it against USDC paid into the treasury; selling burns it and
/// pays USDC out. Stand-in for a real AMM or aggregator so the vault-strategy
/// example is self-contained.
#[program]
mod quasar_mock_swap_router {
    use super::*;

    #[instruction(discriminator = 0)]
    pub fn initialize_router(
        ctx: Ctx<InitializeRouterAccountConstraints>,
    ) -> Result<(), ProgramError> {
        instructions::initialize_router::handle_initialize_router(&mut ctx.accounts, &ctx.bumps)
    }

    #[instruction(discriminator = 1)]
    pub fn set_rate(
        ctx: Ctx<SetRateAccountConstraints>,
        usdc_per_token: u64,
    ) -> Result<(), ProgramError> {
        instructions::set_rate::handle_set_rate(&mut ctx.accounts, usdc_per_token, &ctx.bumps)
    }

    #[instruction(discriminator = 2)]
    pub fn swap_usdc_for_asset(
        ctx: Ctx<SwapUsdcForAssetAccountConstraints>,
        usdc_amount_in: u64,
        minimum_asset_out: u64,
    ) -> Result<(), ProgramError> {
        instructions::swap_usdc_for_asset::handle_swap_usdc_for_asset(
            &mut ctx.accounts,
            usdc_amount_in,
            minimum_asset_out,
            &ctx.bumps,
        )
    }

    #[instruction(discriminator = 3)]
    pub fn swap_asset_for_usdc(
        ctx: Ctx<SwapAssetForUsdcAccountConstraints>,
        asset_amount_in: u64,
        minimum_usdc_out: u64,
    ) -> Result<(), ProgramError> {
        instructions::swap_asset_for_usdc::handle_swap_asset_for_usdc(
            &mut ctx.accounts,
            asset_amount_in,
            minimum_usdc_out,
            &ctx.bumps,
        )
    }
}
