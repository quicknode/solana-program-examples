use quasar_lang::prelude::*;

pub const ORDER_SEED: &[u8] = b"order";

/// Side of the book an order sits on. Stored on-chain as a `u8` (Quasar
/// zero-copy accounts hold POD scalars, not Rust enums); the instruction wire
/// format also encodes `side` as a single byte, matching the Anchor build's
/// borsh enum discriminant. `0 = Bid`, `1 = Ask`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OrderSide {
    Bid = 0,
    Ask = 1,
}

impl OrderSide {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(OrderSide::Bid),
            1 => Some(OrderSide::Ask),
            _ => None,
        }
    }

    pub fn opposite(self) -> Self {
        match self {
            OrderSide::Bid => OrderSide::Ask,
            OrderSide::Ask => OrderSide::Bid,
        }
    }
}

/// Lifecycle of an order. Stored on-chain as a `u8`.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum OrderStatus {
    Open = 0,
    PartiallyFilled = 1,
    Filled = 2,
    Cancelled = 3,
}

/// A single order. PDA: `["order", market, order_id]`. The order id is the
/// book's monotonic `next_order_id` at placement time, so each order gets a
/// unique, deterministic address.
#[account(discriminator = 3, set_inner)]
#[seeds(b"order", market: Address, order_id: u64)]
pub struct Order {
    pub market: Address,
    pub owner: Address,
    pub order_id: u64,
    pub side: u8,
    pub price: u64,
    pub original_quantity: u64,
    pub filled_quantity: u64,
    pub status: u8,
    pub timestamp: i64,
    pub bump: u8,
}

/// Base lots still resting: original minus filled. Saturating because a
/// cosmetic read should never trap; the matching engine keeps
/// `filled_quantity <= original_quantity` by construction.
pub fn remaining_quantity(original_quantity: u64, filled_quantity: u64) -> u64 {
    original_quantity.saturating_sub(filled_quantity)
}

/// Copy an order account's current state into an owned `OrderInner` so a
/// handler can mutate it and write it back with `set_inner`.
pub fn snapshot_order(order: &Account<Order>) -> OrderInner {
    OrderInner {
        market: order.market,
        owner: order.owner,
        order_id: u64::from(order.order_id),
        side: order.side,
        price: u64::from(order.price),
        original_quantity: u64::from(order.original_quantity),
        filled_quantity: u64::from(order.filled_quantity),
        status: order.status,
        timestamp: i64::from(order.timestamp),
        bump: order.bump,
    }
}
