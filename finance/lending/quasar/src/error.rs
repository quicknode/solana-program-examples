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
}
