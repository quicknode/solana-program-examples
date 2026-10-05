use quasar_lang::prelude::*;

/// Program errors. `#[error_code]` assigns codes starting at 6000 and generates
/// the `From<LendingError> for ProgramError` conversion that `?` and `require!` use.
#[error_code]
pub enum LendingError {
    MathOverflow = 6000,
    InvalidConfig,
    ZeroAmount,
    DepositTooSmall,
    InsufficientLiquidity,
    StalePrice,
    InvalidOraclePrice,
    BorrowTooLarge,
    WithdrawTooLarge,
    ObligationHealthy,
    WrongReserve,
    LiquidationTooLarge,
    NothingToCollect,
    PricePredatesRestart,
    /// The price feed is too uncertain to value against: its confidence band
    /// exceeds the reserve's limit.
    OracleConfidenceTooWide,
    /// The obligation still holds collateral or debt and cannot be closed.
    ObligationNotEmpty,
}
