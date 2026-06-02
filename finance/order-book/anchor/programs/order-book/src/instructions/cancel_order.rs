use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::{
    remaining_quantity, remove_open_order, Market, Order, OrderBook, OrderSide, OrderStatus,
    MarketUser, ORDER_SEED, MARKET_USER_SEED,
};

pub fn handle_cancel_order(context: Context<CancelOrder>) -> Result<()> {
    let order = &mut context.accounts.order;

    require!(
        order.owner == context.accounts.owner.key(),
        ErrorCode::Unauthorized
    );

    require!(
        order.status == OrderStatus::Open || order.status == OrderStatus::PartiallyFilled,
        ErrorCode::OrderNotCancellable
    );

    // Funds the order had locked in the vault are now owed back to the
    // owner. Credit the appropriate unsettled balance; settle_funds moves
    // those funds from the vault to the owner's token account.
    let remaining = remaining_quantity(order);
    if remaining > 0 {
        let market_user = &mut context.accounts.market_user;
        match order.side {
            OrderSide::Bid => {
                // u128 intermediate: the lock was originally taken on a
                // u64 quote balance, so price * remaining must fit u64
                // — but the multiplication itself can transiently exceed
                // u64. Mirror the same pattern as place_order: widen,
                // multiply, narrow.
                let quote_amount: u64 = (order.price as u128)
                    .checked_mul(remaining as u128)
                    .ok_or(ErrorCode::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| error!(ErrorCode::NumericalOverflow))?;
                market_user.unsettled_quote = market_user
                    .unsettled_quote
                    .checked_add(quote_amount)
                    .ok_or(ErrorCode::NumericalOverflow)?;
            }
            OrderSide::Ask => {
                market_user.unsettled_base = market_user
                    .unsettled_base
                    .checked_add(remaining)
                    .ok_or(ErrorCode::NumericalOverflow)?;
            }
        }
    }

    // Remove the leaf from the slab. The current cancel API doesn't tell us
    // which side the order is on without reading the Order PDA — which we
    // already have, so use it.
    let mut order_book = context.accounts.order_book.load_mut()?;
    let removed = order_book.remove_from(order.side, order.order_id).is_some();
    require!(removed, ErrorCode::OrderNotFound);
    drop(order_book);

    let market_user = &mut context.accounts.market_user;
    remove_open_order(market_user, order.order_id);

    order.status = OrderStatus::Cancelled;

    Ok(())
}

#[derive(Accounts)]
pub struct CancelOrder<'info> {
    #[account(has_one = order_book @ ErrorCode::InvalidOrderBook)]
    pub market: Account<'info, Market>,

    // Not a PDA (see initialize_market.rs); bound to `market` via has_one.
    #[account(mut)]
    pub order_book: AccountLoader<'info, OrderBook>,

    #[account(
        mut,
        seeds = [ORDER_SEED, market.key().as_ref(), order.order_id.to_le_bytes().as_ref()],
        bump = order.bump
    )]
    pub order: Account<'info, Order>,

    #[account(
        mut,
        seeds = [MARKET_USER_SEED, market.key().as_ref(), owner.key().as_ref()],
        bump = market_user.bump
    )]
    pub market_user: Account<'info, MarketUser>,

    pub owner: Signer<'info>,
}
