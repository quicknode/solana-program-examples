use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::Metadata,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};

use anchor_spl::metadata::mpl_token_metadata::{
    instructions::{
        CreateMasterEditionV3Cpi, CreateMasterEditionV3CpiAccounts,
        CreateMasterEditionV3InstructionArgs, CreateMetadataAccountV3Cpi,
        CreateMetadataAccountV3CpiAccounts, CreateMetadataAccountV3InstructionArgs,
    },
    types::{Collection, Creator, DataV2},
};

use super::validate_metadata_strings;

#[derive(Accounts)]
pub struct MintNftAccountConstraints {
    #[account(mut)]
    pub owner: Signer,

    #[account(
        init,
        payer = owner,
        mint::decimals = 0,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
    )]
    pub mint: Account<Mint>,

    #[account(
        init,
        payer = owner,
        associated_token::mint = mint,
        associated_token::authority = owner
    )]
    pub destination: Account<TokenAccount>,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub metadata: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub master_edition: UncheckedAccount,

    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This is account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount,

    #[account(mut)]
    pub collection_mint: Account<Mint>,

    pub system_program: Program<System>,
    pub token_program: Program<Token>,
    pub associated_token_program: Program<AssociatedToken>,
    pub token_metadata_program: Program<Metadata>,
}

/// Mints an NFT into the collection with caller-supplied metadata.
///
/// `name`, `symbol`, and `uri` are validated against the Metaplex Token
/// Metadata limits (32, 10, and 200 bytes respectively). The collection
/// reference starts unverified; call `verify_collection` to verify it.
pub fn handle_mint_nft(
    accounts: &mut MintNftAccountConstraints,
    bumps: &MintNftAccountConstraintsBumps,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    validate_metadata_strings(&name, &symbol, &uri)?;

    let metadata = &accounts.metadata.cpi_handle_mut();
    let master_edition = &accounts.master_edition.cpi_handle_mut();
    let mint = &accounts.mint.cpi_handle_mut();
    let authority = &accounts.mint_authority.cpi_handle_mut();
    let payer = &accounts.owner.cpi_handle_mut();
    let system_program = &accounts.system_program.cpi_handle_mut();
    let spl_token_program = &accounts.token_program.cpi_handle_mut();
    let spl_metadata_program = &accounts.token_metadata_program.cpi_handle_mut();

    let seeds = &[&b"authority"[..], &[bumps.mint_authority]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = MintTo {
        mint: accounts.mint.cpi_handle_mut(),
        to: accounts.destination.cpi_handle_mut(),
        authority: accounts.mint_authority.cpi_handle(),
    };
    let cpi_ctx =
        CpiContext::new_with_signer(accounts.token_program.address(), cpi_accounts, signer_seeds);
    mint_to(cpi_ctx, 1)?;
    msg!("NFT minted!");

    let creator = vec![Creator {
        address: *accounts.mint_authority.address(),
        verified: true,
        share: 100,
    }];

    let metadata_account = CreateMetadataAccountV3Cpi::new(
        spl_metadata_program,
        CreateMetadataAccountV3CpiAccounts {
            metadata,
            mint,
            mint_authority: authority,
            payer,
            update_authority: (authority, true),
            system_program,
            rent: None,
        },
        CreateMetadataAccountV3InstructionArgs {
            data: DataV2 {
                name,
                symbol,
                uri,
                seller_fee_basis_points: 0,
                creators: Some(creator),
                collection: Some(Collection {
                    verified: false,
                    key: *accounts.collection_mint.address(),
                }),
                uses: None,
            },
            is_mutable: true,
            collection_details: None,
        },
    );
    metadata_account.invoke_signed(signer_seeds)?;

    let master_edition_account = CreateMasterEditionV3Cpi::new(
        spl_metadata_program,
        CreateMasterEditionV3CpiAccounts {
            edition: master_edition,
            update_authority: authority,
            mint_authority: authority,
            mint,
            payer,
            metadata,
            token_program: spl_token_program,
            system_program,
            rent: None,
        },
        CreateMasterEditionV3InstructionArgs {
            max_supply: Some(0),
        },
    );
    master_edition_account.invoke_signed(signer_seeds)?;

    Ok(())
}
