use quasar_lang::prelude::*;

#[error_code]
pub enum FundraiserError {
    /// The target amount has not been raised, so the maker cannot withdraw.
    // 6000 is the conventional Anchor-compatible starting offset for
    // program-specific error codes (Quasar's #[error_code] starts at 0
    // unless told otherwise; framework errors occupy 3000+).
    TargetNotMet = 6000,
    /// The target amount was raised, so contributors cannot claim refunds.
    TargetMet,
    /// The fundraising window has closed, so contributions are rejected.
    FundraiserEnded,
    /// The fundraising window is still open, so refunds are rejected.
    FundraiserNotEnded,
    /// An amount argument was zero or otherwise unusable.
    InvalidAmount,
    /// A duration argument was zero, which would create a fundraiser that
    /// could never accept contributions.
    InvalidDuration,
    /// Checked arithmetic overflowed or underflowed.
    MathOverflow,
    /// A token balance after a transfer did not match the expected value.
    BalanceMismatch,
}
