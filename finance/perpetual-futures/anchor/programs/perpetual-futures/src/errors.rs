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

    #[msg("Requested leverage exceeds the pool maximum")]
    LeverageTooHigh,

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

    #[msg("Pool does not have enough free liquidity to satisfy this request")]
    InsufficientLiquidity,

    #[msg("Posted collateral does not cover the open fee")]
    InsufficientCollateral,

    #[msg("Pool is insolvent: liabilities exceed assets")]
    PoolInsolvent,

    #[msg("Position is still healthy and cannot be liquidated")]
    PositionHealthy,

    #[msg("Position equity is below maintenance margin; it must be liquidated, not closed")]
    PositionNotHealthy,

    #[msg("Profit has not matured yet; wait out the warm-up period before closing in profit")]
    ProfitNotMatured,

    #[msg("No protocol fees are available to collect")]
    NothingToClaim,
}
