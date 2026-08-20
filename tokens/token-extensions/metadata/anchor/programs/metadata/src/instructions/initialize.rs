use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, transfer, CreateAccount, Transfer};
use anchor_spl::{
    token_2022::{
        initialize_mint2,
        spl_token_2022::{extension::ExtensionType, pod::PodMint},
        InitializeMint2,
    },
    token_2022_extensions::metadata_pointer::{
        metadata_pointer_initialize, MetadataPointerInitialize,
    },
    token_interface::{token_metadata_initialize, Token2022, TokenMetadataInitialize},
};
use spl_token_metadata_interface::state::TokenMetadata;
use spl_type_length_value::variable_len_pack::VariableLenPack;

#[derive(Accounts)]
pub struct InitializeAccountConstraints {
    #[account(mut)]
    pub payer: Signer,

    #[account(mut)]
    pub mint_account: Signer,
    pub token_program: Program<Token2022>,
    pub system_program: Program<System>,
}

pub fn process_initialize(
    context: &mut Context<InitializeAccountConstraints>,
    args: TokenMetadataArgs,
) -> Result<()> {
    let TokenMetadataArgs { name, symbol, uri } = args;

    // There is currently not an anchor constraint to automatically initialize
    // the MetadataPointer extension, so create and initialize the mint by hand:
    // allocate with room for the extension, initialize the extension, then
    // initialize the mint data.
    let mint_size =
        ExtensionType::try_calculate_account_len::<PodMint>(&[ExtensionType::MetadataPointer])?;
    let mint_lamports = Rent::get()?.try_minimum_balance(mint_size)?;
    let mint_address = *context.accounts.mint_account.address();

    create_account(
        CpiContext::new(
            context.accounts.system_program.address(),
            CreateAccount {
                from: context.accounts.payer.cpi_handle_mut(),
                to: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        mint_lamports,
        mint_size as u64,
        context.accounts.token_program.address(),
    )?;

    // The metadata lives in the mint account itself, so the pointer points at it.
    metadata_pointer_initialize(
        CpiContext::new(
            context.accounts.token_program.address(),
            MetadataPointerInitialize {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        Some(context.accounts.payer.address()),
        Some(&mint_address),
    )?;

    initialize_mint2(
        CpiContext::new(
            context.accounts.token_program.address(),
            InitializeMint2 {
                mint: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        2,
        context.accounts.payer.address(),
        None,
    )?;

    // Define token metadata
    let token_metadata = TokenMetadata {
        name: name.clone(),
        symbol: symbol.clone(),
        uri: uri.clone(),
        ..Default::default()
    };

    // Add 4 extra bytes for size of MetadataExtension (2 bytes for type, 2 bytes for length)
    let data_len = 4 + token_metadata.get_packed_len()?;

    // Calculate lamports required for the additional metadata
    let lamports = Rent::get()?.try_minimum_balance(data_len)?;

    // Transfer additional lamports to mint account
    transfer(
        CpiContext::new(
            context.accounts.system_program.address(),
            Transfer {
                from: context.accounts.payer.cpi_handle_mut(),
                to: context.accounts.mint_account.cpi_handle_mut(),
            },
        ),
        lamports,
    )?;

    // Initialize token metadata. `AccountView` is Copy and a copy still points
    // at the same account, so the read-only slots come from copies. v2's typed
    // handles enforce borrow exclusivity at compile time.
    let payer_view = *context.accounts.payer.account();
    let mint_view = *context.accounts.mint_account.account();

    token_metadata_initialize(
        CpiContext::new(
            context.accounts.token_program.address(),
            TokenMetadataInitialize {
                mint: CpiHandle::readonly(&mint_view),
                metadata: context.accounts.mint_account.cpi_handle_mut(),
                mint_authority: CpiHandle::readonly(&payer_view),
                update_authority: CpiHandle::readonly(&payer_view),
            },
        ),
        name,
        symbol,
        uri,
    )?;
    Ok(())
}

#[derive(IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub struct TokenMetadataArgs {
    pub name: String,
    pub symbol: String,
    pub uri: String,
}
