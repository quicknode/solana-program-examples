use quasar_lang::prelude::*;

use crate::errors::OrderBookError;
use crate::state::{
    load_order_book_mut, remaining_quantity, remove_open_order, snapshot_market_user,
    snapshot_order, Market, MarketUser, Order, OrderSide, OrderStatus,
};

#[derive(Accounts)]
pub struct CancelOrderAccountConstraints {
    #[account(has_one(order_book) @ OrderBookError::InvalidOrderBook)]
    pub market: Account<Market>,

    // Not a PDA (see initialize_market); bound to `market` via has_one.
    #[account(mut)]
    pub order_book: UncheckedAccount,

    #[account(mut, address = Order::seeds(market.address(), order.order_id.into()))]
    pub order: Account<Order>,

    #[account(mut, address = MarketUser::seeds(market.address(), owner.address()))]
    pub market_user: Account<MarketUser>,

    pub owner: Signer,
}

#[inline(always)]
pub fn handle_cancel_order(
    accounts: &mut CancelOrderAccountConstraints,
) -> Result<(), ProgramError> {
    let mut order = snapshot_order(&accounts.order);

    require_keys_eq!(order.owner, *accounts.owner.address(), OrderBookError::Unauthorized);

    require!(
        order.status == OrderStatus::Open as u8
            || order.status == OrderStatus::PartiallyFilled as u8,
        OrderBookError::OrderNotCancellable
    );

    let side = OrderSide::from_u8(order.side).ok_or(OrderBookError::OrderNotCancellable)?;

    // Funds the order had locked in the vault are now owed back to the owner.
    // Credit the appropriate unsettled balance; settle_funds moves those funds
    // from the vault to the owner's token account.
    let remaining = remaining_quantity(order.original_quantity, order.filled_quantity);
    if remaining > 0 {
        let quote_lot_size = u64::from(accounts.market.quote_lot_size);
        let base_lot_size = u64::from(accounts.market.base_lot_size);
        let mut market_user = snapshot_market_user(&accounts.market_user);
        match side {
            OrderSide::Bid => {
                // raw_quote = price × remaining × quote_lot_size (u128 to
                // mirror the bid-lock formula in place_order).
                let quote_amount: u64 = (order.price as u128)
                    .checked_mul(remaining as u128)
                    .ok_or(OrderBookError::NumericalOverflow)?
                    .checked_mul(quote_lot_size as u128)
                    .ok_or(OrderBookError::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| OrderBookError::NumericalOverflow)?;
                market_user.unsettled_quote = market_user
                    .unsettled_quote
                    .checked_add(quote_amount)
                    .ok_or(OrderBookError::NumericalOverflow)?;
            }
            OrderSide::Ask => {
                let base_amount: u64 = (remaining as u128)
                    .checked_mul(base_lot_size as u128)
                    .ok_or(OrderBookError::NumericalOverflow)?
                    .try_into()
                    .map_err(|_| OrderBookError::NumericalOverflow)?;
                market_user.unsettled_base = market_user
                    .unsettled_base
                    .checked_add(base_amount)
                    .ok_or(OrderBookError::NumericalOverflow)?;
            }
        }
        remove_open_order(
            &mut market_user.open_orders,
            &mut market_user.open_orders_len,
            order.order_id,
        );
        accounts.market_user.set_inner(market_user);
    } else {
        // No locked remainder, but the id is still tracked as open - drop it.
        let mut market_user = snapshot_market_user(&accounts.market_user);
        remove_open_order(
            &mut market_user.open_orders,
            &mut market_user.open_orders_len,
            order.order_id,
        );
        accounts.market_user.set_inner(market_user);
    }

    // Remove the leaf from the slab. The side comes from the Order account, so
    // no cross-side scan is needed.
    {
        let view = accounts.order_book.to_account_view();
        let data =
            unsafe { core::slice::from_raw_parts_mut(view.data_ptr() as *mut u8, view.data_len()) };
        let order_book = load_order_book_mut(data)?;
        let removed = order_book.remove_from(side, order.order_id);
        require!(removed, OrderBookError::OrderNotFound);
    }

    order.status = OrderStatus::Cancelled as u8;
    accounts.order.set_inner(order);

    Ok(())
}
