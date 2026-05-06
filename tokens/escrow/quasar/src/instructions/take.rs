use {
    crate::state::Escrow,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Take {
    #[account(mut)]
    pub taker: Signer,
    #[account(
        mut,
        has_one(maker),
        has_one(maker_ta_b),
        constraints(escrow.receive > 0),
        close(dest = taker),
        address = Escrow::seeds(maker.address())
    )]
    pub escrow: Account<Escrow>,
    #[account(mut)]
    pub maker: UncheckedAccount,
    pub mint_a: Account<Mint>,
    pub mint_b: Account<Mint>,
    #[account(
        mut,
        init(idempotent),
        payer = taker,
        token(mint = mint_a, authority = taker, token_program = token_program),
    )]
    pub taker_ta_a: Account<Token>,
    #[account(mut)]
    pub taker_ta_b: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = taker,
        token(mint = mint_b, authority = maker, token_program = token_program),
    )]
    pub maker_ta_b: Account<Token>,
    #[account(mut)]
    pub vault_ta_a: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_transfer_tokens(accounts: &mut Take) -> Result<(), ProgramError> {
    accounts.token_program
        .transfer(
            &accounts.taker_ta_b,
            &accounts.maker_ta_b,
            &accounts.taker,
            accounts.escrow.receive,
        )
        .invoke()
}

#[inline(always)]
pub fn handle_withdraw_tokens_and_close_take(accounts: &mut Take, bumps: &TakeBumps) -> Result<(), ProgramError> {
    let bump = [bumps.escrow];
    let seeds = [
        Seed::from(b"escrow" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    accounts.token_program
        .transfer(
            &accounts.vault_ta_a,
            &accounts.taker_ta_a,
            &accounts.escrow,
            accounts.vault_ta_a.amount(),
        )
        .invoke_signed(&seeds)?;

    accounts.token_program
        .close_account(&accounts.vault_ta_a, &accounts.taker, &accounts.escrow)
        .invoke_signed(&seeds)?;
    Ok(())
}
