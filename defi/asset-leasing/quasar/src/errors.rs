use quasar_lang::prelude::*;

/// Program-specific errors. Codes start at 6000 (Quasar's default
/// `#[error_code]` offset, matching Anchor), so they never collide with
/// Solana's built-in `ProgramError` codes or the framework's
/// `QuasarError` codes.
#[error_code]
pub enum AssetLeasingError {
    InvalidLeaseStatus,
    InvalidDuration,
    InvalidLeasedAmount,
    InvalidCollateralAmount,
    InvalidLeaseFeePerSecond,
    InvalidMaintenanceMargin,
    InvalidLiquidationBounty,
    LeaseExpired,
    LeaseNotExpired,
    PositionHealthy,
    StalePrice,
    NonPositivePrice,
    MathOverflow,
    Unauthorised,
    LeasedMintEqualsCollateralMint,
    PriceFeedMismatch,
    InvalidStatusByte,
}
