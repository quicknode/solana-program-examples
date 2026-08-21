use anchor_lang::prelude::*;

/// One perpetual-futures market: a single collateral token priced by a single
/// oracle feed. Liquidity providers fund the pool and are the counterparty to
/// every trader; the pool absorbs trader profit and loss.
///
/// Money fields are raw base units of the collateral token. The pool never
/// assumes decimals — `transfer_checked` carries them through every CPI.
#[account]
#[derive(InitSpace)]
pub struct Pool {
    /// Admin: configures the pool and sweeps protocol fees. Not a custody
    /// escape hatch — it cannot touch liquidity-provider or trader funds.
    pub authority: Pubkey,

    pub collateral_mint: Pubkey,

    /// Oracle feed this market reads its price from. Stored so handlers can
    /// reject any substituted feed account.
    pub oracle_feed: Pubkey,

    /// Decimal places the oracle price is quoted in. Pinned at creation so a
    /// feed that silently changes scale is rejected rather than mis-read.
    pub oracle_scale: u32,

    pub custody_vault: Pubkey,

    pub lp_mint: Pubkey,

    /// Liquidity-provider-owned assets, in collateral base units. Grows with
    /// deposits, trader losses, fees-to-LPs; shrinks with withdrawals and
    /// trader profits. Trader collateral is tracked separately in
    /// `total_collateral` and is not part of this figure.
    pub liquidity: u64,

    /// Portion of `liquidity` reserved to cover open positions' maximum
    /// recoverable profit (one notional `size` per position). Liquidity-provider
    /// withdrawals can only take the free remainder (`liquidity - reserved`), so
    /// a winning trader can always be paid. Also caps total exposure: a position
    /// can only open while `reserved + size <= liquidity`.
    pub reserved_liquidity: u64,

    /// Sum of every open position's posted collateral, held in the same vault.
    pub total_collateral: u64,

    /// Protocol fees accrued from open/close fees, awaiting `collect_fees`.
    pub protocol_fees: u64,

    /// Aggregate long open interest (sum of position `size`), in collateral
    /// base units of notional.
    pub long_size: u128,

    pub short_size: u128,

    /// Running sum of `size * SIZE_PRECISION / entry_price` for each side.
    /// Lets mark-to-market assets-under-management be derived from the current
    /// price without iterating positions: aggregate long profit/loss equals
    /// `price * long_size_scaled / SIZE_PRECISION - long_size`.
    pub long_size_scaled: u128,

    pub short_size_scaled: u128,

    /// Cumulative funding index, scaled by `FUNDING_PRECISION`. Rises while
    /// longs are the heavier side (longs pay), falls while shorts are heavier.
    /// A position pays funding proportional to the change in this index between
    /// open and close.
    pub cumulative_funding: i128,

    pub last_funding_slot: u64,

    /// Funding accrued per slot, in `FUNDING_PRECISION` units, applied to the
    /// heavier side. The funding paid by traders accrues to the pool.
    pub funding_rate_per_slot: u64,

    /// Fee charged on notional when opening a position, in basis points.
    pub open_fee_bps: u16,

    pub close_fee_bps: u16,

    /// Highest leverage a position may open at (`size <= collateral * max`).
    pub max_leverage: u16,

    /// Equity threshold, in basis points of notional, below which a position is
    /// liquidatable.
    pub maintenance_margin_bps: u16,

    /// Reward paid to a liquidator, in basis points of the liquidated notional.
    pub liquidation_fee_bps: u16,

    /// Maximum oracle confidence band, in basis points of the price, that the
    /// pool will trade against. A wider band is rejected as untrustworthy.
    pub max_confidence_bps: u16,

    pub bump: u8,

    /// Bump for the vault/LP-mint authority PDA, stored so CPIs can sign without
    /// re-deriving it.
    pub authority_bump: u8,
}
