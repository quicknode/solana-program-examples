use {
    crate::{
        instructions::shared::{err, error},
        state::Pool,
        PoolAuthorityPda,
    },
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
    #[account(address = PoolAuthorityPda::seeds(pool.address()))]
    pub pool_authority: UncheckedAccount,
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

    let bump = [bumps.pool_authority];
    let seeds: &[Seed] = &[
        Seed::from(b"authority".as_ref()),
        Seed::from(accounts.pool.address().as_ref()),
        Seed::from(&bump as &[u8]),
    ];
    accounts
        .token_program
        .transfer(
            &accounts.custody_vault,
            &accounts.authority_collateral,
            &accounts.pool_authority,
            amount,
        )
        .invoke_signed(seeds)?;

    Ok(())
}
