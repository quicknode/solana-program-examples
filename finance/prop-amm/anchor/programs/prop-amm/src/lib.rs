use anchor_lang::prelude::*;

mod constants;
mod errors;
// Public so the LiteSVM integration tests can build instruction arguments
// (`MarketParameters`, `Direction`) against the program's own types.
pub mod instructions;
pub mod quote_math;
pub mod state;

use instructions::*;
use state::Direction;

declare_id!("9ZMtJFtn5n4wwpEeXXG5paFQakcDtrd3ova5ptJL4VT1");

/// An oracle-quoted proprietary AMM ("prop AMM").
///
/// One operator — a market-making firm — funds a market's inventory with its
/// own capital and quotes both sides of it: anyone may buy the base token at
/// the oracle price plus a spread, or sell it at the oracle price minus the
/// spread. There is no pricing curve, there are no liquidity providers, and
/// there are no shares: the operator is the only capital in the pool, which is
/// the property that gives the design its name. This is the architecture
/// behind venues like Lifinity, SolFi, and HumidiFi, which fill most Solana
/// swaps via aggregator routing.
#[program]
pub mod prop_amm {
    use super::*;

    /// Create a market for one base/quote pair, priced by one oracle feed.
    /// The signer becomes the market's operator: the only party allowed to
    /// move inventory or change the quote.
    pub fn initialize_market(
        context: &mut Context<InitializeMarketAccountConstraints>,
        parameters: MarketParameters,
    ) -> Result<()> {
        instructions::handle_initialize_market(context, parameters)
    }

    /// Operator moves inventory into the market's vaults. Either amount may be
    /// zero, but not both.
    pub fn deposit_inventory(
        context: &mut Context<DepositInventoryAccountConstraints>,
        base_amount: u64,
        quote_amount: u64,
    ) -> Result<()> {
        instructions::handle_deposit_inventory(context, base_amount, quote_amount)
    }

    /// Operator withdraws inventory from the market's vaults — up to all of
    /// it, at any time. The capital is the operator's own; nobody else has a
    /// claim on it.
    pub fn withdraw_inventory(
        context: &mut Context<WithdrawInventoryAccountConstraints>,
        base_amount: u64,
        quote_amount: u64,
    ) -> Result<()> {
        instructions::handle_withdraw_inventory(context, base_amount, quote_amount)
    }

    /// Operator re-quotes the market: a new spread, or a pause. Pulling quotes
    /// during volatility is not an emergency measure for a market maker; it is
    /// Tuesday.
    pub fn set_quote(
        context: &mut Context<SetQuoteAccountConstraints>,
        spread_bps: u16,
        paused: bool,
    ) -> Result<()> {
        instructions::handle_set_quote(context, spread_bps, paused)
    }

    /// Swap against the operator's quote: buy the base token at oracle plus
    /// spread, or sell it at oracle minus spread. Permissionless.
    /// `minimum_amount_out` is slippage protection; pass `0` to opt out.
    pub fn swap(
        context: &mut Context<SwapAccountConstraints>,
        direction: Direction,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<()> {
        instructions::handle_swap(context, direction, amount_in, minimum_amount_out)
    }
}
