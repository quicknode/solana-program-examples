use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::AccountMeta, program::invoke_signed};

use crate::error::VaultError;
use crate::state::{Vault, VAULT_SEED};
use crate::{build_transfer_instruction, SPLCompression, TransferArgs, MPL_BUBBLEGUM_ID};

#[derive(Accounts)]
pub struct WithdrawTwoCnftsAccountConstraints<'info> {
    /// The stored vault authority. Only this signer may withdraw.
    pub authority: Signer<'info>,

    // The vault PDA owns the cNFTs (as Bubblegum leaf owner) and signs both
    // transfer CPIs via invoke_signed.
    #[account(
        seeds = [VAULT_SEED],
        bump = vault.bump,
        has_one = authority @ VaultError::InvalidWithdrawAuthority,
    )]
    pub vault: Account<'info, Vault>,

    #[account(mut)]
    #[account(
        seeds = [merkle_tree1.key().as_ref()],
        bump,
        seeds::program = bubblegum_program.key()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority1: UncheckedAccount<'info>,

    /// CHECK: This account is neither written to nor read from.
    pub new_leaf_owner1: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: This account is modified in the downstream program
    pub merkle_tree1: UncheckedAccount<'info>,

    #[account(mut)]
    #[account(
        seeds = [merkle_tree2.key().as_ref()],
        bump,
        seeds::program = bubblegum_program.key()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority2: UncheckedAccount<'info>,

    /// CHECK: This account is neither written to nor read from.
    pub new_leaf_owner2: UncheckedAccount<'info>,

    #[account(mut)]
    /// CHECK: This account is modified in the downstream program
    pub merkle_tree2: UncheckedAccount<'info>,

    /// CHECK: This account is neither written to nor read from.
    pub log_wrapper: UncheckedAccount<'info>,

    pub compression_program: Program<'info, SPLCompression>,

    // Pin the bubblegum program account to the known mpl-bubblegum id. Without
    // this constraint the caller could pass any account to the two CPI calls.
    /// CHECK: address constrained to the mpl-bubblegum program id.
    #[account(address = MPL_BUBBLEGUM_ID)]
    pub bubblegum_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[allow(clippy::too_many_arguments)]
pub fn handler<'info>(
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
    let merkle_tree1 = context.accounts.merkle_tree1.key();
    let merkle_tree2 = context.accounts.merkle_tree2.key();
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
            .is_some_and(|total| total == context.remaining_accounts.len()),
        VaultError::ProofLengthMismatch
    );

    let signer_seeds: &[&[u8]] = &[VAULT_SEED, &[context.accounts.vault.bump]];

    // Split remaining accounts into proof1 and proof2
    let (proof1_accounts, proof2_accounts) = context.remaining_accounts.split_at(proof_1_length);

    let proof1_metas: Vec<AccountMeta> = proof1_accounts
        .iter()
        .map(|acc| AccountMeta::new_readonly(acc.key(), false))
        .collect();

    let proof2_metas: Vec<AccountMeta> = proof2_accounts
        .iter()
        .map(|acc| AccountMeta::new_readonly(acc.key(), false))
        .collect();

    // Withdraw cNFT#1
    msg!("withdrawing cNFT#1");
    let instruction1 = build_transfer_instruction(
        context.accounts.tree_authority1.key(),
        context.accounts.vault.key(),
        context.accounts.vault.key(),
        context.accounts.new_leaf_owner1.key(),
        context.accounts.merkle_tree1.key(),
        context.accounts.log_wrapper.key(),
        context.accounts.compression_program.key(),
        context.accounts.system_program.key(),
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
        context.accounts.bubblegum_program.to_account_info(),
        context.accounts.tree_authority1.to_account_info(),
        context.accounts.vault.to_account_info(),
        context.accounts.new_leaf_owner1.to_account_info(),
        context.accounts.merkle_tree1.to_account_info(),
        context.accounts.log_wrapper.to_account_info(),
        context.accounts.compression_program.to_account_info(),
        context.accounts.system_program.to_account_info(),
    ];
    for acc in proof1_accounts.iter() {
        account_infos1.push(acc.to_account_info());
    }

    invoke_signed(&instruction1, &account_infos1, &[signer_seeds])?;

    // Withdraw cNFT#2
    msg!("withdrawing cNFT#2");
    let instruction2 = build_transfer_instruction(
        context.accounts.tree_authority2.key(),
        context.accounts.vault.key(),
        context.accounts.vault.key(),
        context.accounts.new_leaf_owner2.key(),
        context.accounts.merkle_tree2.key(),
        context.accounts.log_wrapper.key(),
        context.accounts.compression_program.key(),
        context.accounts.system_program.key(),
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
        context.accounts.bubblegum_program.to_account_info(),
        context.accounts.tree_authority2.to_account_info(),
        context.accounts.vault.to_account_info(),
        context.accounts.new_leaf_owner2.to_account_info(),
        context.accounts.merkle_tree2.to_account_info(),
        context.accounts.log_wrapper.to_account_info(),
        context.accounts.compression_program.to_account_info(),
        context.accounts.system_program.to_account_info(),
    ];
    for acc in proof2_accounts.iter() {
        account_infos2.push(acc.to_account_info());
    }

    invoke_signed(&instruction2, &account_infos2, &[signer_seeds])?;

    msg!("successfully sent cNFTs");
    Ok(())
}
