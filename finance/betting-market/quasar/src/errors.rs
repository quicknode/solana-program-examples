use quasar_lang::prelude::*;

/// Program errors. `#[error_code]` assigns the numeric codes (starting at 6000,
/// matching Anchor's base) and generates the `From<BettingError> for
/// ProgramError` conversion that `?` and `require!` use.
#[error_code]
pub enum BettingError {
    FeeTooHigh = 6000,
    Unauthorized,
    EventNotOpen,
    EventNotSettled,
    EventNotCancelled,
    OutcomeHasNoBets,
    InvalidWinningOutcome,
    NothingToClaim,
    BetWon,
    ZeroAmount,
    TooManyBets,
    BetNotInUserIndex,
    MathOverflow,
    BettingAlreadyStarted,
    DescriptionTooLong,
    LabelTooLong,
}
