use anchor_lang::prelude::*;

#[error_code]
pub enum PropAmmError {
    #[msg("Market parameter is outside the allowed range")]
    InvalidParameter,

    #[msg("Amount must be greater than zero")]
    ZeroAmount,

    #[msg("Computed output amount rounded down to zero")]
    AmountRoundsToZero,

    #[msg("Arithmetic overflow")]
    MathOverflow,

    #[msg("The operator has paused this market's quotes")]
    MarketPaused,

    #[msg("Output is below the caller's minimum")]
    SlippageExceeded,

    #[msg("The market does not hold enough inventory to fill this swap")]
    InsufficientInventory,

    #[msg("Swap output exceeds the input's value at the oracle price")]
    InvariantViolated,

    #[msg("Oracle price has not been updated recently enough")]
    StalePrice,

    #[msg("Oracle price must be positive")]
    NonPositivePrice,

    #[msg("Oracle feed scale does not match the market configuration")]
    OracleScaleMismatch,

    #[msg("Oracle feed account data is too short to decode")]
    OracleDataTooShort,

    #[msg("Oracle price confidence band is too wide to trust")]
    OracleConfidenceTooWide,
}
