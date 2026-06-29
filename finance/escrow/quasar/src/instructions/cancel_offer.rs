use {
    crate::state::Offer,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CancelOfferAccountConstraints {
    #[account(mut)]
    pub maker: Signer,
    // Only the maker can cancel. The mint and vault are bound to the stored
    // offer state, and the offer closes back to the maker, who paid its rent
    // in make_offer.
    #[account(
        mut,
        has_one(maker),
        has_one(token_mint_a),
        has_one(vault),
        close(dest = maker),
        address = Offer::seeds(maker.address(), offer.id.into())
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
pub fn handle_withdraw_tokens_and_close_cancel_offer(
    accounts: &mut CancelOfferAccountConstraints,
    bumps: &CancelOfferAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    let id_bytes = u64::from(accounts.offer.id).to_le_bytes();
    let bump = [bumps.offer];
    let seeds = [
        Seed::from(b"offer" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(id_bytes.as_ref()),
        Seed::from(bump.as_ref()),
    ];

    accounts
        .token_program
        .transfer(
            &accounts.vault,
            &accounts.maker_token_account_a,
            &accounts.offer,
            accounts.vault.amount(),
        )
        .invoke_signed(&seeds)?;

    accounts
        .token_program
        .close_account(&accounts.vault, &accounts.maker, &accounts.offer)
        .invoke_signed(&seeds)?;
    Ok(())
}
