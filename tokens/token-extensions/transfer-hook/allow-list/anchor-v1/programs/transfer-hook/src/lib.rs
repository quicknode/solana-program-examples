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

pub const ALLOW_LIST_SEED: &[u8] = b"allow_list";
pub const ALLOWED_WALLET_SEED: &[u8] = b"allowed_wallet";

#[error_code]
pub enum AllowListError {
    #[msg("The token is not currently transferring")]
    IsNotCurrentlyTransferring,
    #[msg("The recipient wallet is not on this mint's allow list")]
    RecipientNotAllowed,
}

#[program]
pub mod transfer_hook {
    use super::*;

    #[instruction(discriminator = InitializeExtraAccountMetaListInstruction::SPL_DISCRIMINATOR_SLICE)]
    pub fn initialize_extra_account_meta_list(
        context: Context<InitializeExtraAccountMetaListAccountConstraints>,
    ) -> Result<()> {
        instructions::initialize_extra_account_meta_list::handler(context)
    }

    #[instruction(discriminator = ExecuteInstruction::SPL_DISCRIMINATOR_SLICE)]
    pub fn transfer_hook(
        context: Context<TransferHookAccountConstraints>,
        amount: u64,
    ) -> Result<()> {
        instructions::transfer_hook::handler(context, amount)
    }

    pub fn allow_wallet(context: Context<AllowWalletAccountConstraints>) -> Result<()> {
        instructions::allow_wallet::handler(context)
    }

    pub fn disallow_wallet(context: Context<DisallowWalletAccountConstraints>) -> Result<()> {
        instructions::disallow_wallet::handler(context)
    }
}

pub fn check_is_transferring(context: &Context<TransferHookAccountConstraints>) -> Result<()> {
    let source_token_info = context.accounts.source_token.to_account_info();
    let mut account_data_ref: RefMut<&mut [u8]> = source_token_info.try_borrow_mut_data()?;
    // .map_err() needed because spl-token-2022 uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    let mut account = PodStateWithExtensionsMut::<PodAccount>::unpack(*account_data_ref)
        .map_err(|_| ProgramError::InvalidAccountData)?;
    let account_extension = account
        .get_extension_mut::<TransferHookAccount>()
        .map_err(|_| ProgramError::InvalidAccountData)?;

    if !bool::from(account_extension.transferring) {
        return err!(AllowListError::IsNotCurrentlyTransferring);
    }

    Ok(())
}

// The one extra account Token Extensions passes to the hook: the recipient's
// `AllowedWallet` PDA, ["allowed_wallet", mint, recipient wallet]. Token
// Extensions derives it at transfer time from the mint (account 1) and the
// owner field of the destination token account (account 2, bytes 32..64).
pub fn handle_extra_account_metas() -> Result<Vec<ExtraAccountMeta>> {
    // .map_err() needed because spl-tlv-account-resolution uses solana-program-error 2.x
    // while anchor-lang 1.0 uses 3.x - structurally identical but different semver types
    Ok(vec![ExtraAccountMeta::new_with_seeds(
        &[
            Seed::Literal {
                bytes: ALLOWED_WALLET_SEED.to_vec(),
            },
            Seed::AccountKey { index: 1 },
            Seed::AccountData {
                account_index: 2,
                data_index: 32,
                length: 32,
            },
        ],
        false, // is_signer
        false, // is_writable
    )
    .map_err(|_| ProgramError::InvalidArgument)?])
}

/// Returns the count of extra account metas (avoids the error conversion issue in #[account] attributes)
pub fn handle_extra_account_metas_count() -> usize {
    1 // one extra account: the recipient's AllowedWallet PDA
}

/// One per mint: who may add wallets to that mint's allow list.
#[account]
#[derive(InitSpace)]
pub struct AllowList {
    pub authority: Pubkey,
    /// Canonical bump for this PDA.
    pub bump: u8,
}

/// One per allowed wallet per mint. Its existence is the permission: the hook
/// accepts a transfer only when the recipient's `AllowedWallet` account exists.
#[account]
#[derive(InitSpace)]
pub struct AllowedWallet {
    /// Canonical bump for this PDA.
    pub bump: u8,
}
