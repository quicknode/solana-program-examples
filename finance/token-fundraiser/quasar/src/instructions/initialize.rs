use {
    crate::state::{Fundraiser, FundraiserInner},
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Initialize {
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
pub fn handle_initialize(
    accounts: &mut Initialize,
    amount_to_raise: u64,
    duration: u16,
    bump: u8,
) -> Result<(), ProgramError> {
    // Validate minimum raise amount
    require!(amount_to_raise > 0, ProgramError::InvalidArgument);

    accounts.fundraiser.set_inner(FundraiserInner {
        maker: *accounts.maker.address(),
        mint_to_raise: *accounts.mint_to_raise.address(),
        amount_to_raise,
        current_amount: 0,
        time_started: 0,
        duration,
        bump,
    });
    Ok(())
}
