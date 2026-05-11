use anchor_lang::prelude::*;
use anchor_lang::solana_program::{
    instruction::AccountMeta,
    program::invoke_signed,
};

use crate::{build_transfer_instruction, TransferArgs, SPLCompression, MPL_BUBBLEGUM_ID};

#[derive(Accounts)]
pub struct WithdrawTwo<'info> {
    #[account(mut)]
    #[account(
        seeds = [merkle_tree1.key().as_ref()],
        bump,
        seeds::program = bubblegum_program.key()
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority1: UncheckedAccount<'info>,
    #[account(
        seeds = [b"cNFT-vault"],
        bump,
    )]
    /// CHECK: This account doesnt even exist (it is just the pda to sign)
    pub leaf_owner: UncheckedAccount<'info>,
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
    context: Context<'info, WithdrawTwo<'info>>,
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
    _proof_2_length: u8,
) -> Result<()> {
    let merkle_tree1 = context.accounts.merkle_tree1.key();
    let merkle_tree2 = context.accounts.merkle_tree2.key();
    msg!(
        "attempting to send nfts from trees {} and {}",
        merkle_tree1,
        merkle_tree2
    );

    let signer_seeds: &[&[u8]] = &[b"cNFT-vault", &[context.bumps.leaf_owner]];

    // Split remaining accounts into proof1 and proof2
    let (proof1_accounts, proof2_accounts) =
        context.remaining_accounts.split_at(proof_1_length as usize);

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
        context.accounts.leaf_owner.key(),
        context.accounts.leaf_owner.key(),
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
        context.accounts.leaf_owner.to_account_info(),
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
        context.accounts.leaf_owner.key(),
        context.accounts.leaf_owner.key(),
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
        context.accounts.leaf_owner.to_account_info(),
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
