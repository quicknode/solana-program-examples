use anchor_lang::prelude::*;

use crate::error::HackathonError;
use crate::state::{Hackathon, HACKATHON_NAME_MAX_LEN};

use super::name_seed;

#[derive(Accounts)]
#[instruction(name: String, name_seed_arg: [u8; 32])]
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
    /// CHECK: stored verbatim as `hackathon.authority`; no onchain reads.
    pub authority: UncheckedAccount<'info>,

    // The PDA is seeded by `name_seed_arg` (an instruction argument) rather
    // than computing `sha256(name)` inside the seed expression — Anchor's
    // IDL builder does not support function calls in seed expressions. The
    // handler then verifies the binding (`name_seed_arg == sha256(name)`)
    // before storing both on the account.
    #[account(
        init,
        payer = payer,
        space = Hackathon::DISCRIMINATOR.len() + Hackathon::INIT_SPACE,
        seeds = [b"hackathon", authority.key().as_ref(), name_seed_arg.as_ref()],
        bump
    )]
    pub hackathon: Account<'info, Hackathon>,

    pub system_program: Program<'info, System>,
}

pub fn handle_create_hackathon(
    context: Context<CreateHackathon>,
    name: String,
    name_seed_arg: [u8; 32],
) -> Result<()> {
    require!(!name.is_empty(), HackathonError::EmptyName);
    require!(
        name.len() <= HACKATHON_NAME_MAX_LEN,
        HackathonError::NameTooLong
    );
    // Bind `name_seed_arg` to `name` so the seed cannot be chosen
    // independently of the stored name. Without this, a caller could create
    // a hackathon at any PDA address and claim any human-readable name.
    require!(
        name_seed_arg == name_seed(&name),
        HackathonError::NameSeedMismatch
    );

    context.accounts.hackathon.set_inner(Hackathon {
        authority: context.accounts.authority.key(),
        name_seed: name_seed_arg,
        prize_count: 0,
        bump: context.bumps.hackathon,
        name,
    });
    Ok(())
}
