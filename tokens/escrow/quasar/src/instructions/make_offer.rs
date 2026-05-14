use {
    crate::state::{Offer, OfferInner},
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct MakeOffer {
    #[account(mut)]
    pub maker: Signer,
    #[account(mut, init, payer = maker, address = Offer::seeds(maker.address()))]
    pub offer: Account<Offer>,
    pub token_mint_a: Account<Mint>,
    pub token_mint_b: Account<Mint>,
    #[account(mut)]
    pub maker_token_account_a: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = maker,
        token(mint = token_mint_b, authority = maker, token_program = token_program),
    )]
    pub maker_token_account_b: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = maker,
        token(mint = token_mint_a, authority = offer, token_program = token_program),
    )]
    pub vault: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_make_offer(accounts: &mut MakeOffer, receive: u64, bumps: &MakeOfferBumps) -> Result<(), ProgramError> {
    accounts.offer.set_inner(OfferInner {
        maker: *accounts.maker.address(),
        token_mint_a: *accounts.token_mint_a.address(),
        token_mint_b: *accounts.token_mint_b.address(),
        maker_token_account_b: *accounts.maker_token_account_b.address(),
        receive,
        bump: bumps.offer,
    });
    Ok(())
}

#[inline(always)]
pub fn handle_deposit_tokens(accounts: &mut MakeOffer, amount: u64) -> Result<(), ProgramError> {
    accounts.token_program
        .transfer(&accounts.maker_token_account_a, &accounts.vault, &accounts.maker, amount)
        .invoke()
}
