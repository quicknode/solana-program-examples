use anchor_lang::prelude::*;

use crate::state::{Market, MarketUser, MARKET_USER_SEED};

pub fn handle_initialize_market_user(
    context: &mut Context<InitializeMarketUserAccountConstraints>,
) -> Result<()> {
    let market_user = &mut context.accounts.market_user;
    market_user.market = *context.accounts.market.address();
    market_user.owner = *context.accounts.owner.address();
    market_user.unsettled_base = 0;
    market_user.unsettled_quote = 0;
    market_user.open_orders = Vec::new();
    market_user.bump = context.bumps.market_user;

    Ok(())
}

#[derive(Accounts)]
pub struct InitializeMarketUserAccountConstraints {
    #[account(
        init,
        payer = owner,
        space = MarketUser::DISCRIMINATOR.len() + MarketUser::INIT_SPACE,
        seeds = [MARKET_USER_SEED, market.address().as_ref(), owner.address().as_ref()],
        bump
    )]
    pub market_user: BorshAccount<MarketUser>,

    pub market: BorshAccount<Market>,

    #[account(mut)]
    pub owner: Signer,

    pub system_program: Program<System>,
}
