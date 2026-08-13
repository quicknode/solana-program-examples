pub use crate::constants::TOKEN_METADATA_EXTENSION_SPACE;
pub use crate::errors::GameErrorCode;
pub use crate::state::game_data::GameData;
use anchor_lang::solana_program::program::{invoke, invoke_signed};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::{self, AssociatedToken},
    token_2022,
    token_2022_extensions::spl_token_metadata_interface,
    token_interface::{
        spl_token_2022::{self, extension::ExtensionType, instruction::AuthorityType, state::Mint},
        Token2022,
    },
};

pub fn handle_mint_nft(context: &mut Context<MintNftAccountConstraints>) -> Result<()> {
    msg!("Mint nft with meta data extension and additional meta data");

    let space =
        match ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::MetadataPointer]) {
            Ok(space) => space,
            Err(_) => {
                return err!(GameErrorCode::InvalidMintAccountSpace);
            }
        };

    // Space required for the inline SPL Token Metadata extension TLV. The
    // metadata lives on the mint account itself (not a separate account)
    // so we just over-allocate enough room at creation time.
    let meta_data_space = TOKEN_METADATA_EXTENSION_SPACE;

    let lamports_required = Rent::get()?.try_minimum_balance(space + meta_data_space)?;

    msg!(
        "Create Mint and metadata account size and cost: {} lamports: {}",
        space as u64,
        lamports_required
    );

    system_program::create_account(
        CpiContext::new(
            context.accounts.token_program.address(),
            system_program::CreateAccount {
                from: context.accounts.signer.cpi_handle_mut(),
                to: context.accounts.mint.cpi_handle_mut(),
            },
        ),
        lamports_required,
        space as u64,
        &context.accounts.token_program.address(),
    )?;

    // Assign the mint to the token program
    system_program::assign(
        CpiContext::new(
            context.accounts.token_program.address(),
            system_program::Assign {
                account_to_assign: context.accounts.mint.cpi_handle_mut(),
            },
        ),
        &token_2022::ID,
    )?;

    // Initialize the metadata pointer (Need to do this before initializing the mint)
    let init_meta_data_pointer_ix =
        match spl_token_2022::extension::metadata_pointer::instruction::initialize(
            &Token2022::id(),
            &context.accounts.mint.address(),
            Some(*context.accounts.nft_authority.address()),
            Some(*context.accounts.mint.address()),
        ) {
            Ok(ix) => ix,
            Err(_) => {
                return err!(GameErrorCode::CantInitializeMetadataPointer);
            }
        };

    invoke(
        &init_meta_data_pointer_ix,
        &[
            context.accounts.mint.cpi_handle(),
            context.accounts.nft_authority.cpi_handle(),
        ],
    )?;

    // Initialize the mint cpi
    let mint_cpi_ix = CpiContext::new(
        context.accounts.token_program.address(),
        token_2022::InitializeMint2 {
            mint: context.accounts.mint.cpi_handle_mut(),
        },
    );

    token_2022::initialize_mint2(mint_cpi_ix, 0, &context.accounts.nft_authority.address(), None)
        .unwrap();

    // We use a PDA as a mint authority for the metadata account because
    // we want to be able to update the NFT from the program.
    let seeds = b"nft_authority";
    let bump = context.bumps.nft_authority;
    let signer: &[&[&[u8]]] = &[&[seeds, &[bump]]];

    msg!(
        "Init metadata {0}",
        context.accounts.nft_authority.cpi_handle_mut().key
    );

    // Init the metadata account
    let init_token_meta_data_ix = &spl_token_metadata_interface::instruction::initialize(
        &spl_token_2022::id(),
        context.accounts.mint.key,
        context.accounts.nft_authority.cpi_handle_mut().key,
        context.accounts.mint.key,
        context.accounts.nft_authority.cpi_handle_mut().key,
        "Beaver".to_string(),
        "BVA".to_string(),
        "https://arweave.net/MHK3Iopy0GgvDoM7LkkiAdg7pQqExuuWvedApCnzfj0".to_string(),
    );

    invoke_signed(
        init_token_meta_data_ix,
        &[
            context.accounts.mint.cpi_handle().clone(),
            context.accounts.nft_authority.cpi_handle().clone(),
        ],
        signer,
    )?;

    // Update the metadata account with an additional metadata field in this case the player level
    invoke_signed(
        &spl_token_metadata_interface::instruction::update_field(
            &spl_token_2022::id(),
            context.accounts.mint.key,
            context.accounts.nft_authority.cpi_handle_mut().key,
            spl_token_metadata_interface::state::Field::Key("level".to_string()),
            "1".to_string(),
        ),
        &[
            context.accounts.mint.cpi_handle().clone(),
            context.accounts.nft_authority.cpi_handle().clone(),
        ],
        signer,
    )?;

    // Create the associated token account
    associated_token::create(CpiContext::new(
        context.accounts.associated_token_program.address(),
        associated_token::Create {
            payer: context.accounts.signer.cpi_handle_mut(),
            associated_token: context.accounts.token_account.cpi_handle_mut(),
            authority: context.accounts.signer.cpi_handle(),
            mint: context.accounts.mint.cpi_handle(),
            system_program: context.accounts.system_program.cpi_handle(),
            token_program: context.accounts.token_program.cpi_handle(),
        },
    ))?;

    // Mint one token to the associated token account of the player
    token_2022::mint_to(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            token_2022::MintTo {
                mint: context.accounts.mint.cpi_handle_mut(),
                to: context.accounts.token_account.cpi_handle_mut(),
                authority: context.accounts.nft_authority.cpi_handle(),
            },
            signer,
        ),
        1,
    )?;

    // Freeze the mint authority so no more tokens can be minted to make it an NFT
    token_2022::set_authority(
        CpiContext::new_with_signer(
            context.accounts.token_program.address(),
            token_2022::SetAuthority {
                current_authority: context.accounts.nft_authority.cpi_handle(),
                account_or_mint: context.accounts.mint.cpi_handle_mut(),
            },
            signer,
        ),
        AuthorityType::MintTokens,
        None,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct MintNftAccountConstraints {
    #[account(mut)]
    pub signer: Signer,
    pub system_program: Program<System>,
    pub token_program: Program<Token2022>,
    /// CHECK: We will create this one for the user
    #[account(mut)]
    pub token_account: UncheckedAccount,
    #[account(mut)]
    pub mint: Signer,
    pub rent: Sysvar<Rent>,
    pub associated_token_program: Program<AssociatedToken>,
    #[account(
        init_if_needed,
        seeds = [b"nft_authority".as_ref()],
        bump,
        space = NftAuthority::DISCRIMINATOR.len() + NftAuthority::INIT_SPACE,
        payer = signer
    )]
    pub nft_authority: BorshAccount<NftAuthority>,
}

#[account(borsh)]
#[derive(InitSpace)]
pub struct NftAuthority {}
