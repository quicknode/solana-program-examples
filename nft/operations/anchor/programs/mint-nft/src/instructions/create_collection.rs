use anchor_lang::prelude::*;
use anchor_spl::mint;
use anchor_spl::{
    associated_token::AssociatedToken,
    metadata::Metadata,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};

// v2's anchor-spl wraps these CPIs in terms of `CpiHandle`s, so the raw
// mpl-token-metadata `*Cpi` builders (which want `&AccountInfo`) are not
// usable here.
use anchor_spl::metadata::{
    create_master_edition_v3, create_metadata_accounts_v3,
    mpl_token_metadata::types::{CollectionDetails, Creator, DataV2},
    CreateMasterEditionV3, CreateMetadataAccountsV3,
};

use super::validate_metadata_strings;

#[derive(Accounts)]
pub struct CreateCollectionAccountConstraints {
    #[account(mut)]
    pub user: Signer,

    #[account(
        init,
        payer = user,
        mint::decimals = 0,
        mint::authority = mint_authority,
        mint::freeze_authority = mint_authority,
    )]
    pub mint: Account<Mint>,

    #[account(
        seeds = [b"authority"],
        bump,
    )]
    /// CHECK: This account is not initialized and is being used for signing purposes only
    pub mint_authority: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub metadata: UncheckedAccount,

    #[account(mut)]
    /// CHECK: This account will be initialized by the metaplex program
    pub master_edition: UncheckedAccount,

    #[account(
        init,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user
    )]
    pub destination: Account<TokenAccount>,

    pub system_program: Program<System>,
    pub token_program: Program<Token>,
    pub associated_token_program: Program<AssociatedToken>,
    pub token_metadata_program: Program<Metadata>,
}

/// Creates a collection NFT with caller-supplied metadata.
///
/// `name`, `symbol`, and `uri` are validated against the Metaplex Token
/// Metadata limits (32, 10, and 200 bytes respectively).
pub fn handle_create_collection(
    accounts: &mut CreateCollectionAccountConstraints,
    bumps: &CreateCollectionAccountConstraintsBumps,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    validate_metadata_strings(&name, &symbol, &uri)?;

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
    msg!("Collection NFT minted!");

    let creator = vec![Creator {
        address: *accounts.mint_authority.address(),
        verified: true,
        share: 100,
    }];

    // Read-only slots use the wrapper's own `cpi_handle()`: it takes `&self`,
    // so one account can fill several of them, and on a data account it also
    // relaxes the runtime borrow check that a hand-built
    // `CpiHandle::readonly(&copied_view)` would still trip.
    create_metadata_accounts_v3(
        CpiContext::new_with_signer(
            accounts.token_metadata_program.address(),
            CreateMetadataAccountsV3 {
                metadata: accounts.metadata.cpi_handle_mut(),
                mint: accounts.mint.cpi_handle(),
                mint_authority: accounts.mint_authority.cpi_handle(),
                payer: accounts.user.cpi_handle_mut(),
                update_authority: accounts.mint_authority.cpi_handle(),
                system_program: accounts.system_program.cpi_handle(),
                update_authority_is_signer: true,
            },
            signer_seeds,
        ),
        DataV2 {
            name,
            symbol,
            uri,
            seller_fee_basis_points: 0,
            creators: Some(creator),
            collection: None,
            uses: None,
        },
        true,
        Some(CollectionDetails::V1 { size: 0 }),
    )?;
    msg!("Metadata Account created!");

    create_master_edition_v3(
        CpiContext::new_with_signer(
            accounts.token_metadata_program.address(),
            CreateMasterEditionV3 {
                edition: accounts.master_edition.cpi_handle_mut(),
                mint: accounts.mint.cpi_handle_mut(),
                update_authority: accounts.mint_authority.cpi_handle(),
                mint_authority: accounts.mint_authority.cpi_handle(),
                payer: accounts.user.cpi_handle_mut(),
                metadata: accounts.metadata.cpi_handle_mut(),
                token_program: accounts.token_program.cpi_handle(),
                system_program: accounts.system_program.cpi_handle(),
            },
            signer_seeds,
        ),
        Some(0),
    )?;
    msg!("Master Edition Account created");

    Ok(())
}
