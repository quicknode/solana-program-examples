use std::cell::RefMut;

use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{
        transfer_hook::TransferHookAccount, BaseStateWithExtensionsMut, PodStateWithExtensionsMut,
    },
    pod::PodAccount,
};
use spl_discriminator::SplDiscriminate;
use spl_tlv_account_resolution::account::ExtraAccountMeta;
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

// v2's `#[program(interface)]` declares an interface for CPI and emits no
// entrypoint. This is a real deployable program that also implements the
// transfer-hook interface, so it stays a plain `#[program]`; the interface
// instruction gets its discriminator from `#[discrim = ...]` below.
#[program]
pub mod transfer_hook {
    use super::*;

    pub fn initialize(
        context: &mut Context<InitializeAccountConstraints>,
        decimals: u8,
    ) -> Result<()> {
        instructions::initialize::handler(context, decimals)
    }

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
// In this example there are none
pub fn handle_extra_account_metas() -> Result<Vec<ExtraAccountMeta>> {
    Ok(vec![])
}

/// Returns the count of extra account metas (avoids the error conversion issue in #[account] attributes)
pub fn handle_extra_account_metas_count() -> usize {
    0 // no extra accounts in this example
}
