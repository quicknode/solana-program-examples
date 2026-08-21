use anchor_lang::{prelude::*, solana_program::pubkey::Pubkey};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_2022::spl_token_2022::{
        extension::{
            transfer_hook::TransferHookAccount, BaseStateWithExtensionsMut,
            PodStateWithExtensionsMut,
        },
        pod::PodAccount,
    },
    token_interface::Mint,
};
use spl_discriminator::SplDiscriminate;
use spl_tlv_account_resolution::{
    account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList,
};
use spl_transfer_hook_interface::instruction::{
    ExecuteInstruction, InitializeExtraAccountMetaListInstruction,
};
use std::{cell::RefMut, str::FromStr};

// transfer-hook program that charges a SOL fee on token transfer
// use a delegate and wrapped SOL because signers from initial transfer are not accessible

mod instructions;
use instructions::*;

declare_id!("FjcHckEgXcBhFmSGai3FRpDLiT6hbpV893n8iTxVd81g");

#[error_code]
pub enum TransferError {
    #[msg("Amount Too big")]
    AmountTooBig,
    #[msg("The token is not currently transferring")]
    IsNotCurrentlyTransferring,
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
    pub fn transfer_hook(context: Context<TransferHookAccountConstraints>, amount: u64) -> Result<()> {
        instructions::transfer_hook::handler(context, amount)
    }
}

pub fn check_is_transferring(context: &Context<TransferHookAccountConstraints>) -> Result<()> {
    let source_token_info = context.accounts.source_token.to_account_info();
    let mut account_data_ref: RefMut<&mut [u8]> = source_token_info.try_borrow_mut_data()?;
    let mut account = PodStateWithExtensionsMut::<PodAccount>::unpack(*account_data_ref)
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
    // When the token2022 program CPIs to the transfer_hook instruction on this program,
    // the accounts are provided in order defined specified the list:

    // index 0-3 are the accounts required for token transfer (source, mint, destination, owner)
    // index 4 is address of ExtraAccountMetaList account

    let wsol_mint = Pubkey::from_str("So11111111111111111111111111111111111111112").unwrap();
    let token_program_id = Token::id();
    let ata_program_id = AssociatedToken::id();

    Ok(vec![
        // index 5, wrapped SOL mint
        ExtraAccountMeta::new_with_pubkey(&wsol_mint, false, false)
            .map_err(|_| ProgramError::InvalidArgument)?,
        // index 6, token program (for wsol token transfer)
        ExtraAccountMeta::new_with_pubkey(&token_program_id, false, false)
            .map_err(|_| ProgramError::InvalidArgument)?,
        // index 7, associated token program
        ExtraAccountMeta::new_with_pubkey(&ata_program_id, false, false)
            .map_err(|_| ProgramError::InvalidArgument)?,
        // index 8, delegate PDA
        ExtraAccountMeta::new_with_seeds(
            &[Seed::Literal {
                bytes: b"delegate".to_vec(),
            }],
            false, // is_signer
            true,  // is_writable
        )
        .map_err(|_| ProgramError::InvalidArgument)?,
        // index 9, delegate wrapped SOL token account
        ExtraAccountMeta::new_external_pda_with_seeds(
            7, // associated token program index
            &[
                Seed::AccountKey { index: 8 }, // owner index (delegate PDA)
                Seed::AccountKey { index: 6 }, // token program index
                Seed::AccountKey { index: 5 }, // wsol mint index
            ],
            false, // is_signer
            true,  // is_writable
        )
        .map_err(|_| ProgramError::InvalidArgument)?,
        // index 10, sender wrapped SOL token account
        ExtraAccountMeta::new_external_pda_with_seeds(
            7, // associated token program index
            &[
                Seed::AccountKey { index: 3 }, // owner index
                Seed::AccountKey { index: 6 }, // token program index
                Seed::AccountKey { index: 5 }, // wsol mint index
            ],
            false, // is_signer
            true,  // is_writable
        )
        .map_err(|_| ProgramError::InvalidArgument)?,
        ExtraAccountMeta::new_with_seeds(
            &[Seed::Literal {
                bytes: b"counter".to_vec(),
            }],
            false, // is_signer
            true,  // is_writable
        )
        .map_err(|_| ProgramError::InvalidArgument)?,
    ])
}

/// Returns the count of extra account metas (avoids the error conversion issue in #[account] attributes)
pub fn handle_extra_account_metas_count() -> usize {
    7 // wsol_mint, token_program, ata_program, delegate, delegate_wsol, sender_wsol, counter
}

#[account]
#[derive(InitSpace)]
pub struct CounterAccount {
    pub counter: u8,
    /// Canonical bump for this PDA.
    pub bump: u8,
}
