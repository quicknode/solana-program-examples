use anchor_lang::prelude::*;

mod constants;
mod errors;
mod last_restart;
// Public so the LiteSVM integration tests can build instruction arguments
// (`PoolParameters`, `Side`) against the program's own types.
pub mod instructions;
pub mod state;

use instructions::*;
use state::Side;

declare_id!("3uCm8Jep469pHUpYQCh6eA6dpYV3ogvTvaRDZBPtw5So");

#[program]
pub mod perpetual_futures {
    use super::*;

    /// Create a perpetual-futures pool for one collateral token, priced by one
    /// oracle feed. Sets the trading parameters and creates the custody vault
    /// and liquidity-provider mint.
    pub fn initialize_pool(
        context: &mut Context<InitializePoolAccountConstraints>,
        parameters: PoolParameters,
    ) -> Result<()> {
        instructions::handle_initialize_pool(context, parameters)
    }

    /// Deposit collateral into the pool and receive liquidity-provider shares.
    /// `minimum_shares_out` is slippage protection; pass `0` to opt out.
    pub fn add_liquidity(
        context: &mut Context<AddLiquidityAccountConstraints>,
        amount: u64,
        minimum_shares_out: u64,
    ) -> Result<()> {
        instructions::handle_add_liquidity(context, amount, minimum_shares_out)
    }

    /// Burn liquidity-provider shares and withdraw the matching collateral.
    /// `minimum_amount_out` is slippage protection; pass `0` to opt out.
    pub fn remove_liquidity(
        context: &mut Context<RemoveLiquidityAccountConstraints>,
        shares: u64,
        minimum_amount_out: u64,
    ) -> Result<()> {
        instructions::handle_remove_liquidity(context, shares, minimum_amount_out)
    }

    /// Open a leveraged long or short position against the pool at the current
    /// oracle price. `acceptable_price` bounds the fill (longs reject above it,
    /// shorts reject below it); pass `0` to opt out.
    pub fn open_position(
        context: &mut Context<OpenPositionAccountConstraints>,
        side: Side,
        collateral_amount: u64,
        size: u64,
        acceptable_price: u64,
    ) -> Result<()> {
        instructions::handle_open_position(context, side, collateral_amount, size, acceptable_price)
    }

    /// Close the caller's own position, settling profit/loss, accrued funding,
    /// and the close fee. `minimum_payout` is slippage protection; pass `0` to
    /// opt out.
    pub fn close_position(
        context: &mut Context<ClosePositionAccountConstraints>,
        minimum_payout: u64,
    ) -> Result<()> {
        instructions::handle_close_position(context, minimum_payout)
    }

    /// Permissionlessly close a position whose equity has fallen to or below
    /// the maintenance margin. The caller earns the liquidation fee.
    pub fn liquidate_position(
        context: &mut Context<LiquidatePositionAccountConstraints>,
    ) -> Result<()> {
        instructions::handle_liquidate_position(context)
    }

    /// Pool authority sweeps the accumulated protocol fees from the vault.
    pub fn collect_fees(context: &mut Context<CollectFeesAccountConstraints>) -> Result<()> {
        instructions::handle_collect_fees(context)
    }

    /// Pool authority retunes the per-slot funding rate, accruing at the old
    /// rate first.
    pub fn set_funding_rate(
        context: Context<SetFundingRateAccountConstraints>,
        funding_rate_per_slot: u64,
    ) -> Result<()> {
        instructions::handle_set_funding_rate(context, funding_rate_per_slot)
    }
}
