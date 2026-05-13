use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::error::HackathonError;
use crate::state::{Hackathon, Prize};

#[derive(Accounts)]
pub struct AddPrize<'info> {
    // Rent payer. Separate from `authority` to allow a non-signing Squads
    // vault PDA to be the authority while a human keypair funds the call.
    #[account(mut)]
    pub payer: Signer<'info>,

    // Hackathon admin. Must match `hackathon.authority`.
    pub authority: Signer<'info>,

    #[account(
        mut,
        has_one = authority,
        seeds = [b"hackathon", authority.key().as_ref(), super::name_seed(&hackathon.name).as_ref()],
        bump = hackathon.bump,
    )]
    pub hackathon: Account<'info, Hackathon>,

    // Per-prize mint. Using the token interface so the same compiled program
    // works for classic SPL Token (e.g. USDC) and Token-2022 mints.
    #[account(mint::token_program = token_program)]
    pub mint: InterfaceAccount<'info, Mint>,

    #[account(
        init,
        payer = payer,
        space = Prize::DISCRIMINATOR.len() + Prize::INIT_SPACE,
        seeds = [b"prize", hackathon.key().as_ref(), &[hackathon.prize_count]],
        bump
    )]
    pub prize: Account<'info, Prize>,

    // Vault ATA for this prize. Owned by the Prize PDA so `pay_winner` can
    // sign the outgoing transfer with the prize's seeds.
    #[account(
        init,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = prize,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_add_prize(context: Context<AddPrize>, amount: u64) -> Result<()> {
    let hackathon = &mut context.accounts.hackathon;
    let index = hackathon.prize_count;

    context.accounts.prize.set_inner(Prize {
        hackathon: hackathon.key(),
        index,
        mint: context.accounts.mint.key(),
        amount,
        winner: None,
        paid: false,
        cancelled: false,
        bump: context.bumps.prize,
    });

    hackathon.prize_count = hackathon
        .prize_count
        .checked_add(1)
        .ok_or(HackathonError::PrizeCounterOverflow)?;

    Ok(())
}
