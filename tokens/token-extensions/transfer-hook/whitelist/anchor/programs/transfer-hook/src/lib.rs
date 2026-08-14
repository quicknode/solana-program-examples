use std::cell::RefMut;

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{
        transfer_hook::TransferHookAccount, BaseStateWithExtensionsMut, PodStateWithExtensionsMut,
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

declare_id!("DrWbQtYJGtsoRwzKqAbHKHKsCJJfpysudF39GBVFSxub");

#[error_code]
pub enum TransferError {
    #[msg("The token is not currently transferring")]
    IsNotCurrentlyTransferring,
}

#[program(interface, program_id = ID)]
pub mod transfer_hook {
    use super::*;

    // sha256("spl-transfer-hook-interface:initialize-extra-account-metas")[..8]
    #[discrim = [43, 34, 13, 49, 167, 88, 235, 235]]
    pub fn initialize_extra_account_meta_list(
        context: &mut Context<InitializeExtraAccountMetaListAccountConstraints>,
    ) -> Result<()> {
        instructions::initialize_extra_account_meta_list::handler(context)
    }

    // sha256("spl-transfer-hook-interface:execute")[..8]
    #[discrim = [105, 37, 101, 197, 75, 251, 102, 26]]
    pub fn transfer_hook(
        context: &mut Context<TransferHookAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        instructions::transfer_hook::handler(context, amount)
    }

    pub fn add_to_whitelist(context: &mut Context<AddToWhiteListAccountConstraints>) -> Result<()> {
        instructions::add_to_whitelist::handler(context)
    }
}

pub fn check_is_transferring(context: &Context<TransferHookAccountConstraints>) -> Result<()> {
    let mut source_token_info = *context.accounts.source_token.account();
    let mut account_data_ref = source_token_info.try_borrow_mut()?;
    // .map_err() needed because spl-token-2022 uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    let mut account = PodStateWithExtensionsMut::<PodAccount>::unpack(&mut account_data_ref)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let account_extension = account
        .get_extension_mut::<TransferHookAccount>()
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
            bytes: "white_list".as_bytes().to_vec(),
        }],
        false, // is_signer
        true,  // is_writable
    )
    .map_err(|_| ProgramError::InvalidArgument)?])
}

/// Returns the count of extra account metas (avoids the error conversion issue in #[account] attributes)
pub fn handle_extra_account_metas_count() -> usize {
    1 // one extra account: the whitelist PDA
}

#[account(borsh)]
#[derive(InitSpace)]
pub struct WhiteList {
    pub authority: Address,
    #[max_len(11)]
    pub white_list: Vec<Address>,
    /// Canonical bump for this PDA.
    pub bump: u8,
}
