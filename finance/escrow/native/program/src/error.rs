use solana_program::program_error::ProgramError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EscrowError {
    #[error("Offer key provided does not match expected")]
    OfferKeyMismatch,

    #[error("Token account provided does not match expected")]
    TokenAccountMismatch,

    #[error("Maker account provided does not match the offer's maker")]
    MakerMismatch,

    #[error("Token mint provided does not match the offer's mint")]
    MintMismatch,

    #[error("Maker's token B account must exist before the offer can be taken")]
    MakerTokenAccountBNotInitialized,

    #[error("Token balances after transfer do not balance against the amounts moved")]
    TokenConservationViolation,

    #[error("Arithmetic overflow")]
    ArithmeticOverflow,
}

impl From<EscrowError> for ProgramError {
    fn from(e: EscrowError) -> Self {
        ProgramError::Custom(e as u32)
    }
}
