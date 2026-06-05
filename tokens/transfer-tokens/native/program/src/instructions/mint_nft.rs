use {
    crate::bridge::{bridge_instruction, to_mpl},
    mpl_token_metadata::instructions::{
        CreateMasterEditionV3, CreateMasterEditionV3InstructionArgs,
    },
    solana_program::{
        account_info::{next_account_info, AccountInfo},
        entrypoint::ProgramResult,
        msg,
        program::invoke,
    },
    spl_associated_token_account_interface::instruction as associated_token_account_instruction,
    spl_token_interface::instruction as token_instruction,
};

pub fn mint_nft(accounts: &[AccountInfo]) -> ProgramResult {
    let accounts_iter = &mut accounts.iter();

    let mint_account = next_account_info(accounts_iter)?;
    let metadata_account = next_account_info(accounts_iter)?;
    let edition_account = next_account_info(accounts_iter)?;
    let mint_authority = next_account_info(accounts_iter)?;
    let associated_token_account = next_account_info(accounts_iter)?;
    let payer = next_account_info(accounts_iter)?;
    let rent = next_account_info(accounts_iter)?;
    let system_program = next_account_info(accounts_iter)?;
    let token_program = next_account_info(accounts_iter)?;
    let associated_token_program = next_account_info(accounts_iter)?;
    let token_metadata_program = next_account_info(accounts_iter)?;

    if associated_token_account.lamports() == 0 {
        msg!("Creating associated token account...");
        invoke(
            &associated_token_account_instruction::create_associated_token_account(
                payer.key,
                payer.key,
                mint_account.key,
                token_program.key,
            ),
            &[
                mint_account.clone(),
                associated_token_account.clone(),
                payer.clone(),
                system_program.clone(),
                token_program.clone(),
                associated_token_program.clone(),
            ],
        )?;
    } else {
        msg!("Associated token account exists.");
    }
    msg!("Associated Token Address: {}", associated_token_account.key);

    // Mint the NFT to the user's wallet
    //
    msg!("Minting NFT to associated token account...");
    invoke(
        &token_instruction::mint_to(
            token_program.key,
            mint_account.key,
            associated_token_account.key,
            mint_authority.key,
            &[mint_authority.key],
            1,
        )?,
        &[
            mint_account.clone(),
            mint_authority.clone(),
            associated_token_account.clone(),
            token_program.clone(),
        ],
    )?;

    // We can make this a Limited Edition NFT through Metaplex,
    //      which will disable minting by setting the Mint & Freeze Authorities to the
    //      Edition Account.
    //
    msg!("Creating edition account...");
    msg!("Edition account address: {}", edition_account.key);
    let create_edition_ix = CreateMasterEditionV3 {
        edition: to_mpl(edition_account.key),
        mint: to_mpl(mint_account.key),
        update_authority: to_mpl(mint_authority.key),
        mint_authority: to_mpl(mint_authority.key),
        payer: to_mpl(payer.key),
        metadata: to_mpl(metadata_account.key),
        token_program: to_mpl(token_program.key),
        system_program: to_mpl(system_program.key),
        rent: Some(to_mpl(rent.key)),
    }
    .instruction(CreateMasterEditionV3InstructionArgs {
        max_supply: Some(1),
    });
    invoke(
        &bridge_instruction(create_edition_ix),
        &[
            edition_account.clone(),
            metadata_account.clone(),
            mint_account.clone(),
            mint_authority.clone(),
            payer.clone(),
            token_metadata_program.clone(),
            token_program.clone(),
            system_program.clone(),
            rent.clone(),
        ],
    )?;

    msg!("NFT minted successfully.");

    Ok(())
}
