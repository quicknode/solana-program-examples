use anchor_lang::error_code;

// Anchor's IDL build allows only a single `#[error_code]` enum per program, so
// the game and program-level errors live in one enum.
#[error_code]
pub enum GameErrorCode {
    #[msg("Not enough energy")]
    NotEnoughEnergy,
    #[msg("Wrong Authority")]
    WrongAuthority,
    #[msg("Invalid Mint account space")]
    InvalidMintAccountSpace,
    #[msg("Cant initialize metadata_pointer")]
    CantInitializeMetadataPointer,
}
