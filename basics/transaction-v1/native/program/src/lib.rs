//! Store a document of arbitrary size in a program-owned account, in one
//! instruction.
//!
//! The program itself has nothing to do with transaction v1: it reads the
//! accounts and instruction data it is given like any other program, and does
//! not know or care which transaction format carried them. What v1 changes is
//! how much a single instruction can carry: a 3,000 byte document is beyond
//! the 1,232 byte limit of a legacy or v0 transaction, so the tests send it
//! in a v1 transaction (up to 4,096 bytes) instead.

use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint,
    entrypoint::ProgramResult,
    program::invoke_signed,
    program_error::ProgramError,
    pubkey::Pubkey,
    rent::Rent,
    sysvar::Sysvar,
};

/// First byte of the instruction data. Everything after it is the document.
pub const STORE_DOCUMENT: u8 = 0;

/// Seed prefix of the document account. One document per payer.
pub const DOCUMENT_SEED: &[u8] = b"document";

entrypoint!(process_instruction);

pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((&STORE_DOCUMENT, document)) => store_document(program_id, accounts, document),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

/// Create the payer's document PDA, sized to the document exactly, and copy
/// the document into it. Rent comes from the payer.
fn store_document(program_id: &Pubkey, accounts: &[AccountInfo], document: &[u8]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();
    let payer = next_account_info(accounts_iter)?;
    let document_account = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;

    if !payer.is_signer {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if document.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }
    if !solana_system_interface::program::check_id(system_program.key) {
        return Err(ProgramError::IncorrectProgramId);
    }

    // The client passes the PDA; derive it here so nobody can hand us some
    // other account to write into.
    let (expected_address, bump) =
        Pubkey::find_program_address(&[DOCUMENT_SEED, payer.key.as_ref()], program_id);
    if *document_account.key != expected_address {
        return Err(ProgramError::InvalidSeeds);
    }

    let lamports_required = Rent::get()?.minimum_balance(document.len());

    invoke_signed(
        &solana_system_interface::instruction::create_account(
            payer.key,
            document_account.key,
            lamports_required,
            document.len() as u64,
            program_id,
        ),
        &[
            payer.clone(),
            document_account.clone(),
            system_program.clone(),
        ],
        &[&[DOCUMENT_SEED, payer.key.as_ref(), &[bump]]],
    )?;

    document_account.data.borrow_mut().copy_from_slice(document);

    Ok(())
}
