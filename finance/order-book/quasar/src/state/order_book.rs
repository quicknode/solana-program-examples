use quasar_lang::prelude::ProgramError;

use crate::errors::OrderBookError;
use crate::state::slab::{
    new_node_key, AnyNode, LeafNode, OrderTreeIter, OrderTreeNodes, OrderTreeRoot, OrderTreeType,
    MAX_TREE_NODES, NODE_SIZE,
};
use crate::state::OrderSide;

pub const ORDER_BOOK_SEED: &[u8] = b"order_book";

/// Per-side capacity. 1024 leaves is enough for any realistic depth a single
/// market quotes; at 88 bytes per node that's ~90 KB per side, so the whole
/// OrderBook account fits in ~180 KB - well under Solana's per-account ceiling.
pub const MAX_ORDERS_PER_SIDE: usize = MAX_TREE_NODES;

/// 8-byte marker written at the front of the order-book account so a wrong or
/// uninitialized account can't be cast as a live book. Analogous to Anchor's
/// account discriminator; the value is arbitrary but stable.
pub const ORDER_BOOK_DISCRIMINATOR: [u8; 8] = *b"ORDRBOOK";

/// Combined order book: two critbit trees plus a shared monotonic seq_num
/// counter that gives every order a unique tie-break and acts as the public
/// `order_id`.
///
/// Accessed zero-copy: the handler casts the account's raw byte region to
/// `&mut OrderBook` with `bytemuck`. The account is far larger than a borsh
/// (de)serialization would happily handle on every instruction, so the raw
/// cast pays no per-field cost. This mirrors the Anchor build's
/// `AccountLoader<OrderBook>`; Quasar has no `AccountLoader`, so the cast is
/// done by hand (see `load_order_book*`).
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct OrderBook {
    /// Market this book belongs to (raw 32-byte address). Bound to the market
    /// via the market's stored `order_book` address, checked in each handler.
    pub market: [u8; 32],

    /// Tree roots for the two sides. Kept on this struct (rather than inside
    /// the OrderTreeNodes blobs) so each side's `leaf_count` is cheap to read
    /// without iterating.
    pub bids_root: OrderTreeRoot,
    pub asks_root: OrderTreeRoot,

    /// Monotonic order id. Incremented on every successful place_order, used
    /// both as the public `order_id` and as the seq_num tie-break baked into
    /// each leaf's tree key.
    pub next_order_id: u64,

    pub bump: u8,
    pub _padding: [u8; 7],

    /// The two slabs, back-to-back so the whole struct is one contiguous
    /// zero-copy region.
    pub bids: OrderTreeNodes,
    pub asks: OrderTreeNodes,
}

/// Total on-chain size: discriminator + struct. The client sizes the
/// order-book account to exactly this at `create_account` time.
pub const ORDER_BOOK_ACCOUNT_SIZE: usize = 8 + core::mem::size_of::<OrderBook>();

// Compile-time check: catch accidental layout drift if anyone adds a field
// without recomputing space. 32 (market) + 8 (bids_root) + 8 (asks_root)
// + 8 (next_order_id) + 1 (bump) + 7 (pad) + 2 * (1 + 3 + 4 + 4 + 4 + 88*1024).
const _: () = {
    assert!(
        core::mem::size_of::<OrderBook>()
            == 32 + 8 + 8 + 8 + 1 + 7 + 2 * (1 + 3 + 4 + 4 + 4 + NODE_SIZE * MAX_TREE_NODES)
    );
};

/// Cast an initialized order-book account's bytes to `&OrderBook`.
pub fn load_order_book(data: &[u8]) -> Result<&OrderBook, ProgramError> {
    if data.len() < ORDER_BOOK_ACCOUNT_SIZE {
        return Err(ProgramError::AccountDataTooSmall);
    }
    if data[..8] != ORDER_BOOK_DISCRIMINATOR {
        return Err(OrderBookError::InvalidOrderBook.into());
    }
    bytemuck::try_from_bytes(&data[8..8 + core::mem::size_of::<OrderBook>()])
        .map_err(|_| ProgramError::InvalidAccountData)
}

/// Cast an initialized order-book account's bytes to `&mut OrderBook`.
pub fn load_order_book_mut(data: &mut [u8]) -> Result<&mut OrderBook, ProgramError> {
    if data.len() < ORDER_BOOK_ACCOUNT_SIZE {
        return Err(ProgramError::AccountDataTooSmall);
    }
    if data[..8] != ORDER_BOOK_DISCRIMINATOR {
        return Err(OrderBookError::InvalidOrderBook.into());
    }
    bytemuck::try_from_bytes_mut(&mut data[8..8 + core::mem::size_of::<OrderBook>()])
        .map_err(|_| ProgramError::InvalidAccountData)
}

/// First-time cast of a freshly `create_account`-d (zeroed) order-book account.
/// Verifies the discriminator slot is still zero (not already initialized),
/// stamps the discriminator, and returns the zeroed `&mut OrderBook` for the
/// handler to initialize in place.
pub fn load_order_book_init(data: &mut [u8]) -> Result<&mut OrderBook, ProgramError> {
    if data.len() < ORDER_BOOK_ACCOUNT_SIZE {
        return Err(ProgramError::AccountDataTooSmall);
    }
    if data[..8] != [0u8; 8] {
        return Err(OrderBookError::OrderBookAlreadyInitialized.into());
    }
    data[..8].copy_from_slice(&ORDER_BOOK_DISCRIMINATOR);
    bytemuck::try_from_bytes_mut(&mut data[8..8 + core::mem::size_of::<OrderBook>()])
        .map_err(|_| ProgramError::InvalidAccountData)
}

impl OrderBook {
    /// First-time initialization. Sets the market binding, the order-id
    /// counter, and stamps each slab with its side tag so the iterator knows
    /// which way to walk.
    pub fn initialize(&mut self, market: [u8; 32], bump: u8) {
        self.market = market;
        self.bids_root = OrderTreeRoot::default();
        self.asks_root = OrderTreeRoot::default();
        // Order ids start at 1 so 0 can stand for "no order" in clients.
        self.next_order_id = 1;
        self.bump = bump;
        self._padding = [0; 7];

        // Slab regions arrive zeroed. We only need to write the side tag -
        // every other field already reads as "empty" (bump_index=0,
        // free_list_len=0, free_list_head=0, all node slots Uninitialized).
        self.bids.order_tree_type = OrderTreeType::Bids as u8;
        self.asks.order_tree_type = OrderTreeType::Asks as u8;
    }

    /// Allocate the next order id and roll the counter. Returns the id the
    /// caller should stamp into the new Order PDA.
    pub fn allocate_order_id(&mut self) -> Result<u64, ProgramError> {
        let id = self.next_order_id;
        self.next_order_id = self
            .next_order_id
            .checked_add(1)
            .ok_or(OrderBookError::NumericalOverflow)?;
        Ok(id)
    }

    /// Insert a resting order at `price` on `side`. The seq_num baked into the
    /// leaf's tree key gives price-time priority: at any single price, earlier
    /// orders sort before later ones when the iterator walks the tree.
    pub fn place_resting(
        &mut self,
        side: OrderSide,
        price: u64,
        quantity: u64,
        owner: [u8; 32],
        order_id: u64,
        timestamp: i64,
    ) -> Result<(), ProgramError> {
        let key = new_node_key(side, price, order_id);
        let leaf = LeafNode::new(key, owner, quantity, order_id, timestamp);
        let (root, nodes) = match side {
            OrderSide::Bid => (&mut self.bids_root, &mut self.bids),
            OrderSide::Ask => (&mut self.asks_root, &mut self.asks),
        };
        nodes.insert_leaf(root, &leaf)?;
        Ok(())
    }

    /// Remove a resting order from a specific side. Returns `true` if it was
    /// found and removed. The side is known at cancel time (from the Order
    /// PDA), so no cross-side scan is needed.
    pub fn remove_from(&mut self, side: OrderSide, order_id: u64) -> bool {
        let (root, nodes) = match side {
            OrderSide::Bid => (&mut self.bids_root, &mut self.bids),
            OrderSide::Ask => (&mut self.asks_root, &mut self.asks),
        };

        // Tree keys embed price in the high 64 bits - we don't have the price
        // at cancellation time, so we can't reconstruct the exact key without
        // scanning. Linear scan to find the full key, then remove by key.
        // Cheap relative to a CPI, and only happens at cancellation.
        let mut found_key: Option<u128> = None;
        for (_, leaf) in OrderTreeIter::new(nodes, root) {
            if leaf.order_id == order_id {
                found_key = Some(leaf.key);
                break;
            }
        }
        match found_key {
            Some(key) => nodes.remove_by_key(root, key).is_some(),
            None => false,
        }
    }

    /// Decrease the remaining quantity on a resting leaf, or remove it if the
    /// fill consumes everything. Used by the matching engine to apply fills
    /// against the maker side of the book without reinserting leaves.
    ///
    /// Leaves are looked up by (price, order_id) instead of a cached slab
    /// handle because removing any leaf rebalances the tree - every other
    /// handle in the same plan would be stale after the first removal.
    /// (price, order_id) reconstructs the exact tree key the leaf was
    /// inserted with, so the tree walk lands on the right slot every time.
    pub fn apply_fill_to_maker(
        &mut self,
        maker_side: OrderSide,
        maker_order_id: u64,
        fill_price: u64,
        fill_quantity: u64,
    ) -> Result<(), ProgramError> {
        let key = new_node_key(maker_side, fill_price, maker_order_id);
        let (root, nodes) = match maker_side {
            OrderSide::Bid => (&mut self.bids_root, &mut self.bids),
            OrderSide::Ask => (&mut self.asks_root, &mut self.asks),
        };

        // Look up the leaf to read its remaining quantity, deciding whether to
        // mutate-in-place (partial fill) or remove (full fill).
        let (handle, remaining_after) = {
            let (handle, leaf) = nodes
                .find_by_key(root, key)
                .ok_or(OrderBookError::OrderNotFound)?;
            let remaining = leaf
                .quantity
                .checked_sub(fill_quantity)
                .ok_or(OrderBookError::NumericalOverflow)?;
            (handle, remaining)
        };
        if remaining_after == 0 {
            nodes.remove_by_key(root, key);
        } else {
            // Partial: mutate the leaf's quantity in place. The slab slot
            // index does not change here, so no tree rebalancing occurs and
            // the other leaves' slot positions in the slab stay valid.
            let leaf = nodes
                .node_mut(handle)
                .and_then(AnyNode::as_leaf_mut)
                .ok_or(OrderBookError::OrderNotFound)?;
            leaf.quantity = remaining_after;
        }
        Ok(())
    }

    pub fn is_side_full(&self, side: OrderSide) -> bool {
        match side {
            OrderSide::Bid => self.bids.is_full(),
            OrderSide::Ask => self.asks.is_full(),
        }
    }
}
