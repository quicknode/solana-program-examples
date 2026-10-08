use anchor_lang::error_code;

#[error_code]
pub enum FundraiserError {
    #[msg("The amount to raise has not been met")]
    TargetNotMet,
    #[msg("The amount to raise has been achieved")]
    TargetMet,
    #[msg("The contribution is too small")]
    ContributionTooSmall,
    #[msg("The fundraiser has not ended yet")]
    FundraiserNotEnded,
    #[msg("The fundraiser has ended")]
    FundraiserEnded,
    #[msg("The amount to raise is below the minimum of 3 major units")]
    InvalidAmount,
    #[msg("Contributions have not all been refunded yet")]
    RefundsOutstanding,
    #[msg("Arithmetic overflow")]
    MathOverflow,
    #[msg("The fundraiser has already been claimed")]
    FundraiserClaimed,
    #[msg(
        "The fundraiser has not been claimed, so the contribution account closes through refund"
    )]
    FundraiserNotClaimed,
    #[msg("Contribution accounts for this fundraiser are still open, so it cannot close yet")]
    ContributionsOpen,
}
