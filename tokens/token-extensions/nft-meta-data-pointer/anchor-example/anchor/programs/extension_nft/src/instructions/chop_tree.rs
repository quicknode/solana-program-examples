pub use crate::errors::GameErrorCode;
pub use crate::state::game_data::GameData;
use crate::{state::player_data::PlayerData, NftAuthority};
use anchor_lang::prelude::*;
use anchor_lang::solana_program::program::invoke_signed;
use anchor_spl::token_2022_extensions::spl_token_metadata_interface;
use anchor_spl::token_interface::{spl_token_2022, Token2022};
use session_keys::{Session, SessionToken};

pub fn chop_tree(context: &mut Context<ChopTreeAccountConstraints>, counter: u16, amount: u64) -> Result<()> {
    // Save game_data bump on first creation (init_if_needed). See init_player.rs
    // for the same pattern.
    let game_data_bump = context.bumps.game_data;
    let account: &mut ChopTreeAccountConstraints<'_> = context.accounts;
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
    invoke_signed(
        &spl_token_metadata_interface::instruction::update_field(
            &spl_token_2022::id(),
            context.accounts.mint.cpi_handle_mut().key,
            context.accounts.nft_authority.cpi_handle_mut().key,
            spl_token_metadata_interface::state::Field::Key("wood".to_string()),
            context.accounts.player.wood.to_string(),
        ),
        &[
            context.accounts.mint.cpi_handle().clone(),
            context.accounts.nft_authority.cpi_handle().clone(),
        ],
        signer,
    )?;

    Ok(())
}

#[derive(Accounts, Session)]
#[instruction(level_seed: String)]
pub struct ChopTreeAccountConstraints {
    #[session(
        // The ephemeral key pair signing the transaction
        signer = signer,
        // The authority of the user account which must have created the session
        authority = player.authority.address()
    )]
    // Session Tokens are passed as optional accounts
    pub session_token: Option<Account<SessionToken>>,

    // There is one PlayerData account
    #[account(
        mut,
        seeds = [b"player".as_ref(), player.authority.address().as_ref()],
        bump,
    )]
    pub player: BorshAccount<PlayerData>,

    // There can be multiple levels the seed for the level is passed in the instruction
    // First player starting a new level will pay for the account in the current setup
    #[account(
        init_if_needed,
        payer = signer,
        space = GameData::DISCRIMINATOR.len() + GameData::INIT_SPACE,
        seeds = [level_seed.as_ref()],
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
