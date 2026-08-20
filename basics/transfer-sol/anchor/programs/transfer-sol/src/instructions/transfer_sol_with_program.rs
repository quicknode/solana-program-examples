use anchor_lang::prelude::*;

#[error_code]
pub enum TransferSolError {
    #[msg("The payer does not hold enough lamports for this transfer")]
    InsufficientFunds,
    #[msg("Adding the amount to the recipient balance would overflow a u64")]
    AmountOverflow,
}

#[derive(Accounts)]
pub struct TransferSolWithProgramAccountConstraints {
    /// CHECK: Use owner constraint to check account is owned by our program
    #[account(
        mut,
        owner = crate::ID // value of declare_id!()
    )]
    pub payer: UncheckedAccount,

    #[account(mut)]
    pub recipient: SystemAccount,
}

// Directly modifying lamports is only possible if the program is the owner of the account
pub fn handler(
    context: &mut Context<TransferSolWithProgramAccountConstraints>,
    amount: u64,
) -> Result<()> {
    let payer = &context.accounts.payer;
    let recipient = &context.accounts.recipient;

    let new_payer_lamports = payer
        .get_lamports()
        .checked_sub(amount)
        .ok_or(TransferSolError::InsufficientFunds)?;
    let new_recipient_lamports = recipient
        .get_lamports()
        .checked_add(amount)
        .ok_or(TransferSolError::AmountOverflow)?;

    // `AccountView` is `Copy`, and a copy still points at the same backing
    // buffer, so `set_lamports` writes through to the real account.
    let mut payer_view = *payer.account();
    let mut recipient_view = *recipient.account();
    payer_view.set_lamports(new_payer_lamports);
    recipient_view.set_lamports(new_recipient_lamports);
    Ok(())
}
