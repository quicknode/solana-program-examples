use anchor_lang::prelude::*;

#[error_code]
pub enum EscrowError {
    #[msg("An offer must offer and want more than zero tokens")]
    ZeroAmount,
}
