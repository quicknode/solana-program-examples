use quasar_lang::prelude::*;

/// Program errors. Codes start at 6000 to match Anchor's custom-error base.
#[error_code]
pub enum RouterError {
    ZeroRate = 6000,
    SlippageExceeded,
    InvalidAssetMint,
    MathOverflow,
    WrongUsdcMint,
    ZeroAmount,
}
