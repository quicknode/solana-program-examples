//! Store a document of arbitrary size in a program-owned account, in one
//! instruction.
//!
//! The program itself has nothing to do with transaction v1: it reads the
//! accounts and instruction data it is given like any other program, and does
//! not know or care which transaction format carried them. What v1 changes is
//! how much a single instruction can carry: a 3,000 byte document is beyond
//! the 1,232 byte limit of a legacy or v0 transaction, so the tests send it
//! in a v1 transaction (up to 4,096 bytes) instead.

#![no_std]

use pinocchio::{
    cpi::{Seed, Signer},
    entrypoint,
    error::ProgramError,
    nostd_panic_handler,
    sysvars::{rent::Rent, Sysvar},
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::CreateAccount;

entrypoint!(process_instruction);
nostd_panic_handler!();

/// First byte of the instruction data. Everything after it is the document.
pub const STORE_DOCUMENT: u8 = 0;

/// Seed prefix of the document account. One document per payer.
pub const DOCUMENT_SEED: &[u8] = b"document";

fn process_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((&STORE_DOCUMENT, document)) => store_document(program_id, accounts, document),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

/// Create the payer's document PDA, sized to the document exactly, and copy
/// the document into it. Rent comes from the payer.
fn store_document(
    program_id: &Address,
    accounts: &[AccountView],
    document: &[u8],
) -> ProgramResult {
    let [payer, document_account, system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    if !payer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    if document.is_empty() {
        return Err(ProgramError::InvalidInstructionData);
    }
    if !pinocchio_system::check_id(system_program.address()) {
        return Err(ProgramError::IncorrectProgramId);
    }

    // The client passes the PDA; derive it here so nobody can hand us some
    // other account to write into.
    let (expected_address, bump) =
        Address::find_program_address(&[DOCUMENT_SEED, payer.address().as_ref()], program_id);
    if document_account.address() != &expected_address {
        return Err(ProgramError::InvalidSeeds);
    }

    let lamports_required = Rent::get()?.try_minimum_balance(document.len())?;

    let bump_bytes = [bump];
    let seeds = [
        Seed::from(DOCUMENT_SEED),
        Seed::from(payer.address().as_ref()),
        Seed::from(&bump_bytes),
    ];
    let signers = [Signer::from(&seeds)];

    CreateAccount {
        from: payer,
        to: document_account,
        lamports: lamports_required,
        space: document.len() as u64,
        owner: program_id,
    }
    .invoke_signed(&signers)?;

    document_account.try_borrow_mut()?.copy_from_slice(document);

    Ok(())
}
