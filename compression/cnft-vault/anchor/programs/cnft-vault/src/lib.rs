#![allow(clippy::diverging_sub_expression)]

use anchor_lang::prelude::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};
use borsh::BorshSerialize;

pub mod error;
mod instructions;
pub mod state;
use instructions::*;

declare_id!("Fd4iwpPWaCU8BNwGQGtvvrcvG4Tfizq3RgLm8YLBJX6D");

/// mpl-bubblegum program ID (BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY)
const MPL_BUBBLEGUM_ID: Pubkey = Pubkey::new_from_array([
    0x98, 0x8b, 0x80, 0xeb, 0x79, 0x35, 0x28, 0x69, 0xb2, 0x24, 0x74, 0x5f, 0x59, 0xdd, 0xbf, 0x8a,
    0x26, 0x58, 0xca, 0x13, 0xdc, 0x68, 0x81, 0x21, 0x26, 0x35, 0x1c, 0xae, 0x07, 0xc1, 0xa5, 0xa5,
]);

/// SPL Account Compression program ID (cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK)
const SPL_ACCOUNT_COMPRESSION_ID: Pubkey = Pubkey::new_from_array([
    0x09, 0x2a, 0x13, 0xee, 0x95, 0xc4, 0x1c, 0xba, 0x08, 0xa6, 0x7f, 0x5a, 0xc6, 0x7e, 0x8d, 0xf7,
    0xe1, 0xda, 0x11, 0x62, 0x5e, 0x1d, 0x64, 0x13, 0x7f, 0x8f, 0x4f, 0x23, 0x83, 0x03, 0x7f, 0x14,
]);

/// Transfer instruction discriminator from mpl-bubblegum
const TRANSFER_DISCRIMINATOR: [u8; 8] = [163, 52, 200, 231, 140, 3, 69, 186];

/// Instruction arguments for mpl-bubblegum Transfer, serialized with borsh
#[derive(BorshSerialize)]
pub struct TransferArgs {
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
}

#[derive(Clone)]
pub struct SPLCompression;

impl anchor_lang::Id for SPLCompression {
    fn id() -> Pubkey {
        SPL_ACCOUNT_COMPRESSION_ID
    }
}

/// Build a mpl-bubblegum Transfer instruction from pubkeys and args.
/// This avoids using mpl-bubblegum's CPI wrapper which requires solana-program 2.x AccountInfo.
#[allow(clippy::too_many_arguments)]
pub fn build_transfer_instruction(
    tree_config: Pubkey,
    leaf_owner: Pubkey,
    leaf_delegate: Pubkey,
    new_leaf_owner: Pubkey,
    merkle_tree: Pubkey,
    log_wrapper: Pubkey,
    compression_program: Pubkey,
    system_program: Pubkey,
    remaining_accounts: &[AccountMeta],
    args: TransferArgs,
) -> Result<Instruction> {
    let mut accounts = Vec::with_capacity(8 + remaining_accounts.len());
    accounts.push(AccountMeta::new_readonly(tree_config, false));
    // leaf_owner is a signer (PDA signs via invoke_signed)
    accounts.push(AccountMeta::new_readonly(leaf_owner, true));
    // leaf_delegate = leaf_owner, not an additional signer
    accounts.push(AccountMeta::new_readonly(leaf_delegate, false));
    accounts.push(AccountMeta::new_readonly(new_leaf_owner, false));
    accounts.push(AccountMeta::new(merkle_tree, false));
    accounts.push(AccountMeta::new_readonly(log_wrapper, false));
    accounts.push(AccountMeta::new_readonly(compression_program, false));
    accounts.push(AccountMeta::new_readonly(system_program, false));
    accounts.extend_from_slice(remaining_accounts);

    let mut data = TRANSFER_DISCRIMINATOR.to_vec();
    args.serialize(&mut data)?;

    Ok(Instruction {
        program_id: MPL_BUBBLEGUM_ID,
        accounts,
        data,
    })
}

#[program]
pub mod cnft_vault {
    use super::*;

    pub fn initialize_vault(context: Context<InitializeVaultAccountConstraints>) -> Result<()> {
        instructions::initialize_vault::handler(context)
    }

    pub fn withdraw_cnft<'info>(
        context: Context<'info, WithdrawCnftAccountConstraints<'info>>,
        root: [u8; 32],
        data_hash: [u8; 32],
        creator_hash: [u8; 32],
        nonce: u64,
        index: u32,
    ) -> Result<()> {
        instructions::withdraw_cnft::handler(context, root, data_hash, creator_hash, nonce, index)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn withdraw_two_cnfts<'info>(
        context: Context<'info, WithdrawTwoCnftsAccountConstraints<'info>>,
        root1: [u8; 32],
        data_hash1: [u8; 32],
        creator_hash1: [u8; 32],
        nonce1: u64,
        index1: u32,
        proof_1_length: u8,
        root2: [u8; 32],
        data_hash2: [u8; 32],
        creator_hash2: [u8; 32],
        nonce2: u64,
        index2: u32,
        proof_2_length: u8,
    ) -> Result<()> {
        instructions::withdraw_two_cnfts::handler(
            context,
            root1,
            data_hash1,
            creator_hash1,
            nonce1,
            index1,
            proof_1_length,
            root2,
            data_hash2,
            creator_hash2,
            nonce2,
            index2,
            proof_2_length,
        )
    }
}
