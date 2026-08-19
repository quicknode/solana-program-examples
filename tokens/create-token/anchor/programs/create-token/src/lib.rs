use {
    anchor_lang::prelude::*,
    anchor_spl::{
        metadata::{
            create_metadata_accounts_v3, mpl_token_metadata::types::DataV2,
            CreateMetadataAccountsV3, Metadata,
        },
        mint::{self, Mint},
        token::Token,
    },
};

declare_id!("GwvQ53QTu1xz3XXYfG5m5jEqwhMBvVBudPS8TUuFYnhT");

#[program]
pub mod create_token {
    use super::*;

    pub fn create_token_mint(
        context: &mut Context<CreateTokenMintAccountConstraints>,
        _token_decimals: u8,
        token_name: String,
        token_symbol: String,
        token_uri: String,
    ) -> Result<()> {
        msg!("Creating metadata account...");
        msg!(
            "Metadata account address: {}",
            &context.accounts.metadata_account.address()
        );

        // Cross Program Invocation (CPI)
        // Invoking the create_metadata_account_v3 instruction on the token metadata program
        // `payer` fills three CPI slots (payer, mint authority, update
        // authority). v2's typed handles enforce borrow exclusivity at compile
        // time, so the two read-only slots are built from a copy of the
        // `AccountView`, and it still points at the same underlying account.
        let payer_view = *context.accounts.payer.account();

        create_metadata_accounts_v3(
            CpiContext::new(
                context.accounts.token_metadata_program.address(),
                CreateMetadataAccountsV3 {
                    metadata: context.accounts.metadata_account.cpi_handle_mut(),
                    mint: context.accounts.mint_account.cpi_handle(),
                    mint_authority: CpiHandle::readonly(&payer_view),
                    update_authority: CpiHandle::readonly(&payer_view),
                    payer: context.accounts.payer.cpi_handle_mut(),
                    system_program: context.accounts.system_program.cpi_handle(),
                    update_authority_is_signer: true,
                },
            ),
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

        msg!("Token mint created successfully.");

        Ok(())
    }
}

#[derive(Accounts)]
#[instruction(_token_decimals: u8)]
pub struct CreateTokenMintAccountConstraints {
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
    // Create new mint account
    #[account(
        init,
        payer = payer,
        mint::decimals = _token_decimals,
        mint::authority = payer,
    )]
    pub mint_account: Account<Mint>,

    pub token_metadata_program: Program<Metadata>,
    pub token_program: Program<Token>,
    pub system_program: Program<System>,
    pub rent: Sysvar<Rent>,
}
