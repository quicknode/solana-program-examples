use anchor_lang::prelude::*;

#[error_code]
pub enum VaultError {
    #[msg("Only the vault authority may withdraw cNFTs from the vault")]
    InvalidWithdrawAuthority,
    #[msg("proof_1_length + proof_2_length must equal the number of proof accounts supplied")]
    ProofLengthMismatch,
}
