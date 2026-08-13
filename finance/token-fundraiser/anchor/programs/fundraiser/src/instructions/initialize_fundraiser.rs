use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};

use crate::{state::Fundraiser, FundraiserError, MIN_AMOUNT_TO_RAISE};

#[derive(Accounts)]
pub struct InitializeFundraiserAccountConstraints {
    #[account(mut)]
    pub maker: Signer,

    pub mint_to_raise: InterfaceAccount<Mint>,

    #[account(
        init,
        payer = maker,
        seeds = [b"fundraiser", maker.address().as_ref()],
        bump,
        space = Fundraiser::DISCRIMINATOR.len() + Fundraiser::INIT_SPACE,
    )]
    pub fundraiser: BorshAccount<Fundraiser>,

    #[account(
        init,
        payer = maker,
        associated_token::mint = mint_to_raise,
        associated_token::authority = fundraiser,
        associated_token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<TokenAccount>,

    pub system_program: Program<System>,

    pub token_program: Interface<'static, TokenInterface>,

    pub associated_token_program: Program<AssociatedToken>,
}

pub fn handle_initialize_fundraiser(
    accounts: &mut InitializeFundraiserAccountConstraints,
    amount: u64,
    duration: u16,
    bumps: &InitializeFundraiserAccountConstraintsBumps,
) -> Result<()> {
    // The target must be at least MIN_AMOUNT_TO_RAISE major units, expressed
    // in minor units: MIN_AMOUNT_TO_RAISE * 10^decimals.
    let one_major_unit = 10_u64
        .checked_pow(accounts.mint_to_raise.decimals() as u32)
        .ok_or(FundraiserError::MathOverflow)?;
    let minimum_amount_to_raise = MIN_AMOUNT_TO_RAISE
        .checked_mul(one_major_unit)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        amount >= minimum_amount_to_raise,
        FundraiserError::InvalidAmount
    );

    *accounts.fundraiser = (Fundraiser {
        maker: *accounts.maker.address(),
        mint_to_raise: *accounts.mint_to_raise.address(),
        amount_to_raise: amount,
        current_amount: 0,
        time_started: Clock::get()?.unix_timestamp,
        duration,
        bump: bumps.fundraiser,
    });

    Ok(())
}
