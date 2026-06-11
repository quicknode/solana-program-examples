pub use crate::errors::GameErrorCode;
use crate::state::player_data::PlayerData;
use crate::{constants::MAX_ENERGY, GameData};
use anchor_lang::prelude::*;

pub fn handle_init_player(context: Context<InitPlayerAccountConstraints>) -> Result<()> {
    context.accounts.player.energy = MAX_ENERGY;
    context.accounts.player.last_login = Clock::get()?.unix_timestamp;
    context.accounts.player.authority = context.accounts.signer.key();
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
#[instruction(level_seed: String)]
pub struct InitPlayerAccountConstraints<'info> {
    #[account(
        init,
        payer = signer,
        space = PlayerData::DISCRIMINATOR.len() + PlayerData::INIT_SPACE,
        seeds = [b"player".as_ref(), signer.key().as_ref()],
        bump,
    )]
    pub player: Account<'info, PlayerData>,

    #[account(
        init_if_needed,
        payer = signer,
        space = GameData::DISCRIMINATOR.len() + GameData::INIT_SPACE,
        seeds = [level_seed.as_ref()],
        bump,
    )]
    pub game_data: Account<'info, GameData>,

    #[account(mut)]
    pub signer: Signer<'info>,
    pub system_program: Program<'info, System>,
}
