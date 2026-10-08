use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::{remaining_quantity, Market, Order, OrderSide};

pub const MARKET_USER_SEED: &[u8] = b"market_user";

// Per-user, per-market account. Tracks open order ids and amounts owed back
// to the user (unsettled_*). Settlement moves those amounts from the vaults
// to the user's token accounts in settle_funds.
#[derive(InitSpace)]
#[account(borsh)]
pub struct MarketUser {
    pub market: Address,

    pub owner: Address,

    pub unsettled_base: u64,

    pub unsettled_quote: u64,

    // 20 is chosen to match the matching engine's upper bound: a single user
    // shouldn't be able to spam the book. Keep the cap in sync with the
    // TooManyOpenOrders check in place_order.
    #[max_len(20)]
    pub open_orders: Vec<u64>,

    pub bump: u8,
}

pub fn add_open_order(account: &mut MarketUser, order_id: u64) {
    if !account.open_orders.contains(&order_id) {
        account.open_orders.push(order_id);
    }
}

pub fn remove_open_order(account: &mut MarketUser, order_id: u64) {
    if let Some(position) = account.open_orders.iter().position(|&id| id == order_id) {
        account.open_orders.remove(position);
    }
}

/// Credit the owner of a resting `order` with the funds its unfilled
/// remainder still has locked in the vault: quote for a bid, base for an ask.
/// `settle_funds` later moves the credit to their token account. Used by
/// `cancel_order`, and by `place_order` when it evicts an order from a full
/// side. The arithmetic mirrors the lock in `place_order`, in u128 so
/// high-decimal mints cannot overflow the intermediate product.
pub fn credit_unfilled_lock(
    market: &Market,
    order: &Order,
    market_user: &mut MarketUser,
) -> Result<()> {
    let remaining = remaining_quantity(order);
    if remaining == 0 {
        return Ok(());
    }
    match order.side {
        OrderSide::Bid => {
            let quote_amount: u64 = (order.price as u128)
                .checked_mul(remaining as u128)
                .ok_or(ErrorCode::NumericalOverflow)?
                .checked_mul(market.quote_lot_size as u128)
                .ok_or(ErrorCode::NumericalOverflow)?
                .try_into()
                .map_err(|_| ErrorCode::NumericalOverflow)?;
            market_user.unsettled_quote = market_user
                .unsettled_quote
                .checked_add(quote_amount)
                .ok_or(ErrorCode::NumericalOverflow)?;
        }
        OrderSide::Ask => {
            let base_amount: u64 = (remaining as u128)
                .checked_mul(market.base_lot_size as u128)
                .ok_or(ErrorCode::NumericalOverflow)?
                .try_into()
                .map_err(|_| ErrorCode::NumericalOverflow)?;
            market_user.unsettled_base = market_user
                .unsettled_base
                .checked_add(base_amount)
                .ok_or(ErrorCode::NumericalOverflow)?;
        }
    }
    Ok(())
}
