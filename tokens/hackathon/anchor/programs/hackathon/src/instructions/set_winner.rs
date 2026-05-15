use anchor_lang::prelude::*;

use crate::error::HackathonError;
use crate::state::{Hackathon, Prize};

#[derive(Accounts)]
#[instruction(prize_index: u8)]
pub struct SetWinner<'info> {
    // Hackathon admin. Must match `hackathon.authority`.
    pub authority: Signer<'info>,

    #[account(
        has_one = authority,
        seeds = [b"hackathon", authority.key().as_ref(), super::name_seed(&hackathon.name).as_ref()],
        bump = hackathon.bump,
    )]
    pub hackathon: Account<'info, Hackathon>,

    #[account(
        mut,
        seeds = [b"prize", hackathon.key().as_ref(), &[prize_index]],
        bump = prize.bump,
        constraint = prize.hackathon == hackathon.key(),
    )]
    pub prize: Account<'info, Prize>,
}

pub fn handle_set_winner(
    context: Context<SetWinner>,
    _prize_index: u8,
    winner: Pubkey,
) -> Result<()> {
    let prize = &mut context.accounts.prize;
    require!(!prize.paid, HackathonError::AlreadyPaid);
    require!(!prize.cancelled, HackathonError::Cancelled);
    prize.winner = Some(winner);
    Ok(())
}
