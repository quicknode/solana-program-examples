use quasar_lang::prelude::*;

use crate::errors::OrderBookError;
use crate::state::{Market, Order, OrderStatus};

/// Close a finished order's account and return its rent to the owner who
/// paid it. An order is finished once its status is `Filled` or
/// `Cancelled`: both paths have already removed its leaf from the slab and
/// its id from the owner's `open_orders`, and a cancel has already credited
/// the unfilled remainder to the owner's unsettled balance, so nothing on
/// the book or in the vaults still refers to the account. An `Open` or
/// `PartiallyFilled` order still rests on the book and is refused with
/// `OrderNotClosable`; cancel it first.
#[derive(Accounts)]
pub struct CloseOrderAccountConstraints {
    pub market: Account<Market>,

    // Closed after the handler returns, rent to `owner`. The owner check is
    // in the handler so the refusal is this program's own `Unauthorized`
    // rather than a seeds mismatch.
    #[account(
        mut,
        close(dest = owner),
        address = Order::seeds(market.address(), order.order_id.into())
    )]
    pub order: Account<Order>,

    #[account(mut)]
    pub owner: Signer,
}

#[inline(always)]
pub fn handle_close_order(accounts: &mut CloseOrderAccountConstraints) -> Result<(), ProgramError> {
    let order = &accounts.order;

    require_keys_eq!(
        order.owner,
        *accounts.owner.address(),
        OrderBookError::Unauthorized
    );

    require!(
        order.status == OrderStatus::Filled as u8 || order.status == OrderStatus::Cancelled as u8,
        OrderBookError::OrderNotClosable
    );

    Ok(())
}
