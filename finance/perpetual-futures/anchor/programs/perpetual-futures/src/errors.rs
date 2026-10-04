use anchor_lang::prelude::*;

#[error_code]
pub enum PerpError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,

    #[msg("First deposit must exceed the locked minimum liquidity")]
    DepositTooSmall,

    #[msg("Computed share or token amount rounded down to zero")]
    AmountRoundsToZero,

    #[msg("Arithmetic overflow")]
    MathOverflow,

    #[msg("Position is too large for its collateral: net collateral is below the pool's initial margin")]
    InitialMarginNotMet,

    #[msg("Pool parameter is outside the allowed range")]
    InvalidParameter,

    #[msg("Oracle price has not been updated recently enough")]
    StalePrice,

    #[msg("Oracle price must be positive")]
    NonPositivePrice,

    #[msg("Oracle feed scale does not match the pool configuration")]
    OracleScaleMismatch,

    #[msg("Oracle feed account data is too short to decode")]
    OracleDataTooShort,

    #[msg("Oracle price confidence band is too wide to trust")]
    OracleConfidenceTooWide,

    #[msg("Fill price is worse than the caller's acceptable price")]
    SlippageExceeded,

    #[msg("Withdrawal is larger than the pool's liquidity: part of the shares' value is still in open positions")]
    InsufficientLiquidity,

    #[msg("Posted collateral does not cover the open fee")]
    InsufficientCollateral,

    #[msg("Pool is insolvent: liabilities exceed assets")]
    PoolInsolvent,

    #[msg("Position is still healthy and cannot be liquidated")]
    PositionHealthy,

    #[msg("Position equity is below maintenance margin; it must be liquidated, not closed")]
    PositionNotHealthy,

    #[msg("No program fees are available to collect")]
    NothingToClaim,

    #[msg("Oracle price is stale: it predates the last cluster restart")]
    PricePredatesRestart,

    #[msg("Initial margin is at or below the maintenance margin: positions could open already liquidatable")]
    InitialMarginNotAboveMaintenance,

    #[msg("Maximum price deviation is outside the allowed range: it must be above zero and below 10,000 basis points")]
    InvalidPriceDeviation,

    #[msg("Oracle price is too far from the pool's average price: trading pauses until the average catches up")]
    PriceOutsideBand,

    #[msg(
        "Profit cannot be taken yet: the position has not been open for the pool's profit warm-up"
    )]
    ProfitNotMatured,

    #[msg("Price feed is not owned by the oracle program the pool recorded")]
    PriceFeedNotFromOracle,
}
