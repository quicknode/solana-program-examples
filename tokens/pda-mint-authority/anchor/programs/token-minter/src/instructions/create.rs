// In this example the same PDA is used as both the address of the mint account and the mint authority
// This is to demonstrate that the same PDA can be used for both the address of an account and CPI signing
use {
    anchor_lang::prelude::*,
    anchor_spl::{
        metadata::{
            create_metadata_accounts_v3, mpl_token_metadata::types::DataV2,
            CreateMetadataAccountsV3, Metadata,
        },
        mint::{self, Mint},
        token::{self, Token},
    },
};

#[derive(Accounts)]
pub struct CreateTokenAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    // Create mint account
    // Same PDA as address of the account and mint/freeze authority
    #[account(
        init,
        seeds = [b"mint"],
        bump,
        payer = payer,
        mint::decimals = 9,
        mint::authority = mint_account,
        mint::freeze_authority = mint_account,

    )]
    pub mint_account: Account<Mint>,

    /// CHECK: Validate address by deriving pda
    #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.address().as_ref(), mint_account.address().as_ref()],
        bump,
        seeds::program = token_metadata_program.address(),
    )]
    pub metadata_account: UncheckedAccount,

    pub token_program: Program<Token>,
    pub token_metadata_program: Program<Metadata>,
    pub system_program: Program<System>,
    pub rent: Sysvar<Rent>,
}

pub fn handle_create_token(
    context: &mut Context<CreateTokenAccountConstraints>,
    token_name: String,
    token_symbol: String,
    token_uri: String,
) -> Result<()> {
    msg!("Creating metadata account");

    // PDA signer seeds
    let signer_seeds: &[&[&[u8]]] = &[&[b"mint", &[context.bumps.mint_account]]];

    // Cross Program Invocation (CPI) signed by PDA
    // Invoking the create_metadata_account_v3 instruction on the token metadata program
    create_metadata_accounts_v3(
        CpiContext::new(
            context.accounts.token_metadata_program.address(),
            CreateMetadataAccountsV3 {
                metadata: context.accounts.metadata_account.cpi_handle_mut(),
                mint: context.accounts.mint_account.cpi_handle(),
                mint_authority: context.accounts.mint_account.cpi_handle(), // PDA is mint authority
                update_authority: context.accounts.mint_account.cpi_handle(), // PDA is update authority
                payer: context.accounts.payer.cpi_handle_mut(),
                system_program: context.accounts.system_program.cpi_handle(),
                update_authority_is_signer: true,
            },
        )
        .with_signer(signer_seeds),
        DataV2 {
            name: token_name,
            symbol: token_symbol,
            uri: token_uri,
            seller_fee_basis_points: 0,
            creators: None,
            collection: None,
            uses: None,
        },
        false, // Is mutable
        true,  // Update authority is signer
        None,  // Collection details
    )?;

    msg!("Token created successfully.");

    Ok(())
}
