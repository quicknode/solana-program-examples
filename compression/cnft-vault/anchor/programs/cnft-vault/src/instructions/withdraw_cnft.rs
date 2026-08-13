use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::AccountMeta, program::invoke_signed};

use crate::error::VaultError;
use crate::state::{Vault, VAULT_SEED};
use crate::{build_transfer_instruction, SPLCompression, TransferArgs, MPL_BUBBLEGUM_ID};

#[derive(Accounts)]
pub struct WithdrawCnftAccountConstraints {
    /// The stored vault authority. Only this signer may withdraw.
    pub authority: Signer,

    // The vault PDA owns the cNFTs (as Bubblegum leaf owner) and signs the
    // transfer CPI via invoke_signed.
    #[account(
        seeds = [VAULT_SEED],
        bump = vault.bump,
        has_one = authority @ VaultError::InvalidWithdrawAuthority,
    )]
    pub vault: BorshAccount<Vault>,

    #[account(mut)]
    #[account(
        seeds = [merkle_tree.address().as_ref()],
        bump,
        seeds::program = bubblegum_program.address()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub new_leaf_owner: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account is modified in the downstream program
    pub merkle_tree: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub log_wrapper: UncheckedAccount,

    pub compression_program: Program<SPLCompression>,

    // Pin the bubblegum program account to the known mpl-bubblegum id. Without
    // this constraint the caller could pass any account to the CPI.
    /// CHECK: address constrained to the mpl-bubblegum program id.
    #[account(address = MPL_BUBBLEGUM_ID)]
    pub bubblegum_program: UncheckedAccount,

    pub system_program: Program<System>,
}

pub fn handler<'info>(
    context: &mut Context<'info, WithdrawCnftAccountConstraints<'info>>,
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
) -> Result<()> {
    msg!(
        "attempting to send nft {} from tree {}",
        index,
        context.accounts.merkle_tree.address()
    );

    let proof_metas: Vec<AccountMeta> = context
        .remaining_accounts
        .iter()
        .map(|acc| AccountMeta::new_readonly(acc.address(), false))
        .collect();

    let instruction = build_transfer_instruction(
        context.accounts.tree_authority.address(),
        context.accounts.vault.address(),
        context.accounts.vault.address(),
        context.accounts.new_leaf_owner.address(),
        context.accounts.merkle_tree.address(),
        context.accounts.log_wrapper.address(),
        context.accounts.compression_program.address(),
        context.accounts.system_program.address(),
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
        context.accounts.bubblegum_program.cpi_handle_mut(),
        context.accounts.tree_authority.cpi_handle_mut(),
        context.accounts.vault.cpi_handle_mut(),
        context.accounts.new_leaf_owner.cpi_handle_mut(),
        context.accounts.merkle_tree.cpi_handle_mut(),
        context.accounts.log_wrapper.cpi_handle_mut(),
        context.accounts.compression_program.cpi_handle_mut(),
        context.accounts.system_program.cpi_handle_mut(),
    ];
    for acc in context.remaining_accounts().iter() {
        account_infos.push(acc.cpi_handle_mut());
    }

    invoke_signed(
        &instruction,
        &account_infos,
        &[&[VAULT_SEED, &[context.accounts.vault.bump]]],
    )?;

    Ok(())
}
