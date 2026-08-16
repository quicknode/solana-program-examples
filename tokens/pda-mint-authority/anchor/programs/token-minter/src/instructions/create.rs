// In this example the same PDA is used as both the address of the mint account and the mint authority
// This is to demonstrate that the same PDA can be used for both the address of an account and CPI signing
use {
    anchor_lang::prelude::*,
    // `Mint::LEN` comes from `Pack`, which anchor-spl does not re-export.
    solana_program_pack::Pack,
    anchor_lang::system_program::{create_account, CreateAccount},
    anchor_spl::{
        metadata::{
            create_metadata_accounts_v3, mpl_token_metadata::types::DataV2,
            CreateMetadataAccountsV3, Metadata,
        },
        token::{initialize_mint2, spl_token::state::Mint as MintState, InitializeMint2, Token},
    },
};

#[derive(Accounts)]
pub struct CreateTokenAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    // Create mint account. The same PDA is both the account's address and its
    // mint/freeze authority — which is the point of this example, and which v2
    // cannot express as an `init` constraint: `mint::authority` has to name a
    // sibling field, and referencing the account being initialized is rejected
    // at macro-expansion time. So the mint is created by hand in
    // `handle_create_token` below.
    /// CHECK: created and initialized as a mint by this instruction.
    #[account(
        mut,
        seeds = [b"mint"],
        bump,
    )]
    pub mint_account: UncheckedAccount,

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
    // PDA signer seeds
    let signer_seeds: &[&[&[u8]]] = &[&[b"mint", &[context.bumps.mint_account]]];

    msg!("Creating mint account");

    // Allocate and initialize the mint, naming the mint PDA as both mint and
    // freeze authority. `create_account` is signed by the PDA because the PDA
    // is the account being created.
    let mint_address = *context.accounts.mint_account.address();
    let lamports = Rent::get()?.try_minimum_balance(MintState::LEN)?;
    create_account(
        CpiContext::new(
            context.accounts.system_program.address(),
            CreateAccount {
                from: context.accounts.payer.cpi_handle_mut(),
                to: context.accounts.mint_account.cpi_handle_mut(),
            },
        )
        .with_signer(signer_seeds),
        lamports,
        MintState::LEN as u64,
        context.accounts.token_program.address(),
    )?;

    initialize_mint2(
        CpiContext::new(
            context.accounts.token_program.address(),
            InitializeMint2 {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        9,
        &mint_address,
        Some(&mint_address),
    )?;

    msg!("Creating metadata account");

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
        None,  // Collection details
    )?;

    msg!("Token created successfully.");

    Ok(())
}
