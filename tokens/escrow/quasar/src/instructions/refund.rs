use {
    crate::state::Escrow,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct Refund {
    #[account(mut)]
    pub maker: Signer,
    #[account(
        mut,
        has_one(maker),
        close(dest = maker),
        address = Escrow::seeds(maker.address())
    )]
    pub escrow: Account<Escrow>,
    pub mint_a: Account<Mint>,
    #[account(
        mut,
        init(idempotent),
        payer = maker,
        token(mint = mint_a, authority = maker, token_program = token_program),
    )]
    pub maker_ta_a: Account<Token>,
    #[account(mut)]
    pub vault_ta_a: Account<Token>,
    pub rent: Sysvar<Rent>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_withdraw_tokens_and_close_refund(accounts: &mut Refund, bumps: &RefundBumps) -> Result<(), ProgramError> {
    let bump = [bumps.escrow];
    let seeds = [
        Seed::from(b"escrow" as &[u8]),
        Seed::from(accounts.maker.address().as_ref()),
        Seed::from(bump.as_ref()),
    ];

    accounts.token_program
        .transfer(
            &accounts.vault_ta_a,
            &accounts.maker_ta_a,
            &accounts.escrow,
            accounts.vault_ta_a.amount(),
        )
        .invoke_signed(&seeds)?;

    accounts.token_program
        .close_account(&accounts.vault_ta_a, &accounts.maker, &accounts.escrow)
        .invoke_signed(&seeds)?;
    Ok(())
}
