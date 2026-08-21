use anchor_lang::prelude::*;

#[error_code]
pub enum TransferSolError {
    #[msg("The payer does not hold enough lamports for this transfer")]
    InsufficientFunds,
    #[msg("Adding the amount to the recipient balance would overflow a u64")]
    AmountOverflow,
}

#[derive(Accounts)]
pub struct TransferSolWithProgramAccountConstraints<'info> {
    /// CHECK: Use owner constraint to check account is owned by our program
    #[account(
        mut,
        owner = crate::ID // value of declare_id!()
    )]
    payer: UncheckedAccount<'info>,

    #[account(mut)]
    recipient: SystemAccount<'info>,
}

// Directly modifying lamports is only possible if the program is the owner of the account
pub fn handler(
    context: Context<TransferSolWithProgramAccountConstraints>,
    amount: u64,
) -> Result<()> {
    let payer = &context.accounts.payer;
    let recipient = &context.accounts.recipient;

    let new_payer_lamports = payer
        .lamports()
        .checked_sub(amount)
        .ok_or(TransferSolError::InsufficientFunds)?;
    let new_recipient_lamports = recipient
        .lamports()
        .checked_add(amount)
        .ok_or(TransferSolError::AmountOverflow)?;

    **payer.try_borrow_mut_lamports()? = new_payer_lamports;
    **recipient.try_borrow_mut_lamports()? = new_recipient_lamports;
    Ok(())
}
