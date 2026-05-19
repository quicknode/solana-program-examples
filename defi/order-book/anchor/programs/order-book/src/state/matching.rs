//! Matching engine helpers. Pure logic (no CPIs) that walks the resting side
//! of the book in price-time priority and produces a list of fills the
//! caller should apply.
//!
//! Walking the tree returns leaves in best-price-first order (asks
//! ascending, bids descending), and within a single price level the leaf
//! key's seq_num half preserves time priority, so a straight `next()` walk
//! is the right traversal for matching. Stop as soon as either the taker is
//! exhausted or the next leaf no longer crosses the taker's limit price.

use crate::state::slab::OrderTreeIter;
use crate::state::{OrderBook, OrderSide};

/// One matched fill between the incoming taker order and a resting maker
/// order. `place_order` turns these into token movements and account
/// mutations.
///
/// Deliberately does NOT carry the slab handle of the maker leaf: removing
/// any leaf rebalances the tree, which invalidates other handles in the
/// same plan. We look the leaf up by its tree key (built from price +
/// order_id) at apply time instead.
pub struct Fill {
    /// order_id of the resting order being filled. Used both to sanity-
    /// check the maker `Order` PDA the caller passed in remaining_accounts,
    /// and to reconstruct the tree key for the apply-time lookup.
    pub maker_order_id: u64,

    /// Quantity filled (in base tokens).
    pub fill_quantity: u64,

    /// Price at which the fill clears. Always the resting (maker) order's
    /// price — standard CLOB rule: maker's posted price wins; the taker
    /// gets price improvement vs their limit on bids, and a higher payout
    /// vs their limit on asks. Also the high 64 bits of the tree key when
    /// we look the leaf up again at apply time.
    pub fill_price: u64,
}

/// Walk the opposite side of the book and produce the list of fills that
/// should occur for the incoming taker order. Does not mutate the book.
///
/// Returns `(fills, taker_remaining)` — `taker_remaining` is what's left
/// over after crossing, to be rested on the book at the taker's limit price.
pub fn plan_fills(
    order_book: &OrderBook,
    incoming_side: OrderSide,
    incoming_price: u64,
    incoming_quantity: u64,
) -> (Vec<Fill>, u64) {
    // Resting side is the opposite of the taker's side.
    let (root, nodes) = match incoming_side {
        OrderSide::Bid => (&order_book.asks_root, &order_book.asks),
        OrderSide::Ask => (&order_book.bids_root, &order_book.bids),
    };

    let mut fills: Vec<Fill> = Vec::new();
    let mut taker_remaining = incoming_quantity;

    for (_handle, leaf) in OrderTreeIter::new(nodes, root) {
        if taker_remaining == 0 {
            break;
        }

        // Crossing condition: bid takes when its limit is >= the resting
        // ask's price; ask takes when its limit is <= the resting bid's
        // price. The tree walk is in best-price-first order on the resting
        // side, so the first leaf that fails to cross means every
        // subsequent leaf also fails — break, don't continue.
        let resting_price = leaf.price();
        let crosses = match incoming_side {
            OrderSide::Bid => incoming_price >= resting_price,
            OrderSide::Ask => incoming_price <= resting_price,
        };
        if !crosses {
            break;
        }

        if leaf.quantity == 0 {
            // Defensive: fully-filled leaves are removed, but if one ever
            // slips through with zero quantity, skip it rather than emit a
            // zero-size fill.
            continue;
        }

        let fill_quantity = taker_remaining.min(leaf.quantity);
        fills.push(Fill {
            maker_order_id: leaf.order_id,
            fill_quantity,
            fill_price: resting_price,
        });
        taker_remaining = taker_remaining.saturating_sub(fill_quantity);
    }

    (fills, taker_remaining)
}
