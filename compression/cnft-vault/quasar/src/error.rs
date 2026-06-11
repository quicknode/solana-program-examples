use quasar_lang::prelude::*;

#[error_code]
pub enum VaultError {
    /// Only the vault authority may withdraw cNFTs from the vault.
    // 6000 is the conventional Anchor-compatible starting offset for
    // program-specific error codes (Quasar's #[error_code] starts at 0
    // unless told otherwise; framework errors occupy 3000+). Matches the
    // Anchor twin's codes.
    InvalidWithdrawAuthority = 6000,
    /// proof_1_length + proof_2_length must equal the number of proof
    /// accounts supplied.
    ProofLengthMismatch,
}
