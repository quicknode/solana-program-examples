use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::Token,
    token_2022::spl_token_2022::{
        extension::{
            transfer_hook::TransferHookAccount, BaseStateWithExtensions,
            PodStateWithExtensions,
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

pub mod entrypoint;

// v2's `#[program(interface, ...)]` declares an interface for other programs to
// CPI into and emits no entrypoint, and an executable `#[program]` only accepts
// one-byte custom discriminators — so the transfer-hook interface's eight-byte
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
    // When the token2022 program CPIs to the transfer_hook instruction on this program,
    // the accounts are provided in order defined specified the list:

    // index 0-3 are the accounts required for token transfer (source, mint, destination, owner)
    // index 4 is address of ExtraAccountMetaList account

    let wsol_mint = Address::from_str("So11111111111111111111111111111111111111112").unwrap();
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

#[account(borsh)]
#[derive(InitSpace)]
pub struct CounterAccount {
    pub counter: u8,
    /// Canonical bump for this PDA.
    pub bump: u8,
}
