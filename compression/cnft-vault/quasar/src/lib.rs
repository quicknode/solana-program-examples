#![cfg_attr(not(test), no_std)]

use quasar_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;
use instructions::*;
#[cfg(test)]
mod tests;

/// mpl-bubblegum Transfer instruction discriminator.
const TRANSFER_DISCRIMINATOR: [u8; 8] = [163, 52, 200, 231, 140, 3, 69, 186];

/// mpl-bubblegum program ID (BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY).
const MPL_BUBBLEGUM_ID: Address = Address::new_from_array([
    0x98, 0x8b, 0x80, 0xeb, 0x79, 0x35, 0x28, 0x69, 0xb2, 0x24, 0x74, 0x5f, 0x59, 0xdd, 0xbf, 0x8a,
    0x26, 0x58, 0xca, 0x13, 0xdc, 0x68, 0x81, 0x21, 0x26, 0x35, 0x1c, 0xae, 0x07, 0xc1, 0xa5, 0xa5,
]);

/// SPL Account Compression program ID (cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK).
const SPL_ACCOUNT_COMPRESSION_ID: Address = Address::new_from_array([
    0x09, 0x2a, 0x13, 0xee, 0x95, 0xc4, 0x1c, 0xba, 0x08, 0xa6, 0x7f, 0x5a, 0xc6, 0x7e, 0x8d, 0xf7,
    0xe1, 0xda, 0x11, 0x62, 0x5e, 0x1d, 0x64, 0x13, 0x7f, 0x8f, 0x4f, 0x23, 0x83, 0x03, 0x7f, 0x14,
]);

declare_id!("Fd4iwpPWaCU8BNwGQGtvvrcvG4Tfizq3RgLm8YLBJX6D");

#[program]
mod quasar_cnft_vault {
    use super::*;

    /// Withdraw a single compressed NFT from the vault PDA. Only the
    /// authority stored by initialize_vault may sign this.
    ///
    /// The Bubblegum Transfer args arrive as typed instruction arguments:
    /// 0.1.0 clears `ctx.data` after decoding declared args (the instruction
    /// argument zero-copy boundary), so the pre-0.1.0 raw-tail pattern reads
    /// an empty slice. Only the variable-length proof stays dynamic, as
    /// remaining accounts.
    #[instruction(discriminator = 0)]
    pub fn withdraw_cnft(
        ctx: CtxWithRemaining<WithdrawCnftAccountConstraints>,
        root: [u8; 32],
        data_hash: [u8; 32],
        creator_hash: [u8; 32],
        nonce: u64,
        index: u32,
    ) -> Result<(), ProgramError> {
        let remaining = ctx.remaining_accounts();
        let vault_bump = ctx.bumps.vault;
        instructions::handle_withdraw_cnft(
            &mut ctx.accounts,
            root,
            data_hash,
            creator_hash,
            nonce,
            index,
            remaining,
            vault_bump,
        )
    }

    /// Withdraw two compressed NFTs from the vault PDA in a single
    /// transaction. Only the authority stored by initialize_vault may sign
    /// this. The two proofs share the remaining-accounts region; the proof
    /// lengths say where to split it.
    #[instruction(discriminator = 1)]
    pub fn withdraw_two_cnfts(
        ctx: CtxWithRemaining<WithdrawTwoCnftsAccountConstraints>,
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
    ) -> Result<(), ProgramError> {
        let remaining = ctx.remaining_accounts();
        let vault_bump = ctx.bumps.vault;
        instructions::handle_withdraw_two_cnfts(
            &mut ctx.accounts,
            instructions::TransferArgs {
                root: root1,
                data_hash: data_hash1,
                creator_hash: creator_hash1,
                nonce: nonce1,
                index: index1,
            },
            proof_1_length,
            instructions::TransferArgs {
                root: root2,
                data_hash: data_hash2,
                creator_hash: creator_hash2,
                nonce: nonce2,
                index: index2,
            },
            proof_2_length,
            remaining,
            vault_bump,
        )
    }

    /// Create the vault PDA and store the signer as its withdraw authority.
    #[instruction(discriminator = 2)]
    pub fn initialize_vault(
        ctx: Ctx<InitializeVaultAccountConstraints>,
    ) -> Result<(), ProgramError> {
        let vault_bump = ctx.bumps.vault;
        instructions::handle_initialize_vault(&mut ctx.accounts, vault_bump)
    }
}
