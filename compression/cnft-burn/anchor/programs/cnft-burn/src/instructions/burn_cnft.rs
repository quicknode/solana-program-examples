use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};
use borsh::BorshSerialize;

use crate::{MPL_BUBBLEGUM_ID, SPLCompression};

/// Burn instruction discriminator from mpl-bubblegum
const BURN_DISCRIMINATOR: [u8; 8] = [116, 110, 29, 56, 107, 219, 42, 93];

/// Instruction arguments for mpl-bubblegum Burn, serialized with borsh
#[derive(BorshSerialize)]
struct BurnArgs {
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
}

#[derive(Accounts)]
pub struct BurnCnftAccountConstraints<'info> {
    #[account(mut)]
    pub leaf_owner: Signer<'info>,
    #[account(mut)]
    #[account(
        seeds = [merkle_tree.key().as_ref()],
        bump,
        seeds::program = bubblegum_program.key()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority: UncheckedAccount<'info>,
    #[account(mut)]
    /// CHECK: Written by the Bubblegum/Account Compression CPI (the burn
    /// replaces the leaf and updates the tree root); validated downstream
    /// by those programs.
    pub merkle_tree: UncheckedAccount<'info>,
    /// CHECK: This account is neither written to nor read from.
    pub log_wrapper: UncheckedAccount<'info>,
    pub compression_program: Program<'info, SPLCompression>,
    // Pin the bubblegum program account to the known mpl-bubblegum id. Without
    // this constraint the caller could pass any account and a malicious one
    // could short-circuit the CPI in unexpected ways.
    /// CHECK: address constrained to the mpl-bubblegum program id.
    #[account(address = MPL_BUBBLEGUM_ID)]
    pub bubblegum_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

pub fn handle_burn_cnft<'info>(
    context: Context<'info, BurnCnftAccountConstraints<'info>>,
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
) -> Result<()> {
    // Build instruction data: discriminator + borsh-serialized args
    let args = BurnArgs {
        root,
        data_hash,
        creator_hash,
        nonce,
        index,
    };
    let mut data = BURN_DISCRIMINATOR.to_vec();
    args.serialize(&mut data)?;

    // Build account metas matching mpl-bubblegum Burn instruction layout
    let mut accounts = Vec::with_capacity(7 + context.remaining_accounts.len());
    accounts.push(AccountMeta::new_readonly(
        context.accounts.tree_authority.key(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        context.accounts.leaf_owner.key(),
        true,
    ));
    // leaf_delegate = leaf_owner, not a signer in this call
    accounts.push(AccountMeta::new_readonly(
        context.accounts.leaf_owner.key(),
        false,
    ));
    accounts.push(AccountMeta::new(context.accounts.merkle_tree.key(), false));
    accounts.push(AccountMeta::new_readonly(
        context.accounts.log_wrapper.key(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        context.accounts.compression_program.key(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        context.accounts.system_program.key(),
        false,
    ));
    // Append remaining accounts (proof nodes)
    for acc in context.remaining_accounts.iter() {
        accounts.push(AccountMeta::new_readonly(acc.key(), false));
    }

    let instruction = Instruction {
        program_id: MPL_BUBBLEGUM_ID,
        accounts,
        data,
    };

    // Gather all account infos for the CPI
    let mut account_infos = vec![
        context.accounts.bubblegum_program.to_account_info(),
        context.accounts.tree_authority.to_account_info(),
        context.accounts.leaf_owner.to_account_info(),
        context.accounts.merkle_tree.to_account_info(),
        context.accounts.log_wrapper.to_account_info(),
        context.accounts.compression_program.to_account_info(),
        context.accounts.system_program.to_account_info(),
    ];
    for acc in context.remaining_accounts.iter() {
        account_infos.push(acc.to_account_info());
    }

    invoke(&instruction, &account_infos)?;

    Ok(())
}
