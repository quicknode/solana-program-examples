use quasar_lang::prelude::*;

use crate::state::{Market, MarketUser, MarketUserInner, OPEN_ORDERS_BYTES};

#[derive(Accounts)]
pub struct InitializeMarketUserAccountConstraints {
    #[account(mut)]
    pub owner: Signer,

    pub market: Account<Market>,

    #[account(
        init,
        payer = owner,
        address = MarketUser::seeds(market.address(), owner.address())
    )]
    pub market_user: Account<MarketUser>,

    pub rent: Sysvar<Rent>,
    pub system_program: Program<SystemProgram>,
}

#[inline(always)]
pub fn handle_initialize_market_user(
    accounts: &mut InitializeMarketUserAccountConstraints,
    bumps: &InitializeMarketUserAccountConstraintsBumps,
) -> Result<(), ProgramError> {
    accounts.market_user.set_inner(MarketUserInner {
        market: *accounts.market.address(),
        owner: *accounts.owner.address(),
        unsettled_base: 0,
        unsettled_quote: 0,
        open_orders_len: 0,
        bump: bumps.market_user,
        open_orders: [0u8; OPEN_ORDERS_BYTES],
    });
    Ok(())
}
