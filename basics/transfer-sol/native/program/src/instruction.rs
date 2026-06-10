use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program::invoke,
    program_error::ProgramError,
    pubkey::Pubkey,
};

pub fn transfer_sol_with_cpi(accounts: &[AccountInfo], amount: u64) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let payer = next_account_info(accounts_iter)?;
    let recipient = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;

    invoke(
        &solana_system_interface::instruction::transfer(payer.key, recipient.key, amount),
        &[payer.clone(), recipient.clone(), system_program.clone()],
    )?;

    Ok(())
}

pub fn transfer_sol_with_program(
    _program_id: &Pubkey,
    accounts: &[AccountInfo],
    amount: u64,
) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let payer = next_account_info(accounts_iter)?;
    let recipient = next_account_info(accounts_iter)?;

    // Checked math: reject an overdraw or a balance overflow instead of
    // silently wrapping.
    let new_payer_lamports = payer
        .lamports()
        .checked_sub(amount)
        .ok_or(ProgramError::InsufficientFunds)?;
    let new_recipient_lamports = recipient
        .lamports()
        .checked_add(amount)
        .ok_or(ProgramError::ArithmeticOverflow)?;

    **payer.try_borrow_mut_lamports()? = new_payer_lamports;
    **recipient.try_borrow_mut_lamports()? = new_recipient_lamports;

    Ok(())
}
