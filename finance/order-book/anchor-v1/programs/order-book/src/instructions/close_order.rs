use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::{Market, Order, OrderStatus, ORDER_SEED};

/// Close a finished order's account and return its rent to the owner who
/// paid it. An order is finished once its status is `Filled` or
/// `Cancelled`: both paths have already removed its leaf from the slab and
/// its id from the owner's `open_orders`, and a cancel has already credited
/// the unfilled remainder to the owner's unsettled balance, so nothing on
/// the book or in the vaults still refers to the account. An `Open` or
/// `PartiallyFilled` order still rests on the book and is refused with
/// `OrderNotClosable`; cancel it first.
pub fn handle_close_order(context: Context<CloseOrderAccountConstraints>) -> Result<()> {
    let order = &context.accounts.order;

    require!(
        order.owner == context.accounts.owner.key(),
        ErrorCode::Unauthorized
    );

    require!(
        order.status == OrderStatus::Filled || order.status == OrderStatus::Cancelled,
        ErrorCode::OrderNotClosable
    );

    Ok(())
}

#[derive(Accounts)]
pub struct CloseOrderAccountConstraints<'info> {
    pub market: Account<'info, Market>,

    // Closed by Anchor after the handler returns, rent to `owner`. The owner
    // check is in the handler so the refusal is this program's own
    // `Unauthorized` rather than a seeds mismatch.
    #[account(
        mut,
        close = owner,
        seeds = [ORDER_SEED, market.key().as_ref(), order.order_id.to_le_bytes().as_ref()],
        bump = order.bump
    )]
    pub order: Account<'info, Order>,

    #[account(mut)]
    pub owner: Signer<'info>,
}
