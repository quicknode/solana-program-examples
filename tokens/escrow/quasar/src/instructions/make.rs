use {
    crate::state::{Escrow, EscrowInner},
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Make {
    #[account(mut)]
    pub maker: Signer,
    #[account(mut, init, payer = maker, address = Escrow::seeds(maker.address()))]
    pub escrow: Account<Escrow>,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    #[account(mut)]
    pub maker_ta_a: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = maker,
        token(mint = mint_b, authority = maker, token_program = token_program),
    )]
    pub maker_ta_b: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = maker,
        token(mint = mint_a, authority = escrow, token_program = token_program),
    )]
    pub vault_ta_a: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_make_escrow(accounts: &mut Make, receive: u64, bumps: &MakeBumps) -> Result<(), ProgramError> {
    accounts.escrow.set_inner(EscrowInner {
        maker: *accounts.maker.address(),
        mint_a: *accounts.mint_a.address(),
        mint_b: *accounts.mint_b.address(),
        maker_ta_b: *accounts.maker_ta_b.address(),
        receive,
        bump: bumps.escrow,
    });
    Ok(())
}

#[inline(always)]
pub fn handle_deposit_tokens(accounts: &mut Make, amount: u64) -> Result<(), ProgramError> {
    accounts.token_program
        .transfer(&accounts.maker_ta_a, &accounts.vault_ta_a, &accounts.maker, amount)
        .invoke()
}
