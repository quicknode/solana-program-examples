use quasar_lang::prelude::*;

/// Program errors. Codes start at 6000 to match Anchor's custom-error base.
#[error_code]
pub enum FundError {
    SlippageTooHigh = 6000,
    UsdcSlippage,
    SwapSlippageExceeded,
    SlippageConfigTooHigh,
    AssetNotFound,
    TooManyAssets,
    DuplicateAsset,
    WeightOverflow,
    FundNotFullyAllocated,
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
    /// Rebalance threshold is outside the allowed range.
    RebalanceThresholdOutOfRange,
    /// The asset to sell is not far enough above its target weight to rebalance.
    DriftBelowThreshold,
    /// The asset to buy is not below its target weight.
    NotUnderweight,
    /// The Pyth price's confidence interval is too wide to trust.
    OracleConfidenceTooWide,
    /// The Pyth price update is not fully verified by a quorum of Pyth's signers.
    PriceNotFullyVerified,
}
