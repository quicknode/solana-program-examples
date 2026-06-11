use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    pubkey::Pubkey,
};

use crate::state::user::User;

pub fn close_user(program_id: &Pubkey, accounts: &[AccountInfo]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let target_account = next_account_info(accounts_iter)?;
    let payer = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;

    // Only the user whose key derives the PDA may close it.
    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // The target must be this payer's own User PDA; otherwise anyone could
    // close anyone else's account and pocket the rent.
    let (user_pda, _) = Pubkey::find_program_address(
        &[User::SEED_PREFIX.as_bytes(), payer.key.as_ref()],
        program_id,
    );
    if &user_pda != target_account.key {
        return Err(ProgramError::InvalidSeeds);
    }

    // The account must belong to this program before we drain it.
    if target_account.owner != program_id {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Move ALL lamports back to the payer. Leaving any balance behind would
    // strand it forever: nobody can sign for the PDA to recover it later.
    let lamports_to_return = target_account.lamports();
    let new_payer_lamports = payer
        .lamports()
        .checked_add(lamports_to_return)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    **payer.lamports.borrow_mut() = new_payer_lamports;
    **target_account.lamports.borrow_mut() = 0;

    // Wipe the data and hand the empty account back to the System Program.
    target_account.resize(0)?;
    target_account.assign(system_program.key);

    Ok(())
}
