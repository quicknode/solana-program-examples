pub use crate::errors::GameErrorCode;
use crate::state::player_data::PlayerData;
use crate::{constants::MAX_ENERGY, GameData};
use anchor_lang::prelude::*;

pub fn handle_init_player(context: &mut Context<InitPlayerAccountConstraints>) -> Result<()> {
    context.accounts.player.energy = MAX_ENERGY;
    context.accounts.player.last_login = Clock::get()?.unix_timestamp;
    context.accounts.player.authority = *context.accounts.signer.address();
    context.accounts.player.bump = context.bumps.player;
    // init_if_needed - only save bump if this is the first init. Subsequent
    // calls reuse the existing account and must not overwrite the stored bump
    // (they'd be equal anyway because PDA derivation is deterministic, but
    // guarding keeps the intent crystal-clear).
    if context.accounts.game_data.bump == 0 {
        context.accounts.game_data.bump = context.bumps.game_data;
    }
    Ok(())
}

#[derive(Accounts)]
// The leading underscore is for rustc: `#[derive(Accounts)]` expands `_level_seed`
// into a path that never reads it, so the plain name warns as unused. The
// `seeds` expression below is the real use.
#[instruction(_level_seed: String)]
pub struct InitPlayerAccountConstraints {
    #[account(
        init,
        payer = signer,
        space = PlayerData::DISCRIMINATOR.len() + PlayerData::INIT_SPACE,
        seeds = [b"player".as_ref(), signer.address().as_ref()],
        bump,
    )]
    pub player: BorshAccount<PlayerData>,

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
}
