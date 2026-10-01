use anchor_lang::prelude::*;

/// One perpetual-futures market: a single collateral token priced by a single
/// oracle feed. Liquidity providers fund the pool and are the counterparty to
/// every trader; the pool absorbs trader profit and loss.
///
/// Money fields are raw base units of the collateral token. The pool never
/// assumes decimals — `transfer_checked` carries them through every CPI.
#[account(borsh)]
#[derive(InitSpace)]
pub struct Pool {
    /// Admin: configures the pool and sweeps program fees. Not a custody
    /// escape hatch — it cannot touch liquidity-provider or trader funds.
    pub authority: Address,

    pub collateral_mint: Address,

    /// Oracle feed this market reads its price from. Stored so handlers can
    /// reject any substituted feed account.
    pub oracle_feed: Address,

    /// Decimal places the oracle price is quoted in. Pinned at creation so a
    /// feed that silently changes scale is rejected rather than mis-read.
    pub oracle_scale: u32,

    pub custody_vault: Address,

    pub lp_mint: Address,

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

    /// Program fees accrued from open/close fees, awaiting `collect_fees`.
    pub program_fees: u64,

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

    /// The Clock's `unix_timestamp` when funding last accrued. Funding runs on
    /// the wall clock, so what a position costs per hour does not depend on
    /// the cluster's slot time.
    pub last_funding_timestamp: i64,

    /// Time-weighted moving average of the oracle price, in the pool's
    /// `oracle_scale` fixed point. Seeded with the oracle price when the pool is
    /// created. Every handler that reads the oracle credits the seconds since
    /// the previous read to `last_oracle_price`, the price that read saw.
    /// Trading and liquidity handlers refuse an oracle price more than
    /// `max_price_deviation_bps` away from it, so a sudden jump pauses them
    /// until the average catches up.
    pub average_price: u64,

    /// The oracle price at the most recent read, in `oracle_scale` fixed point.
    /// The next read folds it into `average_price` for the seconds in between.
    pub last_oracle_price: u64,

    /// The Clock's `unix_timestamp` of the most recent fold into
    /// `average_price`.
    pub average_price_timestamp: i64,

    /// Funding accrued per second, in `FUNDING_PRECISION` units, applied to the
    /// heavier side. The funding paid by traders accrues to the pool.
    pub funding_rate_per_second: u64,

    /// Fee charged on notional when opening a position, in basis points.
    pub open_fee_bps: u16,

    pub close_fee_bps: u16,

    /// Net collateral a position must post to open, in basis points of its
    /// notional size: 1_000 allows at most 10x leverage. Always above
    /// `maintenance_margin_bps`, so no position opens already liquidatable.
    pub initial_margin_bps: u16,

    /// Equity threshold, in basis points of notional, at or below which a
    /// position is liquidatable.
    pub maintenance_margin_bps: u16,

    /// Reward paid to a liquidator, in basis points of the liquidated notional.
    pub liquidation_fee_bps: u16,

    /// Maximum oracle confidence band, in basis points of the price, that the
    /// pool will trade against. A wider band is rejected as untrustworthy.
    pub max_confidence_bps: u16,

    /// Widest gap the pool trades across between the oracle price and
    /// `average_price`, in basis points of `average_price`.
    pub max_price_deviation_bps: u16,

    /// Bump of this account's own address. The pool owns the custody vault
    /// and is the LP mint's authority, so it signs vault transfers and
    /// mint/burn CPIs with `[POOL_SEED, collateral_mint, oracle_feed, bump]`;
    /// there is no separate signing PDA.
    pub bump: u8,
}
