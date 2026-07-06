use quasar_lang::prelude::*;

pub const MARKET_USER_SEED: &[u8] = b"market_user";

/// Per-user open-order cap. Matches the matching engine's upper bound so a
/// single user can't spam the book. Kept in sync with the `TooManyOpenOrders`
/// check in `place_order`.
pub const MAX_OPEN_ORDERS: usize = 20;

/// Byte length of the packed open-order-id list: `MAX_OPEN_ORDERS` u64s.
pub const OPEN_ORDERS_BYTES: usize = MAX_OPEN_ORDERS * 8;

/// Per-user, per-market account. Tracks open order ids and amounts owed back to
/// the user (`unsettled_*`). Settlement moves those amounts from the vaults to
/// the user's token accounts in `settle_funds`.
///
/// PDA: `["market_user", market, owner]`.
///
/// The Anchor build stores open order ids in a borsh `Vec<u64>`. Quasar
/// accounts are zero-copy, and a resizable `Vec` field would make the account
/// dynamic - awkward to mutate on the *maker* side, where the account arrives
/// as a remaining account. Instead the ids live in a fixed `[u8; 160]` buffer
/// (20 little-endian u64s) with an explicit `open_orders_len`, keeping the
/// account fixed-size so both taker and maker mutations are plain in-place
/// writes.
#[account(discriminator = 2, set_inner)]
#[seeds(b"market_user", market: Address, owner: Address)]
pub struct MarketUser {
    pub market: Address,
    pub owner: Address,
    pub unsettled_base: u64,
    pub unsettled_quote: u64,
    pub open_orders_len: u8,
    pub bump: u8,
    pub open_orders: [u8; OPEN_ORDERS_BYTES],
}

fn read_id(open_orders: &[u8; OPEN_ORDERS_BYTES], index: usize) -> u64 {
    let start = index * 8;
    let mut bytes = [0u8; 8];
    bytes.copy_from_slice(&open_orders[start..start + 8]);
    u64::from_le_bytes(bytes)
}

fn write_id(open_orders: &mut [u8; OPEN_ORDERS_BYTES], index: usize, order_id: u64) {
    let start = index * 8;
    open_orders[start..start + 8].copy_from_slice(&order_id.to_le_bytes());
}

pub fn open_orders_contains(
    open_orders: &[u8; OPEN_ORDERS_BYTES],
    open_orders_len: u8,
    order_id: u64,
) -> bool {
    (0..open_orders_len as usize).any(|index| read_id(open_orders, index) == order_id)
}

/// Append `order_id` if there's room and it isn't already tracked. A full list
/// or a duplicate is a silent no-op - the `TooManyOpenOrders` cap is enforced
/// separately in `place_order` before the order rests.
pub fn add_open_order(
    open_orders: &mut [u8; OPEN_ORDERS_BYTES],
    open_orders_len: &mut u8,
    order_id: u64,
) {
    let len = *open_orders_len as usize;
    if len >= MAX_OPEN_ORDERS || open_orders_contains(open_orders, *open_orders_len, order_id) {
        return;
    }
    write_id(open_orders, len, order_id);
    *open_orders_len = (len + 1) as u8;
}

/// Remove `order_id` if present by swapping in the last entry (order within the
/// list is not significant - it's a set). No-op if the id isn't tracked.
pub fn remove_open_order(
    open_orders: &mut [u8; OPEN_ORDERS_BYTES],
    open_orders_len: &mut u8,
    order_id: u64,
) {
    let len = *open_orders_len as usize;
    let Some(position) = (0..len).find(|&index| read_id(open_orders, index) == order_id) else {
        return;
    };
    let last = len - 1;
    if position != last {
        let moved = read_id(open_orders, last);
        write_id(open_orders, position, moved);
    }
    write_id(open_orders, last, 0);
    *open_orders_len = last as u8;
}

/// Copy the account's current state into an owned `MarketUserInner` so a
/// handler can mutate it and write it back with `set_inner`.
pub fn snapshot_market_user(market_user: &Account<MarketUser>) -> MarketUserInner {
    MarketUserInner {
        market: market_user.market,
        owner: market_user.owner,
        unsettled_base: u64::from(market_user.unsettled_base),
        unsettled_quote: u64::from(market_user.unsettled_quote),
        open_orders_len: market_user.open_orders_len,
        bump: market_user.bump,
        open_orders: market_user.open_orders,
    }
}
