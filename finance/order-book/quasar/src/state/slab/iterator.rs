// Adapted from openbook-dex/openbook-v2 (commit f3e17421e675b083b584867594bf3cf4f675d156),
// MIT-licensed. See LICENSE-OPENBOOK in this directory.
//
// This iterator yields (handle, leaf) pairs in best-price-first order.
// Matching uses pure price-time priority - no oracle peg or time-in-force
// filtering needed here.
//
// The traversal stack is a fixed-size array of node handles rather than a heap
// `Vec` of node references: this program is `no_std` with no allocator. A
// critbit tree over 128-bit keys is at most 128 inner nodes deep (each step
// down strictly increases the shared-prefix length, which is < 128), so the
// stack can never overflow.

use super::nodes::{NodeHandle, NodeRef};
use super::ordertree::{OrderTreeNodes, OrderTreeRoot, OrderTreeType};
use super::LeafNode;

/// Upper bound on critbit-tree depth: one inner node per meaningful key bit.
const MAX_ITER_DEPTH: usize = 128;

/// In-order walk of one side of the book.
///
/// Visits leaves in best-price-first order:
///   - asks: ascending (lowest price first)
///   - bids: descending (highest price first)
///
/// Within a single price level, earlier orders come first (price-time
/// priority) - that ordering is encoded in the leaf key, so it falls out of
/// the in-order walk automatically.
pub struct OrderTreeIter<'a> {
    nodes: &'a OrderTreeNodes,

    /// Handles of inner nodes whose "second" branch (in walk direction) we
    /// still owe a visit to.
    stack: [NodeHandle; MAX_ITER_DEPTH],
    stack_len: usize,

    /// Cached next leaf so `next` can return it while pre-computing the one
    /// after.
    next_leaf: Option<(NodeHandle, &'a LeafNode)>,

    /// Child indexes to walk: (first, second). For asks we go (0, 1) - i.e.
    /// down the left child first, then right; for bids we go (1, 0).
    first: usize,
    second: usize,
}

impl<'a> OrderTreeIter<'a> {
    pub fn new(nodes: &'a OrderTreeNodes, root: &OrderTreeRoot) -> Self {
        let (first, second) = if nodes.order_tree_type() == OrderTreeType::Bids {
            (1, 0)
        } else {
            (0, 1)
        };
        let mut iter = Self {
            nodes,
            stack: [0; MAX_ITER_DEPTH],
            stack_len: 0,
            next_leaf: None,
            first,
            second,
        };
        if let Some(handle) = root.node() {
            iter.next_leaf = iter.walk_to_first_leaf(handle);
        }
        iter
    }

    fn walk_to_first_leaf(&mut self, start: NodeHandle) -> Option<(NodeHandle, &'a LeafNode)> {
        // Copy the `&'a` node-arena reference out so leaf references we return
        // carry lifetime `'a`, independent of the `&mut self` used to push
        // onto the stack.
        let nodes = self.nodes;
        let mut current = start;
        loop {
            match nodes.node(current)?.case()? {
                NodeRef::Inner(inner) => {
                    if self.stack_len < MAX_ITER_DEPTH {
                        self.stack[self.stack_len] = current;
                        self.stack_len += 1;
                    }
                    current = inner.children[self.first];
                }
                NodeRef::Leaf(leaf) => return Some((current, leaf)),
            }
        }
    }
}

impl<'a> Iterator for OrderTreeIter<'a> {
    type Item = (NodeHandle, &'a LeafNode);

    fn next(&mut self) -> Option<Self::Item> {
        let current = self.next_leaf?;
        self.next_leaf = if self.stack_len == 0 {
            None
        } else {
            self.stack_len -= 1;
            let inner_handle = self.stack[self.stack_len];
            match self.nodes.node(inner_handle).and_then(|node| node.case()) {
                Some(NodeRef::Inner(inner)) => {
                    let start = inner.children[self.second];
                    self.walk_to_first_leaf(start)
                }
                _ => None,
            }
        };
        Some(current)
    }
}
