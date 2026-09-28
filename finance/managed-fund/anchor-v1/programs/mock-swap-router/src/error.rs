use anchor_lang::prelude::*;

#[error_code]
pub enum RouterError {
    #[msg("Rate is zero - cannot compute swap")]
    ZeroRate,
    #[msg("Output is below minimum - slippage exceeded")]
    SlippageExceeded,
    #[msg("Asset mint does not match rate record")]
    InvalidAssetMint,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("USDC mint does not match router config")]
    WrongUsdcMint,
    #[msg("Asset amount in must be greater than zero")]
    ZeroAmount,
}
