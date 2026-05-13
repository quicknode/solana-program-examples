use anchor_lang::prelude::*;

#[error_code]
pub enum HackathonError {
    #[msg("Hackathon name must not be empty")]
    EmptyName,
    #[msg("Hackathon name exceeds the maximum length")]
    NameTooLong,
    #[msg("Prize has already been paid")]
    AlreadyPaid,
    #[msg("Prize has been cancelled")]
    Cancelled,
    #[msg("Prize has no winner set")]
    NoWinner,
    #[msg("Recorded winner does not match the supplied winner token account owner")]
    WinnerMismatch,
    #[msg("Vault balance is less than the prize amount")]
    Underfunded,
    #[msg("Prize counter overflow: this hackathon already holds the maximum prizes")]
    PrizeCounterOverflow,
    #[msg("Cannot close hackathon: at least one prize is still active")]
    PrizesStillActive,
}
