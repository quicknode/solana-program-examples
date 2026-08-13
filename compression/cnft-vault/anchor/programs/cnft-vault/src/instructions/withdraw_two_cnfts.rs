use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::AccountMeta, program::invoke_signed};

use crate::error::VaultError;
use crate::state::{Vault, VAULT_SEED};
use crate::{build_transfer_instruction, SPLCompression, TransferArgs, MPL_BUBBLEGUM_ID};

#[derive(Accounts)]
pub struct WithdrawTwoCnftsAccountConstraints {
    /// The stored vault authority. Only this signer may withdraw.
    pub authority: Signer,

    // The vault PDA owns the cNFTs (as Bubblegum leaf owner) and signs both
    // transfer CPIs via invoke_signed.
    #[account(
        seeds = [VAULT_SEED],
        bump = vault.bump,
        has_one = authority @ VaultError::InvalidWithdrawAuthority,
    )]
    pub vault: BorshAccount<Vault>,

    #[account(mut)]
    #[account(
        seeds = [merkle_tree1.address().as_ref()],
        bump,
        seeds::program = bubblegum_program.address()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority1: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub new_leaf_owner1: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account is modified in the downstream program
    pub merkle_tree1: UncheckedAccount,

    #[account(mut)]
    #[account(
        seeds = [merkle_tree2.address().as_ref()],
        bump,
        seeds::program = bubblegum_program.address()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority2: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub new_leaf_owner2: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account is modified in the downstream program
    pub merkle_tree2: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub log_wrapper: UncheckedAccount,

    pub compression_program: Program<SPLCompression>,

    // Pin the bubblegum program account to the known mpl-bubblegum id. Without
    // this constraint the caller could pass any account to the two CPI calls.
    /// CHECK: address constrained to the mpl-bubblegum program id.
    #[account(address = MPL_BUBBLEGUM_ID)]
    pub bubblegum_program: UncheckedAccount,

    pub system_program: Program<System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler<'info>(
    context: &mut Context<'info, WithdrawTwoCnftsAccountConstraints<'info>>,
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
    let merkle_tree1 = context.accounts.merkle_tree1.address();
    let merkle_tree2 = context.accounts.merkle_tree2.address();
    msg!(
        "attempting to send nfts from trees {} and {}",
        merkle_tree1,
        merkle_tree2
    );

    // The proof lengths are client-supplied: bounds-check them against the
    // accounts actually provided before slicing, so adversarial input gets a
    // clean named error instead of a panic.
    let proof_1_length = proof_1_length as usize;
    let proof_2_length = proof_2_length as usize;
    require!(
        proof_1_length
            .checked_add(proof_2_length)
            .is_some_and(|total| total == context.remaining_accounts().len()),
        VaultError::ProofLengthMismatch
    );

    let signer_seeds: &[&[u8]] = &[VAULT_SEED, &[context.accounts.vault.bump]];

    // Split remaining accounts into proof1 and proof2
    let (proof1_accounts, proof2_accounts) = context.remaining_accounts().split_at(proof_1_length);

    let proof1_metas: Vec<AccountMeta> = proof1_accounts
        .iter()
        .map(|acc| AccountMeta::new_readonly(acc.address(), false))
        .collect();

    let proof2_metas: Vec<AccountMeta> = proof2_accounts
        .iter()
        .map(|acc| AccountMeta::new_readonly(acc.address(), false))
        .collect();

    // Withdraw cNFT#1
    msg!("withdrawing cNFT#1");
    let instruction1 = build_transfer_instruction(
        context.accounts.tree_authority1.address(),
        context.accounts.vault.address(),
        context.accounts.vault.address(),
        context.accounts.new_leaf_owner1.address(),
        context.accounts.merkle_tree1.address(),
        context.accounts.log_wrapper.address(),
        context.accounts.compression_program.address(),
        context.accounts.system_program.address(),
        &proof1_metas,
        TransferArgs {
            root: root1,
            data_hash: data_hash1,
            creator_hash: creator_hash1,
            nonce: nonce1,
            index: index1,
        },
    )?;

    let mut account_infos1 = vec![
        context.accounts.bubblegum_program.cpi_handle_mut(),
        context.accounts.tree_authority1.cpi_handle_mut(),
        context.accounts.vault.cpi_handle_mut(),
        context.accounts.new_leaf_owner1.cpi_handle_mut(),
        context.accounts.merkle_tree1.cpi_handle_mut(),
        context.accounts.log_wrapper.cpi_handle_mut(),
        context.accounts.compression_program.cpi_handle_mut(),
        context.accounts.system_program.cpi_handle_mut(),
    ];
    for acc in proof1_accounts.iter() {
        account_infos1.push(acc.cpi_handle_mut());
    }

    invoke_signed(&instruction1, &account_infos1, &[signer_seeds])?;

    // Withdraw cNFT#2
    msg!("withdrawing cNFT#2");
    let instruction2 = build_transfer_instruction(
        context.accounts.tree_authority2.address(),
        context.accounts.vault.address(),
        context.accounts.vault.address(),
        context.accounts.new_leaf_owner2.address(),
        context.accounts.merkle_tree2.address(),
        context.accounts.log_wrapper.address(),
        context.accounts.compression_program.address(),
        context.accounts.system_program.address(),
        &proof2_metas,
        TransferArgs {
            root: root2,
            data_hash: data_hash2,
            creator_hash: creator_hash2,
            nonce: nonce2,
            index: index2,
        },
    )?;

    let mut account_infos2 = vec![
        context.accounts.bubblegum_program.cpi_handle_mut(),
        context.accounts.tree_authority2.cpi_handle_mut(),
        context.accounts.vault.cpi_handle_mut(),
        context.accounts.new_leaf_owner2.cpi_handle_mut(),
        context.accounts.merkle_tree2.cpi_handle_mut(),
        context.accounts.log_wrapper.cpi_handle_mut(),
        context.accounts.compression_program.cpi_handle_mut(),
        context.accounts.system_program.cpi_handle_mut(),
    ];
    for acc in proof2_accounts.iter() {
        account_infos2.push(acc.cpi_handle_mut());
    }

    invoke_signed(&instruction2, &account_infos2, &[signer_seeds])?;

    msg!("successfully sent cNFTs");
    Ok(())
}
