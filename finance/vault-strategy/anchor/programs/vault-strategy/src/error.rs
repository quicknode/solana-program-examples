use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Shares minted are below the minimum - slippage exceeded")]
    SlippageTooHigh,
    #[msg("USDC out is below minimum - slippage exceeded")]
    UsdcSlippage,
    #[msg("Swap output deviates from the oracle price by more than the allowed slippage")]
    SwapSlippageExceeded,
    #[msg("Max slippage exceeds the maximum allowed configuration")]
    SlippageConfigTooHigh,
    #[msg("Asset mint is not part of this strategy")]
    AssetNotFound,
    #[msg("Strategy already holds the maximum number of assets")]
    TooManyAssets,
    #[msg("Asset is already part of this strategy")]
    DuplicateAsset,
    #[msg("Total target weight would exceed 10000 basis points")]
    WeightOverflow,
    #[msg("Strategy weights must sum to 100% before it can accept deposits")]
    StrategyNotFullyAllocated,
    #[msg("Wrong number of asset accounts supplied for the strategy's assets")]
    IncompleteAssetAccounts,
    #[msg("An asset account does not match the strategy's registered asset")]
    InvalidAssetAccount,
    #[msg("Token account could not be read")]
    InvalidVaultAccount,
    #[msg("Recipient token account is not owned by the withdrawing user")]
    InvalidRecipient,
    #[msg("Registry does not match the strategy's registered registry")]
    InvalidRegistry,
    #[msg("No time has elapsed since last fee accrual")]
    NoTimeElapsed,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("Cannot withdraw zero shares")]
    ZeroShares,
    #[msg("Cannot deposit zero USDC")]
    ZeroDeposit,
    #[msg("Total shares are zero - cannot compute proportional withdraw")]
    ZeroTotalShares,
    #[msg("Price feed account does not match the registered feed")]
    InvalidPriceFeed,
    #[msg("Pyth price is zero or negative")]
    NegativePrice,
    #[msg("Pyth price feed is stale")]
    StalePriceFeed,
    #[msg("Sell and buy mints must be different")]
    SameMint,
    #[msg("USDC mint does not match the strategy's registered USDC mint")]
    InvalidUsdcMint,
    #[msg("Swap router program does not match the strategy's registered swap router")]
    InvalidSwapRouter,
    #[msg("Management fee exceeds the maximum allowed")]
    FeeTooHigh,
}
