pub mod error;
pub mod instructions;
pub mod state;

use anchor_lang::prelude::*;

pub use instructions::*;
pub use state::*;

declare_id!("SWPR8Rk3aq3DrDGLdaANq7xCMnXoUFUJWJJmCWxc8Jm");

#[program]
pub mod mock_swap_router {
    use super::*;

    pub fn initialize_router(
        context: Context<InitializeRouterAccountConstraints>,
        usdc_mint: Pubkey,
    ) -> Result<()> {
        instructions::initialize_router::handle_initialize_router(context, usdc_mint)
    }

    pub fn set_rate(
        context: Context<SetRateAccountConstraints>,
        mint: Pubkey,
        usdc_per_token: u64,
    ) -> Result<()> {
        instructions::set_rate::handle_set_rate(context, mint, usdc_per_token)
    }

    pub fn swap_usdc_for_asset(
        context: Context<SwapUsdcForAssetAccountConstraints>,
        usdc_amount_in: u64,
        minimum_asset_out: u64,
    ) -> Result<()> {
        instructions::swap_usdc_for_asset::handle_swap_usdc_for_asset(
            context,
            usdc_amount_in,
            minimum_asset_out,
        )
    }

    pub fn swap_asset_for_usdc(
        context: Context<SwapAssetForUsdcAccountConstraints>,
        asset_amount_in: u64,
        minimum_usdc_out: u64,
    ) -> Result<()> {
        instructions::swap_asset_for_usdc::handle_swap_asset_for_usdc(
            context,
            asset_amount_in,
            minimum_usdc_out,
        )
    }
}
