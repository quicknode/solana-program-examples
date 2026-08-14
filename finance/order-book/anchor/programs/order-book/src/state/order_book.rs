use anchor_lang::prelude::*;

use crate::errors::ErrorCode;
use crate::state::slab::{
    new_node_key, AnyNode, LeafNode, OrderTreeIter, OrderTreeNodes, OrderTreeRoot, OrderTreeType,
    MAX_TREE_NODES, NODE_SIZE,
};
use crate::state::OrderSide;

pub const ORDER_BOOK_SEED: &[u8] = b"order_book";

/// Per-side capacity. 1024 leaves is enough for any realistic depth a single
/// market quotes; at 88 bytes per node that's ~90 KB per side, so the whole
/// OrderBook account fits in ~180 KB - well under Solana's per-account ceiling
/// and well within the rent budget a market authority is happy to fund once.
pub const MAX_ORDERS_PER_SIDE: usize = MAX_TREE_NODES;

/// Combined order book: two critbit trees plus a shared monotonic seq_num
/// counter that gives every order a unique tie-break and acts as the public
/// `order_id`.
///
/// Stored as one `AccountLoader<OrderBook>` (zero-copy). The account is far
/// larger than Anchor's borsh `Account<T>` would happily deserialize on every
/// instruction - zero-copy gives us per-field memory access without paying
/// the (de)serialization cost.
#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct OrderBook {
    /// Market PDA this book belongs to. Constrained onchain via the
    /// `market` `has_one = order_book` (and vice-versa) bindings.
    pub market: Address,

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

    /// The two slabs. Layout-wise they're back-to-back so the whole struct
    /// is one contiguous zero-copy region.
    pub bids: OrderTreeNodes,
    pub asks: OrderTreeNodes,
}

// Sanity check: catch any accidental layout drift if someone adds a field
// without recomputing space. Account discriminator + struct itself.
pub const ORDER_BOOK_ACCOUNT_SIZE: usize = 8 + std::mem::size_of::<OrderBook>();

// Compile-time check: the struct shouldn't quietly balloon past its expected
// ~180 KB if anyone adds fields. 32 (market) + 8 (bids_root) + 8 (asks_root)
// + 8 (next_order_id) + 1 (bump) + 7 (pad) + 2 * (12 + 88*1024)
// = 64 + 2 * 90124 = 180312 bytes for the struct itself.
const _: () = {
    assert!(
        std::mem::size_of::<OrderBook>()
            == 32 + 8 + 8 + 8 + 1 + 7 + 2 * (1 + 3 + 4 + 4 + 4 + NODE_SIZE * MAX_TREE_NODES)
    );
};

/// A compact view of one resting order for the matching engine. Returned
/// from the tree iterator so callers don't have to hand-roll slab access.
pub struct RestingOrderView {
    pub order_id: u64,
    pub price: u64,
    pub quantity: u64,
    pub owner: Address,
}

impl OrderBook {
    /// First-time initialization. Sets the market binding, the order-id
    /// counter, and stamps each slab with its side tag so the iterator knows
    /// which way to walk.
    pub fn initialize(&mut self, market: Address, bump: u8) {
        self.market = market;
        self.bids_root = OrderTreeRoot::default();
        self.asks_root = OrderTreeRoot::default();
        // Order ids start at 1 so 0 can stand for "no order" in clients.
        self.next_order_id = 1;
        self.bump = bump;
        self._padding = [0; 7];

        // Slab regions arrive zeroed (Anchor zero_copy guarantees that on
        // first init). We only need to write the side tag - every other
        // field already reads as "empty" (bump_index=0, free_list_len=0,
        // free_list_head=0, all node slots Uninitialized).
        self.bids.order_tree_type = OrderTreeType::Bids as u8;
        self.asks.order_tree_type = OrderTreeType::Asks as u8;
    }

    /// Allocate the next order id and roll the counter. Returns the id the
    /// caller should stamp into the new Order PDA.
    pub fn allocate_order_id(&mut self) -> Result<u64> {
        let id = self.next_order_id;
        self.next_order_id = self
            .next_order_id
            .checked_add(1)
            .ok_or_else(|| error!(ErrorCode::NumericalOverflow))?;
        Ok(id)
    }

    /// Insert a resting order at `price` on `side`. The seq_num baked into
    /// the leaf's tree key gives price-time priority: at any single price,
    /// earlier orders sort before later ones when the iterator walks the
    /// tree.
    pub fn place_resting(
        &mut self,
        side: OrderSide,
        price: u64,
        quantity: u64,
        owner: Address,
        order_id: u64,
        timestamp: i64,
    ) -> Result<()> {
        let key = new_node_key(side, price, order_id);
        let leaf = LeafNode::new(key, owner, quantity, order_id, timestamp);
        let (root, nodes) = match side {
            OrderSide::Bid => (&mut self.bids_root, &mut self.bids),
            OrderSide::Ask => (&mut self.asks_root, &mut self.asks),
        };
        nodes.insert_leaf(root, &leaf)?;
        Ok(())
    }

    /// Remove a resting order by `order_id`. Returns `true` if it was found
    /// and removed on either side.
    pub fn remove(&mut self, order_id: u64) -> bool {
        // We don't know which side the order is on without scanning, so try
        // both. Each side does a linear scan over its node arena to find the
        // full key (price is not known at cancellation time), then removes
        // by key - see `remove_from`.
        if self.remove_from(OrderSide::Bid, order_id).is_some() {
            return true;
        }
        if self.remove_from(OrderSide::Ask, order_id).is_some() {
            return true;
        }
        false
    }

    /// Remove a resting order from a specific side. Returns the removed leaf
    /// if found, so callers can read its quantity/owner without re-fetching
    /// the Order account.
    pub fn remove_from(&mut self, side: OrderSide, order_id: u64) -> Option<LeafNode> {
        // Tree keys embed price in the high 64 bits - we don't have the price
        // at cancellation time, so we can't reconstruct the exact key without
        // scanning. Linear scan to find the full key, then remove by key.
        let (root, nodes) = match side {
            OrderSide::Bid => (&mut self.bids_root, &mut self.bids),
            OrderSide::Ask => (&mut self.asks_root, &mut self.asks),
        };

        // Linear scan to find the full key (price + seq_num) for this
        // order_id. Cheap relative to a CPI - and only happens at
        // cancellation, not in the hot matching path.
        let mut found_key: Option<u128> = None;
        for (_, leaf) in OrderTreeIter::new(nodes, root) {
            if leaf.order_id == order_id {
                found_key = Some(leaf.key);
                break;
            }
        }
        let key = found_key?;
        nodes.remove_by_key(root, key)
    }

    /// Resting-order view of the best (best-priced) leaf on `side`, if any.
    pub fn best(&self, side: OrderSide) -> Option<RestingOrderView> {
        let (root, nodes) = match side {
            OrderSide::Bid => (&self.bids_root, &self.bids),
            OrderSide::Ask => (&self.asks_root, &self.asks),
        };
        let (_handle, leaf) = nodes.best_leaf(root)?;
        Some(RestingOrderView {
            order_id: leaf.order_id,
            price: leaf.price(),
            quantity: leaf.quantity,
            owner: leaf.owner,
        })
    }

    /// Number of resting orders on a side. O(1).
    pub fn count(&self, side: OrderSide) -> u32 {
        match side {
            OrderSide::Bid => self.bids_root.leaf_count,
            OrderSide::Ask => self.asks_root.leaf_count,
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
    ) -> Result<()> {
        let key = new_node_key(maker_side, fill_price, maker_order_id);
        let (root, nodes) = match maker_side {
            OrderSide::Bid => (&mut self.bids_root, &mut self.bids),
            OrderSide::Ask => (&mut self.asks_root, &mut self.asks),
        };

        // Look up the leaf to read its remaining quantity. We need to know
        // whether to mutate-in-place (partial fill) or remove (full fill).
        let (handle, remaining_after) = {
            let (handle, leaf) = nodes
                .find_by_key(root, key)
                .ok_or_else(|| error!(ErrorCode::OrderNotFound))?;
            let remaining = leaf
                .quantity
                .checked_sub(fill_quantity)
                .ok_or_else(|| error!(ErrorCode::NumericalOverflow))?;
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
                .ok_or_else(|| error!(ErrorCode::OrderNotFound))?;
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

impl Default for OrderTreeRoot {
    fn default() -> Self {
        Self {
            maybe_node: 0,
            leaf_count: 0,
        }
    }
}
