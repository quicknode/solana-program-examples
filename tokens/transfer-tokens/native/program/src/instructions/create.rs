use {
    crate::bridge::{bridge_instruction, to_mpl},
    borsh::{BorshDeserialize, BorshSerialize},
    mpl_token_metadata::{
        instructions::{CreateMetadataAccountV3, CreateMetadataAccountV3InstructionArgs},
        types::DataV2,
    },
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        msg,
        program::invoke,
        program_pack::Pack,
        rent::Rent,
        sysvar::Sysvar,
    },
    solana_system_interface::instruction as system_instruction,
    spl_token_interface::{instruction as token_instruction, state::Mint},
};

#[derive(BorshSerialize, BorshDeserialize, Debug)]
pub struct CreateTokenArgs {
    pub token_title: String,
    pub token_symbol: String,
    pub token_uri: String,
    pub decimals: u8,
}

pub fn create_token(accounts: &[AccountInfo], args: CreateTokenArgs) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let mint_account = next_account_info(accounts_iter)?;
    let mint_authority = next_account_info(accounts_iter)?;
    let metadata_account = next_account_info(accounts_iter)?;
    let payer = next_account_info(accounts_iter)?;
    let rent = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;
    let token_program = next_account_info(accounts_iter)?;
    let token_metadata_program = next_account_info(accounts_iter)?;

    // First create the account for the Mint
    //
    msg!("Creating mint account...");
    msg!("Mint: {}", mint_account.key);
    invoke(
        &system_instruction::create_account(
            payer.key,
            mint_account.key,
            (Rent::get()?).minimum_balance(Mint::LEN),
            Mint::LEN as u64,
            token_program.key,
        ),
        &[
            mint_account.clone(),
            payer.clone(),
            system_program.clone(),
            token_program.clone(),
        ],
    )?;

    // Now initialize that account as a Mint (standard Mint)
    //
    msg!("Initializing mint account...");
    msg!("Mint: {}", mint_account.key);
    invoke(
        &token_instruction::initialize_mint(
            token_program.key,
            mint_account.key,
            mint_authority.key,
            Some(mint_authority.key),
            args.decimals,
        )?,
        &[
            mint_account.clone(),
            mint_authority.clone(),
            token_program.clone(),
            rent.clone(),
        ],
    )?;

    // Now create the account for that Mint's metadata
    //
    msg!("Creating metadata account...");
    msg!("Metadata account address: {}", metadata_account.key);
    let create_metadata_ix = CreateMetadataAccountV3 {
        metadata: to_mpl(metadata_account.key),
        mint: to_mpl(mint_account.key),
        mint_authority: to_mpl(mint_authority.key),
        payer: to_mpl(payer.key),
        update_authority: (to_mpl(mint_authority.key), true),
        system_program: to_mpl(system_program.key),
        rent: Some(to_mpl(rent.key)),
    }
    .instruction(CreateMetadataAccountV3InstructionArgs {
        data: DataV2 {
            name: args.token_title,
            symbol: args.token_symbol,
            uri: args.token_uri,
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        },
        is_mutable: true,
        collection_details: None,
    });
    invoke(
        &bridge_instruction(create_metadata_ix),
        &[
            metadata_account.clone(),
            mint_account.clone(),
            mint_authority.clone(),
            payer.clone(),
            token_metadata_program.clone(),
            rent.clone(),
        ],
    )?;

    msg!("Token mint created successfully.");

    Ok(())
}
