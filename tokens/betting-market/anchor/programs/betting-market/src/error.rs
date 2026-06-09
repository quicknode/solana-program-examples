use anchor_lang::prelude::*;

#[error_code]
pub enum BettingError {
    #[msg("Fee in basis points cannot exceed 10000 (100%)")]
    FeeTooHigh,
    #[msg("Only the admin may perform this action")]
    Unauthorized,
    #[msg("The event is not open for this action")]
    EventNotOpen,
    #[msg("The event has not been settled")]
    EventNotSettled,
    #[msg("The event has not been cancelled")]
    EventNotCancelled,
    #[msg("The winning outcome has no bets, so the event cannot be settled to it")]
    OutcomeHasNoBets,
    #[msg("The winning outcome index does not match the provided outcome account")]
    InvalidWinningOutcome,
    #[msg("This bet did not win, so there is nothing to claim")]
    NothingToClaim,
    #[msg("This bet has already been claimed")]
    AlreadyClaimed,
    #[msg("The bet amount must be greater than zero")]
    ZeroAmount,
    #[msg("This bettor already holds the maximum number of distinct bets")]
    TooManyBets,
    #[msg("Outcomes can only be added before any bets are placed")]
    BettingAlreadyStarted,
    #[msg("The event description is too long")]
    DescriptionTooLong,
    #[msg("The outcome label is too long")]
    LabelTooLong,
}
