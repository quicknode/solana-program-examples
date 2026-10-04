use quasar_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    /// The offer puts zero tokens on one side, so nobody could take it fairly.
    // 6000 is the conventional Anchor-compatible starting offset for
    // program-specific error codes (Quasar's #[error_code] starts at 0
    // unless told otherwise; framework errors occupy 3000+).
    ZeroAmount = 6000,
    /// The offer pays less token A, or wants more token B, than the taker
    /// agreed to: the maker re-made the offer at worse terms after the taker
    /// signed.
    OfferTermsChanged,
}
