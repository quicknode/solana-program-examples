use quasar_lang::prelude::*;

/// Errors for direct lamport transfers. Codes start at 6000, the same
/// offset Anchor uses for custom errors.
#[error_code]
pub enum TransferSolError {
    /// The runtime only lets a program debit lamports from accounts it
    /// owns, so a payer owned by anyone else must be rejected up front.
    PayerNotOwnedByProgram = 6000,
    /// The payer does not hold `amount` lamports.
    InsufficientFunds,
    /// Adding `amount` to the recipient balance would overflow a u64.
    AmountOverflow,
}

/// Accounts for transferring SOL by directly manipulating lamports.
/// The `constraints(...)` check enforces that the payer is owned by this
/// program, mirroring the Anchor twin's `owner = crate::ID` constraint.
#[derive(Accounts)]
pub struct TransferSolWithProgramAccountConstraints {
    #[account(
        mut,
        constraints(payer.to_account_view().owner() == &crate::ID)
            @ TransferSolError::PayerNotOwnedByProgram
    )]
    pub payer: UncheckedAccount,

    #[account(mut)]
    pub recipient: UncheckedAccount,
}

#[inline(always)]
pub fn handle_transfer_sol_with_program(
    accounts: &mut TransferSolWithProgramAccountConstraints,
    amount: u64,
) -> Result<(), ProgramError> {
    let payer_view = accounts.payer.to_account_view();
    let recipient_view = accounts.recipient.to_account_view();

    let new_payer_lamports = payer_view
        .lamports()
        .checked_sub(amount)
        .ok_or(TransferSolError::InsufficientFunds)?;
    let new_recipient_lamports = recipient_view
        .lamports()
        .checked_add(amount)
        .ok_or(TransferSolError::AmountOverflow)?;

    set_lamports(payer_view, new_payer_lamports);
    set_lamports(recipient_view, new_recipient_lamports);
    Ok(())
}
