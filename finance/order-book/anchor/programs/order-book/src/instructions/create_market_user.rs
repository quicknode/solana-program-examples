use anchor_lang::prelude::*;

use crate::state::{Market, MarketUser, MARKET_USER_SEED};

pub fn handle_create_market_user(context: Context<CreateMarketUserAccountConstraints>) -> Result<()> {
    let market_user = &mut context.accounts.market_user;
    market_user.market = context.accounts.market.key();
    market_user.owner = context.accounts.owner.key();
    market_user.unsettled_base = 0;
    market_user.unsettled_quote = 0;
    market_user.open_orders = Vec::new();
    market_user.bump = context.bumps.market_user;

    Ok(())
}

#[derive(Accounts)]
pub struct CreateMarketUserAccountConstraints<'info> {
    #[account(
        init,
        payer = owner,
        space = MarketUser::DISCRIMINATOR.len() + MarketUser::INIT_SPACE,
        seeds = [MARKET_USER_SEED, market.key().as_ref(), owner.key().as_ref()],
        bump
    )]
    pub market_user: Account<'info, MarketUser>,

    pub market: Account<'info, Market>,

    #[account(mut)]
    pub owner: Signer<'info>,

    pub system_program: Program<'info, System>,
}
