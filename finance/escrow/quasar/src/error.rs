use quasar_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    /// The offer puts zero tokens on one side, so nobody could take it fairly.
    // 6000 is the conventional Anchor-compatible starting offset for
    // program-specific error codes (Quasar's #[error_code] starts at 0
    // unless told otherwise; framework errors occupy 3000+).
    ZeroAmount = 6000,
    /// The offer swaps a token for a different amount of itself.
    SameMint,
}
