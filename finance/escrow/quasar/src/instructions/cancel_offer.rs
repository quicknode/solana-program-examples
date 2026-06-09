use {
    crate::state::Offer,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CancelOffer {
    #[account(mut)]
    pub maker: Signer,
    #[account(
        mut,
        has_one(maker),
        close(dest = maker),
        address = Offer::seeds(maker.address())
    )]
    pub offer: Account<Offer>,
    pub token_mint_a: Account<Mint>,
    #[account(
        mut,
        init(idempotent),
        payer = maker,
        token(mint = token_mint_a, authority = maker, token_program = token_program),
    )]
    pub maker_token_account_a: Account<Token>,
    #[account(mut)]
    pub vault: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_withdraw_tokens_and_close_cancel_offer(accounts: &mut CancelOffer, bumps: &CancelOfferBumps) -> Result<(), ProgramError> {
    let bump = [bumps.offer];
    let seeds = [
        Seed::from(b"offer" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    accounts.token_program
        .transfer(
            &accounts.vault,
            &accounts.maker_token_account_a,
            &accounts.offer,
            accounts.vault.amount(),
        )
        .invoke_signed(&seeds)?;

    accounts.token_program
        .close_account(&accounts.vault, &accounts.maker, &accounts.offer)
        .invoke_signed(&seeds)?;
    Ok(())
}
