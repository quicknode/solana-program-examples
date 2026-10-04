use {
    crate::{
        error::FundraiserError,
        state::{one_major_unit, Fundraiser, FundraiserInner, MIN_AMOUNT_TO_RAISE},
    },
    quasar_lang::{prelude::*, sysvars::Sysvar as _},
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct InitializeFundraiserAccountConstraints {
    #[account(mut)]
    pub maker: Signer,

    pub mint_to_raise: Account<Mint>,

    #[account(mut, init, payer = maker, address = Fundraiser::seeds(maker.address()))]
    pub fundraiser: Account<Fundraiser>,

    #[account(
        mut,
        init(idempotent),
        payer = maker,
        token(mint = mint_to_raise, authority = fundraiser, token_program = token_program),
    )]
    pub vault: Account<Token>,

    pub rent: Sysvar<Rent>,

    pub token_program: Program<TokenProgram>,

    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_fundraiser(
    accounts: &mut InitializeFundraiserAccountConstraints,
    amount_to_raise: u64,
    duration: u16,
    bump: u8,
) -> Result<(), ProgramError> {
    // The target must be at least MIN_AMOUNT_TO_RAISE major units, expressed
    // in minor units: MIN_AMOUNT_TO_RAISE * 10^decimals.
    let minimum_amount_to_raise = MIN_AMOUNT_TO_RAISE
        .checked_mul(one_major_unit(accounts.mint_to_raise.decimals())?)
        .ok_or(FundraiserError::MathOverflow)?;
    require!(
        amount_to_raise >= minimum_amount_to_raise,
        FundraiserError::InvalidAmount
    );
    // A zero-day window would close before any contribution could land.
    require!(duration > 0, FundraiserError::InvalidDuration);

    let time_started: i64 = Clock::get()?.unix_timestamp.into();

    accounts.fundraiser.set_inner(FundraiserInner {
        maker: *accounts.maker.address(),
        mint_to_raise: *accounts.mint_to_raise.address(),
        vault: *accounts.vault.address(),
        amount_to_raise,
        current_amount: 0,
        time_started,
        duration,
        claimed: PodBool::from(false),
        open_contributions: 0,
        bump,
    });
    Ok(())
}
