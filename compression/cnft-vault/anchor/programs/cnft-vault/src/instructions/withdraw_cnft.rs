use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::AccountMeta, program::invoke_signed};

use crate::error::VaultError;
use crate::state::{Vault, VAULT_SEED};
use crate::{build_transfer_instruction, SPLCompression, TransferArgs, MPL_BUBBLEGUM_ID};

#[derive(Accounts)]
pub struct WithdrawCnftAccountConstraints<'info> {
    /// The stored vault authority. Only this signer may withdraw.
    pub authority: Signer<'info>,

    // The vault PDA owns the cNFTs (as Bubblegum leaf owner) and signs the
    // transfer CPI via invoke_signed.
    #[account(
        seeds = [VAULT_SEED],
        bump = vault.bump,
        has_one = authority @ VaultError::InvalidWithdrawAuthority,
    )]
    pub vault: Account<'info, Vault>,

    #[account(mut)]
    #[account(
        seeds = [merkle_tree.key().as_ref()],
        bump,
        seeds::program = bubblegum_program.key()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority: UncheckedAccount<'info>,

    /// CHECK: This account is neither written to nor read from.
    pub new_leaf_owner: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: This account is modified in the downstream program
    pub merkle_tree: UncheckedAccount<'info>,

    /// CHECK: This account is neither written to nor read from.
    pub log_wrapper: UncheckedAccount<'info>,

    pub compression_program: Program<'info, SPLCompression>,

    // Pin the bubblegum program account to the known mpl-bubblegum id. Without
    // this constraint the caller could pass any account to the CPI.
    /// CHECK: address constrained to the mpl-bubblegum program id.
    #[account(address = MPL_BUBBLEGUM_ID)]
    pub bubblegum_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handler<'info>(
    context: Context<'info, WithdrawCnftAccountConstraints<'info>>,
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
) -> Result<()> {
    msg!(
        "attempting to send nft {} from tree {}",
        index,
        context.accounts.merkle_tree.key()
    );

    let proof_metas: Vec<AccountMeta> = context
        .remaining_accounts
        .iter()
        .map(|acc| AccountMeta::new_readonly(acc.key(), false))
        .collect();

    let instruction = build_transfer_instruction(
        context.accounts.tree_authority.key(),
        context.accounts.vault.key(),
        context.accounts.vault.key(),
        context.accounts.new_leaf_owner.key(),
        context.accounts.merkle_tree.key(),
        context.accounts.log_wrapper.key(),
        context.accounts.compression_program.key(),
        context.accounts.system_program.key(),
        &proof_metas,
        TransferArgs {
            root,
            data_hash,
            creator_hash,
            nonce,
            index,
        },
    )?;

    // Gather all account infos for the CPI
    let mut account_infos = vec![
        context.accounts.bubblegum_program.to_account_info(),
        context.accounts.tree_authority.to_account_info(),
        context.accounts.vault.to_account_info(),
        context.accounts.new_leaf_owner.to_account_info(),
        context.accounts.merkle_tree.to_account_info(),
        context.accounts.log_wrapper.to_account_info(),
        context.accounts.compression_program.to_account_info(),
        context.accounts.system_program.to_account_info(),
    ];
    for acc in context.remaining_accounts.iter() {
        account_infos.push(acc.to_account_info());
    }

    invoke_signed(
        &instruction,
        &account_infos,
        &[&[VAULT_SEED, &[context.accounts.vault.bump]]],
    )?;

    Ok(())
}
