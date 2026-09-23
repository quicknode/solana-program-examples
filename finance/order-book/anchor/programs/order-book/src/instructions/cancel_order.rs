use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::{
    credit_unfilled_lock, remove_open_order, Market, MarketUser, Order, OrderBook, OrderStatus,
    MARKET_USER_SEED, ORDER_SEED,
};

pub fn handle_cancel_order(context: &mut Context<CancelOrderAccountConstraints>) -> Result<()> {
    let order = &mut context.accounts.order;

    require!(
        order.owner == *context.accounts.owner.address(),
        ErrorCode::Unauthorized
    );

    require!(
        order.status == OrderStatus::Open || order.status == OrderStatus::PartiallyFilled,
        ErrorCode::OrderNotCancellable
    );

    // Funds the order had locked in the vault are now owed back to the
    // owner. Credit the appropriate unsettled balance; settle_funds moves
    // those funds from the vault to the owner's token account.
    credit_unfilled_lock(
        &context.accounts.market,
        order,
        &mut context.accounts.market_user,
    )?;

    // Remove the leaf from the slab. The current cancel API doesn't tell us
    // which side the order is on without reading the Order PDA - which we
    // already have, so use it.
    let removed = context
        .accounts
        .order_book
        .remove_from(order.side, order.order_id)
        .is_some();
    require!(removed, ErrorCode::OrderNotFound);

    let market_user = &mut context.accounts.market_user;
    remove_open_order(market_user, order.order_id);

    order.status = OrderStatus::Cancelled;

    Ok(())
}

#[derive(Accounts)]
pub struct CancelOrderAccountConstraints {
    pub market: BorshAccount<Market>,

    // Not a PDA (see initialize_market.rs); bound to `market` via `address`.
    #[account(mut, address = market.order_book @ ErrorCode::InvalidOrderBook)]
    pub order_book: Account<OrderBook>,

    #[account(
        mut,
        seeds = [ORDER_SEED, market.address().as_ref(), order.order_id.to_le_bytes()],
        bump = order.bump
    )]
    pub order: BorshAccount<Order>,

    #[account(
        mut,
        seeds = [MARKET_USER_SEED, market.address().as_ref(), owner.address().as_ref()],
        bump = market_user.bump
    )]
    pub market_user: BorshAccount<MarketUser>,

    pub owner: Signer,
}
