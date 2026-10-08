use quasar_lang::prelude::*;

use crate::errors::OrderBookError;
use crate::state::{Market, MarketUser};

/// Close a user's `MarketUser` account for a market and return its rent to
/// the owner who paid it. The account may close only when it holds nothing
/// the program would still need to find: `open_orders_len` must be zero
/// (every resting order cancelled or filled, so no fill or cancel will try
/// to credit it) and both unsettled balances must be zero (everything owed
/// has been paid out by `settle_funds`, so no credit is lost with the
/// account). Anything else is refused with `MarketUserNotClosable`. The
/// owner can call `initialize_market_user` again later to trade on the
/// market afresh.
#[derive(Accounts)]
pub struct CloseMarketUserAccountConstraints {
    pub market: Account<Market>,

    // Checked in the handler as the canonical PDA of `market` and the owner
    // the account records, so that a signer who is not that owner reaches
    // the handler's `Unauthorized` check instead of failing an address
    // constraint. Closed after the handler returns, rent to `owner`.
    #[account(mut, close(dest = owner))]
    pub market_user: Account<MarketUser>,

    #[account(mut)]
    pub owner: Signer,
}

#[inline(always)]
pub fn handle_close_market_user(
    accounts: &mut CloseMarketUserAccountConstraints,
) -> Result<(), ProgramError> {
    // Canonical-PDA check against the stored owner and bump. A constraint
    // `address = MarketUser::seeds(market.address(), &market_user.owner)` is
    // inexpressible in Quasar 0.1.0 (an Address-typed stored-data seed cannot
    // both feed client codegen and typecheck onchain), so the handler checks
    // it with one sha256 syscall instead.
    quasar_lang::pda::verify_program_address(
        &MarketUser::seeds(accounts.market.address(), &accounts.market_user.owner)
            .with_bump(accounts.market_user.bump)
            .as_slices(),
        &crate::ID,
        accounts.market_user.address(),
    )?;

    let market_user = &accounts.market_user;

    require_keys_eq!(
        market_user.owner,
        *accounts.owner.address(),
        OrderBookError::Unauthorized
    );

    require!(
        market_user.open_orders_len == 0
            && u64::from(market_user.unsettled_base) == 0
            && u64::from(market_user.unsettled_quote) == 0,
        OrderBookError::MarketUserNotClosable
    );

    Ok(())
}
