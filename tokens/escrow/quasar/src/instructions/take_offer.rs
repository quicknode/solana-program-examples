use {
    crate::state::Offer,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct TakeOffer {
    #[account(mut)]
    pub taker: Signer,
    #[account(
        mut,
        has_one(maker),
        has_one(maker_token_account_b),
        constraints(offer.receive > 0),
        close(dest = taker),
        address = Offer::seeds(maker.address())
    )]
    pub offer: Account<Offer>,
    #[account(mut)]
    pub maker: UncheckedAccount,
    pub token_mint_a: Account<Mint>,
    pub token_mint_b: Account<Mint>,
    #[account(
        mut,
        init(idempotent),
        payer = taker,
        token(mint = token_mint_a, authority = taker, token_program = token_program),
    )]
    pub taker_token_account_a: Account<Token>,
    #[account(mut)]
    pub taker_token_account_b: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = taker,
        token(mint = token_mint_b, authority = maker, token_program = token_program),
    )]
    pub maker_token_account_b: Account<Token>,
    #[account(mut)]
    pub vault: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_transfer_tokens(accounts: &mut TakeOffer) -> Result<(), ProgramError> {
    accounts.token_program
        .transfer(
            &accounts.taker_token_account_b,
            &accounts.maker_token_account_b,
            &accounts.taker,
            accounts.offer.receive,
        )
        .invoke()
}

#[inline(always)]
pub fn handle_withdraw_tokens_and_close_take(accounts: &mut TakeOffer, bumps: &TakeOfferBumps) -> Result<(), ProgramError> {
    let bump = [bumps.offer];
    let seeds = [
        Seed::from(b"offer" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    accounts.token_program
        .transfer(
            &accounts.vault,
            &accounts.taker_token_account_a,
            &accounts.offer,
            accounts.vault.amount(),
        )
        .invoke_signed(&seeds)?;

    accounts.token_program
        .close_account(&accounts.vault, &accounts.taker, &accounts.offer)
        .invoke_signed(&seeds)?;
    Ok(())
}
