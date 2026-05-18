use anchor_lang::prelude::*;

#[error_code]
pub enum AssetLeasingError {
    #[msg("Lease is not in the required state for this action")]
    InvalidLeaseStatus,
    #[msg("Duration must be greater than zero")]
    InvalidDuration,
    #[msg("Leased amount must be greater than zero")]
    InvalidLeasedAmount,
    #[msg("Required collateral amount must be greater than zero")]
    InvalidCollateralAmount,
    #[msg("Lease fee per second must be greater than zero")]
    InvalidLeaseFeePerSecond,
    #[msg("Maintenance margin is outside the allowed range")]
    InvalidMaintenanceMargin,
    #[msg("Liquidation bounty is outside the allowed range")]
    InvalidLiquidationBounty,
    #[msg("Lease has already expired")]
    LeaseExpired,
    #[msg("Lease has not yet expired")]
    LeaseNotExpired,
    #[msg("Position is healthy; liquidation is not allowed")]
    PositionHealthy,
    #[msg("Pyth price update is stale")]
    StalePrice,
    #[msg("Pyth price is not positive")]
    NonPositivePrice,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Signer is not authorised for this action")]
    Unauthorised,
    #[msg("Leased mint and collateral mint must be different")]
    LeasedMintEqualsCollateralMint,
    #[msg("Price update does not match the feed pinned on this lease")]
    PriceFeedMismatch,
}
