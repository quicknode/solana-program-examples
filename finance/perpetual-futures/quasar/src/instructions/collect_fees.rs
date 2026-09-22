use {
    crate::{
        instructions::shared::{err, error},
        state::Pool,
    },
    quasar_lang::cpi::Seed,
    quasar_lang::prelude::*,
    quasar_spl::prelude::*,
};

#[derive(Accounts)]
pub struct CollectFees {
    #[account(mut)]
    pub authority: Signer,
    #[account(
        mut,
        has_one(authority),
        address = Pool::seeds(collateral_mint.address(), oracle_feed.address()),
        has_one(custody_vault),
    )]
    pub pool: Account<Pool>,
    /// CHECK: bound to the pool via its seeds.
    pub oracle_feed: UncheckedAccount,
    pub collateral_mint: Account<Mint>,
    #[account(mut)]
    pub custody_vault: Account<Token>,
    #[account(
        mut,
        init(idempotent),
        payer = authority,
        token(mint = collateral_mint, authority = authority, token_program = token_program),
    )]
    pub authority_collateral: Account<Token>,
    pub token_program: Program<TokenProgram>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_collect_fees(
    accounts: &mut CollectFees,
    bumps: &CollectFeesBumps,
) -> Result<(), ProgramError> {
    let amount = accounts.pool.protocol_fees.get();
    if amount == 0 {
        return Err(err(error::NOTHING_TO_CLAIM));
    }
    accounts.pool.protocol_fees.set(0);

    // The pool signs the CPI below with its own seeds.
    let bump = [bumps.pool];
    let seeds: &[Seed] = &[
        Seed::from(b"pool".as_ref()),
        Seed::from(accounts.collateral_mint.address().as_ref()),
        Seed::from(accounts.oracle_feed.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];
    accounts
        .token_program
        .transfer_checked(
            &accounts.custody_vault,
            &accounts.collateral_mint,
            &accounts.authority_collateral,
            &accounts.pool,
            amount,
            accounts.collateral_mint.decimals(),
        )
        .invoke_signed(seeds)?;

    Ok(())
}
