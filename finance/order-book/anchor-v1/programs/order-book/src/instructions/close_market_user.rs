use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::{Market, MarketUser, MARKET_USER_SEED};

/// Close a user's `MarketUser` account for a market and return its rent to
/// the owner who paid it. The account may close only when it holds nothing
/// the program would still need to find: `open_orders` must be empty (every
/// resting order cancelled or filled, so no fill or cancel will try to
/// credit it) and both unsettled balances must be zero (everything owed
/// has been paid out by `settle_funds`, so no credit is lost with the
/// account). Anything else is refused with `MarketUserNotClosable`. The
/// owner can call `initialize_market_user` again later to trade on the
/// market afresh.
pub fn handle_close_market_user(context: Context<CloseMarketUserAccountConstraints>) -> Result<()> {
    let market_user = &context.accounts.market_user;

    require!(
        market_user.owner == context.accounts.owner.key(),
        ErrorCode::Unauthorized
    );

    require!(
        market_user.open_orders.is_empty()
            && market_user.unsettled_base == 0
            && market_user.unsettled_quote == 0,
        ErrorCode::MarketUserNotClosable
    );

    Ok(())
}

#[derive(Accounts)]
pub struct CloseMarketUserAccountConstraints<'info> {
    pub market: Account<'info, Market>,

    // Derived from the owner the account itself records, so that a signer
    // who is not that owner reaches the handler's `Unauthorized` check
    // instead of failing the seeds constraint. Closed by Anchor after the
    // handler returns, rent to `owner`.
    #[account(
        mut,
        close = owner,
        seeds = [MARKET_USER_SEED, market.key().as_ref(), market_user.owner.as_ref()],
        bump = market_user.bump
    )]
    pub market_user: Account<'info, MarketUser>,

    #[account(mut)]
    pub owner: Signer<'info>,
}
