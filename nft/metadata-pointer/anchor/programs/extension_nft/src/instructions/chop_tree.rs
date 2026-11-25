pub use crate::errors::GameErrorCode;
pub use crate::state::game_data::GameData;
use crate::{state::player_data::PlayerData, NftAuthority};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token_2022_extensions::spl_token_metadata_interface;
use anchor_spl::token_interface::{spl_token_2022, Token2022};

pub fn chop_tree(
    context: &mut Context<ChopTreeAccountConstraints>,
    counter: u16,
    amount: u64,
) -> Result<()> {
    let mint_address = *context.accounts.mint.address();
    let nft_authority_address = *context.accounts.nft_authority.address();
    // Save game_data bump on first creation (init_if_needed). See init_player.rs
    // for the same pattern.
    let game_data_bump = context.bumps.game_data;
    let account = &mut context.accounts;
    account.player.update_energy()?;
    account.player.print()?;

    if account.player.energy < amount {
        return err!(GameErrorCode::NotEnoughEnergy);
    }

    account.player.last_id = counter;
    account.player.chop_tree(amount)?;
    account.game_data.on_tree_chopped(amount)?;
    if account.game_data.bump == 0 {
        account.game_data.bump = game_data_bump;
    }

    msg!(
        "You chopped a tree and got 1 wood. You have {} wood and {} energy left.",
        context.accounts.player.wood,
        context.accounts.player.energy
    );

    // We use a PDA as a mint authority for the metadata account because we want to be able to update the NFT from
    // the program.
    let seeds = b"nft_authority";
    let bump = context.bumps.nft_authority;
    let signer: &[&[&[u8]]] = &[&[seeds, &[bump]]];

    // Update the metadata account with an additional metadata field in this case the player level
    // The handles have to line up positionally with the instruction's account
    // metas: `update_field` names the metadata account (the mint, writable) and
    // its update authority, in that order.
    let wood = context.accounts.player.wood.to_string();

    // `nft_authority` signs the CPI. It is a data account holding a live borrow
    // on its buffer, which the runtime would reject when the CPI borrows the
    // same account, so hand the borrow back across the call.
    context.accounts.nft_authority.release_borrow()?;
    invoke_signed(
        &spl_token_metadata_interface::instruction::update_field(
            &spl_token_2022::id(),
            &mint_address,
            &nft_authority_address,
            spl_token_metadata_interface::state::Field::Key("wood".to_string()),
            wood,
        ),
        &[
            context.accounts.mint.cpi_handle_mut().into(),
            context.accounts.nft_authority.cpi_handle(),
        ],
        signer,
    )?;
    context.accounts.nft_authority.reacquire_borrow_mut()?;

    Ok(())
}

#[derive(Accounts)]
// The leading underscore is for rustc: `#[derive(Accounts)]` expands `_level_seed`
// into a path that never reads it, so the plain name warns as unused. The
// `seeds` expression below is the real use.
#[instruction(_level_seed: String)]
pub struct ChopTreeAccountConstraints {
    // Session tokens are passed as optional accounts. The token is validated in
    // `chop_tree` (see `session::is_valid_session`) rather than by a derive,
    // since the session-keys `Session` derive is Anchor v1 only.
    /// CHECK: read as gpl-session's SessionToken; validated by seeds, owner and
    /// discriminator before it is trusted.
    pub session_token: Option<UncheckedAccount>,

    // There is one PlayerData account
    #[account(
        mut,
        seeds = [b"player".as_ref(), player.authority.as_ref()],
        bump = player.bump,
    )]
    pub player: BorshAccount<PlayerData>,

    // There can be multiple levels the seed for the level is passed in the instruction
    // First player starting a new level will pay for the account in the current setup
    #[account(
        init_if_needed,
        payer = signer,
        space = GameData::DISCRIMINATOR.len() + GameData::INIT_SPACE,
        seeds = [_level_seed.as_bytes()],
        bump,
    )]
    pub game_data: BorshAccount<GameData>,

    #[account(mut)]
    pub signer: Signer,
    pub system_program: Program<System>,
    /// CHECK: Make sure the ata to the mint is actually owned by the signer
    #[account(mut)]
    pub mint: UncheckedAccount,
    #[account(
        init_if_needed,
        seeds = [b"nft_authority".as_ref()],
        bump,
        space = NftAuthority::DISCRIMINATOR.len() + NftAuthority::INIT_SPACE,
        payer = signer,
    )]
    pub nft_authority: BorshAccount<NftAuthority>,
    pub token_program: Program<Token2022>,
}
