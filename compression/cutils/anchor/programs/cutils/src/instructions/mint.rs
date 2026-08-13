use crate::bubblegum_types::{
    Collection, Creator, MetadataArgs, MintToCollectionV1InstructionArgs, TokenProgramVersion,
    TokenStandard, MINT_TO_COLLECTION_V1_DISCRIMINATOR,
};
use crate::*;
use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    program::invoke,
};
use borsh::BorshSerialize;

#[derive(Accounts)]
#[instruction(params: MintParams)]
pub struct MintAccountConstraints {
    pub payer: Signer,

    #[account(
        mut,
        seeds = [merkle_tree.address().as_ref()],
        seeds::program = bubblegum_program.address(),
        bump,
    )]
    /// CHECK: This account is modified in the downstream program
    pub tree_authority: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub leaf_owner: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub leaf_delegate: UncheckedAccount,

    #[account(mut)]
    /// CHECK: Written by the Bubblegum/Account Compression CPI (the mint
    /// appends a leaf and updates the tree root); validated downstream.
    pub merkle_tree: UncheckedAccount,

    pub tree_delegate: Signer,

    pub collection_authority: Signer,

    /// CHECK: Optional collection authority record PDA.
    /// If there is no collection authority record PDA then
    /// this must be the Bubblegum program address.
    pub collection_authority_record_pda: UncheckedAccount,

    /// CHECK: This account is checked in the instruction
    pub collection_mint: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account is checked in the instruction
    pub collection_metadata: UncheckedAccount,

    /// CHECK: This account is checked in the instruction
    pub edition_account: UncheckedAccount,

    /// CHECK: This is just used as a signing PDA.
    pub bubblegum_signer: UncheckedAccount,

    /// CHECK: This account is neither written to nor read from.
    pub log_wrapper: UncheckedAccount,
    pub compression_program: Program<SPLCompression>,
    /// CHECK: This account is neither written to nor read from.
    pub token_metadata_program: UncheckedAccount,
    /// CHECK: This account is neither written to nor read from.
    pub bubblegum_program: UncheckedAccount,
    pub system_program: Program<System>,
}

#[derive(Clone, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct MintParams {
    uri: String,
}

// `with_capacity` + push is intentional here: it documents the exact 16-account
// MintToCollectionV1 layout in CPI order, so allow clippy's vec_init_then_push.
#[allow(clippy::vec_init_then_push)]
pub fn handle_mint<'info>(
    context: &mut Context<'info, MintAccountConstraints<'info>>,
    params: MintParams,
) -> Result<()> {
    // Build MintToCollectionV1 instruction data
    let args = MintToCollectionV1InstructionArgs {
        metadata: MetadataArgs {
            name: "BURGER".to_string(),
            symbol: "BURG".to_string(),
            uri: params.uri,
            creators: vec![Creator {
                address: *context.accounts.collection_authority.address(),
                verified: false,
                share: 100,
            }],
            seller_fee_basis_points: 0,
            primary_sale_happened: false,
            is_mutable: false,
            edition_nonce: Some(0),
            uses: None,
            collection: Some(Collection {
                verified: false,
                key: *context.accounts.collection_mint.address(),
            }),
            token_program_version: TokenProgramVersion::Original,
            token_standard: Some(TokenStandard::NonFungible),
        },
    };

    let mut data = MINT_TO_COLLECTION_V1_DISCRIMINATOR.to_vec();
    args.serialize(&mut data)?;

    // Build account metas matching MintToCollectionV1 instruction layout
    let mut accounts = Vec::with_capacity(16);
    accounts.push(AccountMeta::new(
        *context.accounts.tree_authority.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.leaf_owner.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.leaf_delegate.address(),
        false,
    ));
    accounts.push(AccountMeta::new(*context.accounts.merkle_tree.address(), false));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.payer.address(),
        true,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.tree_delegate.address(),
        true,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.collection_authority.address(),
        true,
    ));
    // collection_authority_record_pda - pass as-is
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.collection_authority_record_pda.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.collection_mint.address(),
        false,
    ));
    accounts.push(AccountMeta::new(
        *context.accounts.collection_metadata.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.edition_account.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.bubblegum_signer.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.log_wrapper.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.compression_program.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.token_metadata_program.address(),
        false,
    ));
    accounts.push(AccountMeta::new_readonly(
        *context.accounts.system_program.address(),
        false,
    ));

    let instruction = Instruction {
        program_id: MPL_BUBBLEGUM_ID,
        accounts,
        data,
    };

    // Gather all account infos for the CPI
    let account_infos = vec![
        context.accounts.bubblegum_program.cpi_handle_mut(),
        context.accounts.tree_authority.cpi_handle_mut(),
        context.accounts.leaf_owner.cpi_handle_mut(),
        context.accounts.leaf_delegate.cpi_handle_mut(),
        context.accounts.merkle_tree.cpi_handle_mut(),
        context.accounts.payer.cpi_handle_mut(),
        context.accounts.tree_delegate.cpi_handle_mut(),
        context.accounts.collection_authority.cpi_handle_mut(),
        context
            .accounts
            .collection_authority_record_pda
            .cpi_handle_mut(),
        context.accounts.collection_mint.cpi_handle_mut(),
        context.accounts.collection_metadata.cpi_handle_mut(),
        context.accounts.edition_account.cpi_handle_mut(),
        context.accounts.bubblegum_signer.cpi_handle_mut(),
        context.accounts.log_wrapper.cpi_handle_mut(),
        context.accounts.compression_program.cpi_handle_mut(),
        context.accounts.token_metadata_program.cpi_handle_mut(),
        context.accounts.system_program.cpi_handle_mut(),
    ];

    invoke(&instruction, &account_infos)?;

    Ok(())
}
