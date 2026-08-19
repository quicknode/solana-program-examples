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
    // `AccountView` is Copy, and a copy still points at the same account. v2's
    // typed handles make the aliasing a compile error. `mint` and `signer` are
    // Signers and `nft_authority` releases its data borrow below, so none of
    // these copies aliases a live borrow.
    let mut mint_view = *context.accounts.mint.account();
    let mint_view_readonly = *context.accounts.mint.account();
    let nft_authority_view = *context.accounts.nft_authority.account();
    let signer_view = *context.accounts.signer.account();
    let mint_address = *context.accounts.mint.address();
    let nft_authority_address = *context.accounts.nft_authority.address();

    // `nft_authority` signs every CPI below. It is a data account holding a
    // live borrow on its buffer, which the runtime would reject when the CPI
    // borrows the same account, so hand the borrow back for the duration.
    context.accounts.nft_authority.release_borrow()?;

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
            context.accounts.system_program.address(),
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
            context.accounts.system_program.address(),
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

    // Handles line up positionally with the instruction's metas, and a writable
    // meta needs a writable handle: `initialize` names only the mint, writable.
    invoke(
        &init_meta_data_pointer_ix,
        &[CpiHandleMut::writable(&mut mint_view).into()],
    )?;

    // Initialize the mint cpi
    let mint_cpi_ix = CpiContext::new(
        context.accounts.token_program.address(),
        token_2022::InitializeMint2 {
            mint: context.accounts.mint.cpi_handle_mut(),
        },
    );

    token_2022::initialize_mint2(mint_cpi_ix, 0, &nft_authority_address, None)
    .unwrap();

    // We use a PDA as a mint authority for the metadata account because
    // we want to be able to update the NFT from the program.
    let seeds = b"nft_authority";
    let bump = context.bumps.nft_authority;
    let signer: &[&[&[u8]]] = &[&[seeds, &[bump]]];

    msg!("Init metadata {0}", nft_authority_address);

    // Init the metadata account
    let init_token_meta_data_ix = &spl_token_metadata_interface::instruction::initialize(
        &spl_token_2022::id(),
        &mint_address,
        &nft_authority_address,
        &mint_address,
        &nft_authority_address,
        "Beaver".to_string(),
        "BVA".to_string(),
        "https://arweave.net/MHK3Iopy0GgvDoM7LkkiAdg7pQqExuuWvedApCnzfj0".to_string(),
    );

    // `initialize` names metadata (the mint, writable), update_authority, mint
    // and mint_authority, so the mint and the authority each fill two slots.
    invoke_signed(
        init_token_meta_data_ix,
        &[
            CpiHandleMut::writable(&mut mint_view).into(),
            CpiHandle::readonly(&nft_authority_view),
            CpiHandle::readonly(&mint_view_readonly),
            CpiHandle::readonly(&nft_authority_view),
        ],
        signer,
    )?;

    // Update the metadata account with an additional metadata field in this case the player level
    invoke_signed(
        &spl_token_metadata_interface::instruction::update_field(
            &spl_token_2022::id(),
            &mint_address,
            &nft_authority_address,
            spl_token_metadata_interface::state::Field::Key("level".to_string()),
            "1".to_string(),
        ),
        &[
            CpiHandleMut::writable(&mut mint_view).into(),
            CpiHandle::readonly(&nft_authority_view),
        ],
        signer,
    )?;

    // Create the associated token account
    associated_token::create(CpiContext::new(
        context.accounts.associated_token_program.address(),
        associated_token::Create {
            payer: context.accounts.signer.cpi_handle_mut(),
            associated_token: context.accounts.token_account.cpi_handle_mut(),
            authority: CpiHandle::readonly(&signer_view),
            mint: CpiHandle::readonly(&mint_view),
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
                authority: CpiHandle::readonly(&nft_authority_view),
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
                current_authority: CpiHandle::readonly(&nft_authority_view),
                account_or_mint: context.accounts.mint.cpi_handle_mut(),
            },
            signer,
        ),
        AuthorityType::MintTokens,
        None,
    )?;

    context.accounts.nft_authority.reacquire_borrow_mut()?;

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
