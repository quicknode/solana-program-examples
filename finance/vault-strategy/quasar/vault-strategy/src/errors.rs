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
    PricePredatesRestart,
    /// A deployment leg of the deposit would buy none of its asset.
    DepositTooSmall,
    /// A rebalance would sell or spend more than the recorded holdings.
    InsufficientHoldings,
}
