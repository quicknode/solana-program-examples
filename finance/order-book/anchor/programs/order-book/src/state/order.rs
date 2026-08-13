use anchor_lang::prelude::*;

pub const ORDER_SEED: &[u8] = b"order";

#[derive(Clone, Copy, PartialEq, Eq, InitSpace, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub enum OrderSide {
    Bid,
    Ask,
}

#[derive(Clone, Copy, PartialEq, Eq, InitSpace, IdlType, wincode::SchemaRead, wincode::SchemaWrite)]
pub enum OrderStatus {
    Open,
    PartiallyFilled,
    Filled,
    Cancelled,
}

#[derive(InitSpace)]
#[account(borsh)]
pub struct Order {
    pub market: Address,

    pub owner: Address,

    pub order_id: u64,

    pub side: OrderSide,

    pub price: u64,

    pub original_quantity: u64,

    pub filled_quantity: u64,

    pub status: OrderStatus,

    pub timestamp: i64,

    pub bump: u8,
}

pub fn remaining_quantity(order: &Order) -> u64 {
    order.original_quantity.saturating_sub(order.filled_quantity)
}
