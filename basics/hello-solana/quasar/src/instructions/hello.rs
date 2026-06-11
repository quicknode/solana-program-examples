use quasar_lang::prelude::*;

/// Accounts for the hello instruction.
/// A payer (signer) is required to submit the transaction, but the program
/// simply logs a greeting and the program ID.
#[derive(Accounts)]
pub struct HelloAccountConstraints {
    #[allow(dead_code)]
    pub payer: Signer,
}

#[inline(always)]
pub fn handle_hello(_accounts: &mut HelloAccountConstraints) -> Result<(), ProgramError> {
    log("Hello, Solana!");
    log("Our program's Program ID: 2phbC62wekpw95XuBk4i1KX4uA8zBUWmYbiTMhicSuBV");
    Ok(())
}
