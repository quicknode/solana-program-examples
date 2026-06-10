use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Weights must sum to 10000 basis points")]
    InvalidWeights,
    #[msg("Shares minted are below the minimum - slippage exceeded")]
    SlippageTooHigh,
    #[msg("USDC out is below minimum - slippage exceeded")]
    UsdcSlippage,
    #[msg("Asset A out is below minimum - slippage exceeded")]
    AssetASlippage,
    #[msg("Asset B out is below minimum - slippage exceeded")]
    AssetBSlippage,
    #[msg("Asset mint is neither asset_a nor asset_b")]
    InvalidAssetMint,
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
    #[msg("Price feed account does not match the strategy's registered feed")]
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
