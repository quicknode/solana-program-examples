use quasar_lang::prelude::*;

/// Program errors. Codes start at 6000, the Anchor-compatible offset, so the
/// two builds report the same numbers (Quasar's `#[error_code]` starts at 0
/// unless told otherwise; framework errors occupy 3000+).
#[error_code]
pub enum OptionsError {
    /// Market or option parameter is outside the allowed range.
    InvalidParameter = 6000,
    /// Option expiry must be in the future.
    ExpiryInPast,
    /// Arithmetic overflow.
    MathOverflow,
    /// Option is not listed for sale: it has been sold or exercised.
    OptionNotListed,
    /// Option has expired: it can no longer be bought or exercised.
    OptionExpired,
    /// Option has no holder: it is unsold or already exercised.
    OptionNotHeld,
    /// Option has not expired: the holder may still exercise it.
    OptionNotExpired,
    /// Option has not been exercised: there are no proceeds to collect.
    OptionNotExercised,
    /// No fees are available to collect.
    NothingToCollect,
    /// Vault balance would fall below what the market owes.
    CustodyInvariantViolated,
}
