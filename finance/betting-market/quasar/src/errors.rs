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
    MathOverflow,
    EventNotDraft,
    DescriptionTooLong,
    LabelTooLong,
    NotEnoughOutcomes,
    CloseTimeInPast,
    BettingClosed,
    BettingStillOpen,
    /// The event has not been settled or cancelled, so its accounts cannot be
    /// closed yet.
    EventNotFinished,
    /// Bet accounts are still open, so closing now would strand their claims.
    BetsStillOpen,
    /// Outcome accounts are still open, so the event cannot be closed yet.
    OutcomesStillOpen,
}
