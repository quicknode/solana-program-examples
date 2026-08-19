use std::cell::RefMut;

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{
        transfer_hook::TransferHookAccount, BaseStateWithExtensions, PodStateWithExtensions,
    },
    pod::PodAccount,
};
use spl_discriminator::SplDiscriminate;
use spl_tlv_account_resolution::{account::ExtraAccountMeta, seeds::Seed};
use spl_transfer_hook_interface::instruction::{
    ExecuteInstruction, InitializeExtraAccountMetaListInstruction,
};

mod instructions;
use instructions::*;

declare_id!("1qahDxKHeCLZhbBU2NyMU6vQCQmEUmdeSEBrG5drffK");

#[error_code]
pub enum TransferError {
    #[msg("The amount is too big")]
    AmountTooBig,
    #[msg("The token is not currently transferring")]
    IsNotCurrentlyTransferring,
}

pub mod entrypoint;

// v2's `#[program(interface, ...)]` declares an interface for other programs to
// CPI into and emits no entrypoint, and an executable `#[program]` only accepts
// one-byte custom discriminators, so the transfer-hook interface's eight-byte
// discriminators have no direct spelling. `entrypoint` bridges the gap: it maps
// each of them onto a handler before anchor's dispatch runs.
#[program]
pub mod transfer_hook {
    use super::*;

    // sha256("spl-transfer-hook-interface:initialize-extra-account-metas")[..8]
    pub fn initialize_extra_account_meta_list(
        context: &mut Context<InitializeExtraAccountMetaListAccountConstraints>,
    ) -> Result<()> {
        instructions::initialize_extra_account_meta_list::handler(context)
    }

    // sha256("spl-transfer-hook-interface:execute")[..8]
    pub fn transfer_hook(
        context: &mut Context<TransferHookAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        instructions::transfer_hook::handler(context, amount)
    }
}

pub fn check_is_transferring(context: &Context<TransferHookAccountConstraints>) -> Result<()> {
    // Read-only: the account already holds a shared borrow of its buffer, and a
    // second shared borrow is fine where `try_borrow_mut` would be rejected.
    let account_data_ref = context.accounts.source_token.account().try_borrow()?;
    // .map_err() needed because spl-token-2022 uses solana-program-error 2.x
    // while anchor-lang uses 3.x - structurally identical but different semver types
    let account = PodStateWithExtensions::<PodAccount>::unpack(&account_data_ref)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let account_extension = account
        .get_extension::<TransferHookAccount>()
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if !bool::from(account_extension.transferring) {
        return err!(TransferError::IsNotCurrentlyTransferring);
    }

    Ok(())
}

// Define extra account metas to store on extra_account_meta_list account
pub fn handle_extra_account_metas() -> Result<Vec<ExtraAccountMeta>> {
    // .map_err() needed because spl-tlv-account-resolution uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    Ok(vec![ExtraAccountMeta::new_with_seeds(
        &[Seed::Literal {
            bytes: b"counter".to_vec(),
        }],
        false, // is_signer
        true,  // is_writable
    )
    .map_err(|_| ProgramError::InvalidArgument)?])
}

/// Returns the count of extra account metas (avoids the error conversion issue in #[account] attributes)
pub fn handle_extra_account_metas_count() -> usize {
    1 // one extra account: the counter PDA
}

#[account(borsh)]
#[derive(InitSpace)]
pub struct CounterAccount {
    pub counter: u64,
    /// Canonical bump for this PDA.
    pub bump: u8,
}
