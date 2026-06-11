use std::cell::RefMut;

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{
        transfer_hook::TransferHookAccount, BaseStateWithExtensionsMut,
        PodStateWithExtensionsMut,
    },
    pod::PodAccount,
};
use spl_tlv_account_resolution::account::ExtraAccountMeta;
use spl_discriminator::SplDiscriminate;
use spl_transfer_hook_interface::instruction::{
    ExecuteInstruction, InitializeExtraAccountMetaListInstruction,
};

mod instructions;
use instructions::*;

declare_id!("jY5DfVksJT8Le38LCaQhz5USeiGu4rUeVSS8QRAMoba");

#[error_code]
pub enum TransferError {
    #[msg("The token is not currently transferring")]
    IsNotCurrentlyTransferring,
}

#[program]
pub mod transfer_hook {
    use super::*;

    pub fn initialize(context: Context<InitializeAccountConstraints>, decimals: u8) -> Result<()> {
        instructions::initialize::handler(context, decimals)
    }

    #[instruction(discriminator = InitializeExtraAccountMetaListInstruction::SPL_DISCRIMINATOR_SLICE)]
    pub fn initialize_extra_account_meta_list(
        context: Context<InitializeExtraAccountMetaListAccountConstraints>,
    ) -> Result<()> {
        instructions::initialize_extra_account_meta_list::handler(context)
    }

    #[instruction(discriminator = ExecuteInstruction::SPL_DISCRIMINATOR_SLICE)]
    pub fn transfer_hook(context: Context<TransferHookAccountConstraints>, amount: u64) -> Result<()> {
        instructions::transfer_hook::handler(context, amount)
    }
}

pub fn check_is_transferring(context: &Context<TransferHookAccountConstraints>) -> Result<()> {
    let source_token_info = context.accounts.source_token.to_account_info();
    let mut account_data_ref: RefMut<&mut [u8]> = source_token_info.try_borrow_mut_data()?;
    // .map_err() needed because spl-token-2022 uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    let mut account = PodStateWithExtensionsMut::<PodAccount>::unpack(*account_data_ref)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let account_extension = account.get_extension_mut::<TransferHookAccount>()
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if !bool::from(account_extension.transferring) {
        return err!(TransferError::IsNotCurrentlyTransferring);
    }

    Ok(())
}

// Define extra account metas to store on extra_account_meta_list account
// In this example there are none
pub fn handle_extra_account_metas() -> Result<Vec<ExtraAccountMeta>> {
    Ok(vec![])
}

/// Returns the count of extra account metas (avoids the error conversion issue in #[account] attributes)
pub fn handle_extra_account_metas_count() -> usize {
    0 // no extra accounts in this example
}
