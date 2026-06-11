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

fn process_instruction(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    match instruction_data.split_first() {
        Some((&CREATE_DISCRIMINATOR, data)) => process_user(program_id, accounts, data),
        Some((&CLOSE_DISCRIMINATOR, _)) => process_close(program_id, accounts),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}

pub const CREATE_DISCRIMINATOR: u8 = 0;
pub const CLOSE_DISCRIMINATOR: u8 = 1;

pub struct User<'a> {
    pub name: &'a [u8],
}

impl<'a> User<'a> {
    pub const SEED_PREFIX: &'static str = "USER";
    pub const LEN: usize = 16;
}

fn process_user(
    program_id: &Address,
    accounts: &[AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    let [target_account, payer, _system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Expected layout: 1 bump byte followed by `User::LEN` name bytes.
    // Bounds-check before slicing so malformed input returns a clean error
    // instead of panicking.
    if instruction_data.len() < 1 + User::LEN {
        return Err(ProgramError::InvalidInstructionData);
    }
    let bump = instruction_data[0];

    // The bump comes from the client, so verify it: it must be the canonical
    // bump and the derived PDA must be the account we were asked to create.
    let (user_pda, canonical_bump) = Address::find_program_address(
        &[User::SEED_PREFIX.as_bytes(), payer.address().as_ref()],
        program_id,
    );
    if bump != canonical_bump || target_account.address() != &user_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    let rent = Rent::get()?;

    let account_span = User::LEN;
    let lamports_required = rent.try_minimum_balance(account_span)?;

    let bump_bytes = [bump];

    let seeds = [
        Seed::from(User::SEED_PREFIX.as_bytes()),
        Seed::from(payer.address().as_ref()),
        Seed::from(&bump_bytes),
    ];
    let signers = [Signer::from(&seeds)];

    CreateAccount {
        from: payer,
        to: target_account,
        lamports: lamports_required,
        space: account_span as u64,
        owner: program_id,
    }
    .invoke_signed(&signers)?;

    let mut user_account_data = target_account.try_borrow_mut()?;
    user_account_data.copy_from_slice(&instruction_data[1..1 + User::LEN]);

    Ok(())
}

fn process_close(program_id: &Address, accounts: &[AccountView]) -> ProgramResult {
    let [target_account, payer, system_program] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // Only the user whose key derives the PDA may close it.
    if !payer.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }

    // The target must be this payer's own User PDA; otherwise anyone could
    // close anyone else's account and pocket the rent.
    let (user_pda, _) = Address::find_program_address(
        &[User::SEED_PREFIX.as_bytes(), payer.address().as_ref()],
        program_id,
    );
    if target_account.address() != &user_pda {
        return Err(ProgramError::InvalidSeeds);
    }

    // The account must belong to this program before we drain it.
    if !target_account.owned_by(program_id) {
        return Err(ProgramError::IncorrectProgramId);
    }

    // Move ALL lamports back to the payer. Leaving any balance behind would
    // strand it forever: nobody can sign for the PDA to recover it later.
    let lamports_to_return = target_account.lamports();
    let new_payer_lamports = payer
        .lamports()
        .checked_add(lamports_to_return)
        .ok_or(ProgramError::ArithmeticOverflow)?;
    payer.set_lamports(new_payer_lamports);
    target_account.set_lamports(0);

    // Wipe the data and hand the empty account back to the System Program.
    target_account.resize(0)?;

    unsafe {
        target_account.assign(system_program.address());
    }

    Ok(())
}
