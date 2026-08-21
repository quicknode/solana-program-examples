use anchor_lang::prelude::*;

#[error_code]
pub enum LendingError {
    #[msg("Arithmetic operation overflowed")]
    MathOverflow,
    #[msg("Reserve config has an invalid value")]
    InvalidConfig,
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Deposit is too small to mint any share tokens")]
    DepositTooSmall,
    #[msg("Reserve does not have enough available liquidity")]
    InsufficientReserveLiquidity,
    #[msg("Reserve must be refreshed in this same transaction before use")]
    ReserveStale,
    #[msg("Obligation must be refreshed in this same transaction before use")]
    ObligationStale,
    #[msg("Price feed has not been updated recently enough")]
    StalePriceFeed,
    #[msg("Price feed is stale: it predates the last cluster restart")]
    PricePredatesRestart,
    #[msg("Price feed reported a non-positive price")]
    InvalidOraclePrice,
    #[msg("Borrow would exceed the obligation's allowed borrow value")]
    BorrowTooLarge,
    #[msg("Withdraw would leave the obligation undercollateralized")]
    WithdrawTooLarge,
    #[msg("Obligation is healthy and cannot be liquidated")]
    ObligationHealthy,
    #[msg("Obligation already uses the maximum number of reserves")]
    TooManyReserves,
    #[msg("Reserve is not part of this obligation")]
    ReserveNotFound,
    #[msg("A refresh account did not match the obligation's stored reserves")]
    InvalidObligationAccount,
    #[msg("Reserve belongs to a different lending market than the obligation")]
    MarketMismatch,
    #[msg("Repay amount would seize more collateral than the obligation holds")]
    LiquidationTooLarge,
    #[msg("No protocol fees are available to collect")]
    NothingToCollect,
}
