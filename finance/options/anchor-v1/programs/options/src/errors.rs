use anchor_lang::prelude::*;

#[error_code]
pub enum OptionsError {
    #[msg("Market or option parameter is outside the allowed range")]
    InvalidParameter,

    #[msg("Option expiry must be in the future")]
    ExpiryInPast,

    #[msg("Arithmetic overflow")]
    MathOverflow,

    #[msg("Option is not listed for sale: it has been sold or exercised")]
    OptionNotListed,

    #[msg("Option has expired: it can no longer be bought or exercised")]
    OptionExpired,

    #[msg("Option has no holder: it is unsold or already exercised")]
    OptionNotHeld,

    #[msg("Option has not expired: the holder may still exercise it")]
    OptionNotExpired,

    #[msg("Option has not been exercised: there are no proceeds to collect")]
    OptionNotExercised,

    #[msg("No fees are available to collect")]
    NothingToCollect,

    #[msg("Vault balance would fall below what the market owes")]
    CustodyInvariantViolated,
}
