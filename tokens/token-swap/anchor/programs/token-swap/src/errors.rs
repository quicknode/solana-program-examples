use anchor_lang::prelude::*;

#[error_code]
pub enum TutorialError {
    #[msg("Invalid fee value")]
    InvalidFee,

    #[msg("Depositing too little liquidity")]
    DepositTooSmall,

    #[msg("Output is below the minimum expected")]
    OutputTooSmall,

    #[msg("Invariant does not hold")]
    InvariantViolated,

    // Returned when a caller asks to deposit or swap more tokens than they hold.
    // Previously the program silently clamped to the available balance, which broke
    // slippage protection for callers (they expected their input to be the actual
    // amount used). We now fail fast so callers can react.
    #[msg("Requested amount exceeds available balance")]
    InsufficientBalance,
}
