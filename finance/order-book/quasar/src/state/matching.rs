//! Matching engine helpers. Pure logic (no CPIs) that walks the resting side
//! of the book in price-time priority and produces the list of fills the
//! caller should apply.
//!
//! Walking the tree returns leaves in best-price-first order (asks ascending,
//! bids descending), and within a single price level the leaf key's seq_num
//! half preserves time priority, so a straight `next()` walk is the right
//! traversal for matching. Stop as soon as either the taker is exhausted or
//! the next leaf no longer crosses the taker's limit price.

use crate::state::slab::OrderTreeIter;
use crate::state::{OrderBook, OrderSide};

/// Maximum resting orders one `place_order` can cross in a single call. Each
/// fill needs a `(maker_order, maker_market_user)` account pair supplied as
/// remaining accounts, so the real ceiling is the transaction's account limit;
/// this fixed cap keeps `plan_fills` heap-free. Any taker remainder past the
/// cap simply rests on the book to be crossed by a later order.
pub const MAX_FILLS: usize = 16;

/// One matched fill between the incoming taker order and a resting maker order.
/// `place_order` turns these into token movements and account mutations.
///
/// Deliberately does NOT carry the slab handle of the maker leaf: removing any
/// leaf rebalances the tree, invalidating other handles in the same plan. We
/// look the leaf up by its tree key (built from price + order_id) at apply
/// time instead.
#[derive(Copy, Clone, Default)]
pub struct Fill {
    /// order_id of the resting order being filled. Used both to sanity-check
    /// the maker `Order` account the caller passed, and to reconstruct the
    /// tree key for the apply-time lookup.
    pub maker_order_id: u64,

    /// Quantity filled (in base lots).
    pub fill_quantity: u64,

    /// Price at which the fill clears. Always the resting (maker) order's
    /// price - standard order-book rule: the maker's posted price wins.
    pub fill_price: u64,
}

/// Output of `plan_fills`: the fills to apply (first `count` entries of
/// `fills`) and the taker quantity left over to rest on the book.
pub struct FillPlan {
    pub fills: [Fill; MAX_FILLS],
    pub count: usize,
    pub taker_remaining: u64,
}

/// Walk the opposite side of the book and produce the fills that should occur
/// for the incoming taker order. Does not mutate the book.
pub fn plan_fills(
    order_book: &OrderBook,
    incoming_side: OrderSide,
    incoming_price: u64,
    incoming_quantity: u64,
) -> FillPlan {
    // Resting side is the opposite of the taker's side.
    let (root, nodes) = match incoming_side {
        OrderSide::Bid => (&order_book.asks_root, &order_book.asks),
        OrderSide::Ask => (&order_book.bids_root, &order_book.bids),
    };

    let mut fills = [Fill::default(); MAX_FILLS];
    let mut count = 0usize;
    let mut taker_remaining = incoming_quantity;

    for (_handle, leaf) in OrderTreeIter::new(nodes, root) {
        if taker_remaining == 0 || count >= MAX_FILLS {
            break;
        }

        // Crossing condition: a bid takes when its limit is >= the resting
        // ask's price; an ask takes when its limit is <= the resting bid's
        // price. The walk is best-price-first on the resting side, so the
        // first leaf that fails to cross means every subsequent leaf also
        // fails - break, don't continue.
        let resting_price = leaf.price();
        let crosses = match incoming_side {
            OrderSide::Bid => incoming_price >= resting_price,
            OrderSide::Ask => incoming_price <= resting_price,
        };
        if !crosses {
            break;
        }

        let leaf_quantity = leaf.quantity;
        if leaf_quantity == 0 {
            // Defensive: fully-filled leaves are removed, but skip a stray
            // zero-quantity leaf rather than emit a zero-size fill.
            continue;
        }

        let fill_quantity = taker_remaining.min(leaf_quantity);
        fills[count] = Fill {
            maker_order_id: leaf.order_id,
            fill_quantity,
            fill_price: resting_price,
        };
        count += 1;
        taker_remaining = taker_remaining.saturating_sub(fill_quantity);
    }

    FillPlan {
        fills,
        count,
        taker_remaining,
    }
}
