use quasar_lang::prelude::*;

/// Demonstrates Quasar's account type constraints:
/// - `Signer`: automatically verified as a transaction signer
/// - `UncheckedAccount`: no runtime checks (opt-in unchecked access)
/// - `Program<SystemProgram>`: verified as the system program (executable + address check)
///
/// Note: Anchor's `#[account(owner = id())]` owner constraint is not directly available
/// in Quasar. Owner checks can be done manually in the instruction body if needed.
#[derive(Accounts)]
pub struct CheckAccounts {
    /// Checks that this account signed the transaction.
    pub payer: Signer,
    /// No checks performed — the caller is responsible for validation.
    #[account(mut)]
    pub account_to_create: UncheckedAccount,
    /// No automatic owner check in Quasar; see note above.
    #[account(mut)]
    pub account_to_change: UncheckedAccount,
    /// Checks the account is executable and matches the system program address.
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_check_accounts(_accounts: &mut CheckAccounts) -> Result<(), ProgramError> {
    // All validation happens declaratively via the account types above.
    // If any check fails, the runtime rejects the transaction before this runs.
    Ok(())
}
