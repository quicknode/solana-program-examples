use anchor_lang::prelude::*;

#[error_code]
pub enum StopLossError {
    #[msg("Oracle reported a price above the stop-loss threshold; no conversion needed.")]
    PriceAboveThreshold,

    #[msg("Oracle feed account is shorter than expected; refusing to read.")]
    FeedDataTooShort,

    #[msg("Oracle reported a non-positive price.")]
    NonPositivePrice,

    #[msg("Oracle price update is older than the maximum accepted staleness.")]
    StalePrice,

    #[msg("Vault has not been triggered yet; stables are not available to withdraw.")]
    VaultNotTriggered,

    #[msg("Vault has already triggered; cannot deposit, re-arm, or change threshold.")]
    VaultAlreadyTriggered,

    #[msg("Vault holds no volatile balance to convert.")]
    EmptyVault,

    #[msg("Math overflow.")]
    MathOverflow,

    #[msg("Caller is not the vault owner.")]
    Unauthorized,
}
