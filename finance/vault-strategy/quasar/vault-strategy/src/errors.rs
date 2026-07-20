use quasar_lang::prelude::*;

/// Program errors. Codes start at 6000 to match Anchor's custom-error base.
#[error_code]
pub enum VaultError {
    SlippageTooHigh = 6000,
    UsdcSlippage,
    SwapSlippageExceeded,
    SlippageConfigTooHigh,
    AssetNotFound,
    TooManyAssets,
    DuplicateAsset,
    WeightOverflow,
    StrategyNotFullyAllocated,
    IncompleteAssetAccounts,
    InvalidAssetAccount,
    InvalidVaultAccount,
    InvalidRecipient,
    InvalidRegistry,
    NoTimeElapsed,
    MathOverflow,
    ZeroShares,
    ZeroDeposit,
    ZeroTotalShares,
    InvalidPriceFeed,
    NegativePrice,
    StalePriceFeed,
    SameMint,
    InvalidUsdcMint,
    InvalidSwapRouter,
    FeeTooHigh,
}
