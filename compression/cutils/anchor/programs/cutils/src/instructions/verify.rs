use crate::bubblegum_types::{get_asset_id, leaf_schema_v1_hash};
use crate::*;
use anchor_lang::solana_program::instruction::{AccountMeta, Instruction};

#[derive(Accounts)]
#[instruction(params: VerifyParams)]
pub struct VerifyAccountConstraints {
    pub leaf_owner: Signer,

    /// CHECK: This account is neither written to nor read from.
    pub leaf_delegate: UncheckedAccount,

    /// CHECK: Read by the SPL Account Compression verify_leaf CPI, which
    /// validates the proof against this tree's stored root.
    pub merkle_tree: UncheckedAccount,

    pub compression_program: Program<SPLCompression>,
}

#[derive(Clone, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct VerifyParams {
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
}

/// spl-account-compression `verify_leaf` instruction discriminator:
/// sha256("global:verify_leaf")[..8]. Precomputed because hashing a constant
/// at runtime burns compute for no benefit.
const VERIFY_LEAF_DISCRIMINATOR: [u8; 8] = [124, 220, 22, 223, 104, 10, 250, 224];

pub fn handle_verify<'info>(
    context: &mut Context<'info, VerifyAccountConstraints<'info>>,
    params: &VerifyParams,
) -> Result<()> {
    let asset_id = get_asset_id(&context.accounts.merkle_tree.address(), params.nonce);
    let leaf_hash = leaf_schema_v1_hash(
        &asset_id,
        &context.accounts.leaf_owner.address(),
        &context.accounts.leaf_delegate.address(),
        params.nonce,
        &params.data_hash,
        &params.creator_hash,
    );

    // Build verify_leaf instruction manually because spl-account-compression 1.0.0
    // depends on solana-program 2.x which is incompatible with Anchor 1.0's solana 3.x
    // types. Once a compatible version is available, replace this with the CPI wrapper.
    let mut accounts = vec![AccountMeta::new_readonly(
        *context.accounts.merkle_tree.address(),
        false,
    )];
    for acc in context.remaining_accounts().iter() {
        accounts.push(AccountMeta::new_readonly(acc.address(), false));
    }

    let mut data = VERIFY_LEAF_DISCRIMINATOR.to_vec();
    data.extend_from_slice(&params.root);
    data.extend_from_slice(&leaf_hash);
    data.extend_from_slice(&params.index.to_le_bytes());

    let mut account_infos = vec![context.accounts.merkle_tree.cpi_handle_mut()];
    for acc in context.remaining_accounts().iter() {
        account_infos.push(acc.cpi_handle_mut());
    }

    anchor_lang::solana_program::program::invoke(
        &Instruction {
            program_id: *context.accounts.compression_program.address(),
            accounts,
            data,
        },
        &account_infos,
    )?;

    Ok(())
}
