use {
    crate::state::Offer,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct TakeOfferAccountConstraints {
    #[account(mut)]
    pub taker: Signer,
    // Every account the offer recorded at make time is bound to the stored
    // state: the maker, both mints, the maker's token B account, and the
    // vault. The offer closes back to the maker, who paid its rent in
    // make_offer.
    #[account(
        mut,
        has_one(maker),
        has_one(token_mint_a),
        has_one(token_mint_b),
        has_one(maker_token_account_b),
        has_one(vault),
        constraints(offer.receive > 0),
        close(dest = maker),
        address = Offer::seeds(maker.address(), offer.id.into())
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
    #[account(mut)]
    pub maker_token_account_b: Account<Token>,
    #[account(mut)]
    pub vault: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_transfer_tokens(
    accounts: &mut TakeOfferAccountConstraints,
) -> Result<(), ProgramError> {
    accounts
        .token_program
        .transfer_checked(
            &accounts.taker_token_account_b,
            &accounts.token_mint_b,
            &accounts.maker_token_account_b,
            &accounts.taker,
            accounts.offer.receive,
            accounts.token_mint_b.decimals(),
        )
        .invoke()
}

#[inline(always)]
pub fn handle_withdraw_tokens_and_close_take(
    accounts: &mut TakeOfferAccountConstraints,
    bumps: &TakeOfferAccountConstraintsBumps,
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
        .transfer_checked(
            &accounts.vault,
            &accounts.token_mint_a,
            &accounts.taker_token_account_a,
            &accounts.offer,
            accounts.vault.amount(),
            accounts.token_mint_a.decimals(),
        )
        .invoke_signed(&seeds)?;

    // The maker paid the vault's rent in make_offer, so the vault closes
    // back to the maker.
    accounts
        .token_program
        .close_account(&accounts.vault, &accounts.maker, &accounts.offer)
        .invoke_signed(&seeds)?;
    Ok(())
}
