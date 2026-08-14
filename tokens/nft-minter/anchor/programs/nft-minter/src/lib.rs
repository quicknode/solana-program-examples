use {
    anchor_lang::prelude::*,
    anchor_spl::{
        associated_token::AssociatedToken,
        metadata::{
            create_master_edition_v3, create_metadata_accounts_v3,
            mpl_token_metadata::types::DataV2, CreateMasterEditionV3, CreateMetadataAccountsV3,
            Metadata,
        },
        mint,
        token::{mint_to, Mint, MintTo, Token, TokenAccount},
    },
};

declare_id!("52quezNUzc1Ej6Jh6L4bvtxPW8j6TEFHuLVAWiFvdnsc");

#[program]
pub mod nft_minter {
    use super::*;

    pub fn mint_nft(
        context: &mut Context<MintNftAccountConstraints>,
        nft_name: String,
        nft_symbol: String,
        nft_uri: String,
    ) -> Result<()> {
        // `AccountView` is Copy, and a copy still points at the same
        // account — v2's typed handles make the aliasing a compile error.
        let mint_account_view = *context.accounts.mint_account.account();
        let payer_view = *context.accounts.payer.account();
        msg!("Minting Token");
        // Cross Program Invocation (CPI)
        // Invoking the mint_to instruction on the token program
        mint_to(
            CpiContext::new(
                context.accounts.token_program.address(),
                MintTo {
                    mint: context.accounts.mint_account.cpi_handle_mut(),
                    to: context.accounts.associated_token_account.cpi_handle_mut(),
                    authority: CpiHandle::readonly(&payer_view),
                },
            ),
            1,
        )?;

        msg!("Creating metadata account");
        // Cross Program Invocation (CPI)
        // Invoking the create_metadata_account_v3 instruction on the token metadata program
        create_metadata_accounts_v3(
            CpiContext::new(
                context.accounts.token_metadata_program.address(),
                CreateMetadataAccountsV3 {
                    metadata: context.accounts.metadata_account.cpi_handle_mut(),
                    mint: CpiHandle::readonly(&mint_account_view),
                    mint_authority: CpiHandle::readonly(&payer_view),
                    update_authority: CpiHandle::readonly(&payer_view),
                    payer: context.accounts.payer.cpi_handle_mut(),
                    system_program: context.accounts.system_program.cpi_handle(),
                    update_authority_is_signer: true,
                },
            ),
            DataV2 {
                name: nft_name,
                symbol: nft_symbol,
                uri: nft_uri,
                seller_fee_basis_points: 0,
                creators: None,
                collection: None,
                uses: None,
            },
            false, // Is mutable
            None,  // Collection details
        )?;

        msg!("Creating master edition account");
        // Cross Program Invocation (CPI)
        // Invoking the create_master_edition_v3 instruction on the token metadata program
        create_master_edition_v3(
            CpiContext::new(
                context.accounts.token_metadata_program.address(),
                CreateMasterEditionV3 {
                    edition: context.accounts.edition_account.cpi_handle_mut(),
                    mint: context.accounts.mint_account.cpi_handle_mut(),
                    update_authority: CpiHandle::readonly(&payer_view),
                    mint_authority: CpiHandle::readonly(&payer_view),
                    payer: context.accounts.payer.cpi_handle_mut(),
                    metadata: context.accounts.metadata_account.cpi_handle_mut(),
                    token_program: context.accounts.token_program.cpi_handle(),
                    system_program: context.accounts.system_program.cpi_handle(),
                    update_authority_is_signer: true,
                },
            ),
            None, // Max Supply
        )?;

        msg!("NFT minted successfully.");

        Ok(())
    }
}

#[derive(Accounts)]
pub struct MintNftAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    /// CHECK: Validate address by deriving pda
    #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.address().as_ref(), mint_account.address().as_ref()],
        bump,
        seeds::program = token_metadata_program.address(),
    )]
    pub metadata_account: UncheckedAccount,

    /// CHECK: Validate address by deriving pda
    #[account(
        mut,
        seeds = [b"metadata", token_metadata_program.address().as_ref(), mint_account.address().as_ref(), b"edition"],
        bump,
        seeds::program = token_metadata_program.address(),
    )]
    pub edition_account: UncheckedAccount,

    // Create new mint account, NFTs have 0 decimals
    #[account(
        init,
        payer = payer,
        mint::decimals = 0,
        mint::authority = payer,
        mint::freeze_authority = payer,
    )]
    pub mint_account: Account<Mint>,

    // Create associated token account, if needed
    // This is the account that will hold the NFT
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint_account,
        associated_token::authority = payer,
    )]
    pub associated_token_account: Account<TokenAccount>,

    pub token_program: Program<Token>,
    pub token_metadata_program: Program<Metadata>,
    pub associated_token_program: Program<AssociatedToken>,
    pub system_program: Program<System>,
    pub rent: Sysvar<Rent>,
}
