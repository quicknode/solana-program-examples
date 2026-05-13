use anchor_lang::prelude::*;

use crate::error::HackathonError;
use crate::state::{Hackathon, HACKATHON_NAME_MAX_LEN};

use super::name_seed;

#[derive(Accounts)]
#[instruction(name: String)]
pub struct CreateHackathon<'info> {
    // Pays rent for the Hackathon account. Separate from `authority` so a
    // Squads vault PDA (which cannot pay rent directly) can still be the
    // authority — a human keypair funds the create call.
    #[account(mut)]
    pub payer: Signer<'info>,

    // The eventual administrator of this hackathon. Stored on the account
    // verbatim. Does not need to sign `create_hackathon` (the payer signs
    // for rent), but every privileged handler thereafter requires this key
    // to sign.
    /// CHECK: stored verbatim as `hackathon.authority`; no on-chain reads.
    pub authority: UncheckedAccount<'info>,

    #[account(
        init,
        payer = payer,
        space = Hackathon::DISCRIMINATOR.len() + Hackathon::INIT_SPACE,
        seeds = [b"hackathon", authority.key().as_ref(), name_seed(&name).as_ref()],
        bump
    )]
    pub hackathon: Account<'info, Hackathon>,

    pub system_program: Program<'info, System>,
}

pub fn handle_create_hackathon(context: Context<CreateHackathon>, name: String) -> Result<()> {
    require!(!name.is_empty(), HackathonError::EmptyName);
    require!(
        name.len() <= HACKATHON_NAME_MAX_LEN,
        HackathonError::NameTooLong
    );

    context.accounts.hackathon.set_inner(Hackathon {
        authority: context.accounts.authority.key(),
        prize_count: 0,
        bump: context.bumps.hackathon,
        name,
    });
    Ok(())
}
