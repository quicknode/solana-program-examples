use borsh::{BorshDeserialize, BorshSerialize};
use solana_program::{
    account_info::{next_account_info, AccountInfo},
    declare_id,
    entrypoint::ProgramResult,
    msg,
    program_error::ProgramError,
    pubkey::Pubkey,
};

mod state;
pub use state::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[cfg(not(feature = "no-entrypoint"))]
use solana_program::entrypoint;

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    // Reject empty instruction data explicitly rather than panicking via split_at.
    if instruction_data.is_empty() {
        msg!("Error: instruction data is empty");
        return Err(ProgramError::InvalidInstructionData);
    }
    let (instruction_discriminant, instruction_data_inner) = instruction_data.split_at(1);
    match instruction_discriminant[0] {
        0 => {
            msg!("Instruction: Increment");
            process_increment_counter(program_id, accounts, instruction_data_inner)?;
        }
        _ => {
            // Previously this branch logged a message and returned Ok(()), which let
            // unknown discriminants succeed silently. Return InvalidInstructionData
            // so callers get a real failure.
            msg!("Error: unknown instruction");
            return Err(ProgramError::InvalidInstructionData);
        }
    }
    Ok(())
}

pub fn process_increment_counter(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let account_info_iter = &mut accounts.iter();

    let counter_account = next_account_info(account_info_iter)?;
    assert!(
        counter_account.is_writable,
        "Counter account must be writable"
    );

    // Owner check: without this the program would happily decode any 8 bytes of
    // data as a Counter, even when the account belongs to a different program.
    if counter_account.owner != program_id {
        msg!("Error: counter account is not owned by this program");
        return Err(ProgramError::IncorrectProgramId);
    }

    let mut counter = Counter::try_from_slice(&counter_account.try_borrow_mut_data()?)?;
    counter.count += 1;
    counter.serialize(&mut *counter_account.data.borrow_mut())?;

    msg!("Counter state incremented to {:?}", counter.count);
    Ok(())
}
