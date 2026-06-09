// Adapted from openbook-dex/openbook-v2 (commit f3e17421e675b083b584867594bf3cf4f675d156),
// MIT-licensed. See LICENSE-OPENBOOK in this directory.
//
// This iterator yields (handle, leaf) pairs in best-price-first order.
// Matching uses pure price-time priority — no oracle peg or time-in-force
// filtering needed here.

use super::nodes::{InnerNode, LeafNode, NodeHandle, NodeRef};
use super::ordertree::{OrderTreeNodes, OrderTreeRoot, OrderTreeType};

/// In-order walk of one side of the book.
///
/// Visits leaves in best-price-first order:
///   - asks: ascending (lowest price first)
///   - bids: descending (highest price first)
///
/// Within a single price level, earlier orders come first (price-time
/// priority) — that ordering is encoded in the leaf key, so it falls out of
/// the in-order walk automatically.
pub struct OrderTreeIter<'a> {
    nodes: &'a OrderTreeNodes,

    /// Inner nodes whose "right" branch (in walk direction) we still owe a
    /// visit to.
    stack: Vec<&'a InnerNode>,

    /// Cached next leaf so `peek` and `next` can share work.
    next_leaf: Option<(NodeHandle, &'a LeafNode)>,

    /// Child indexes to walk: (first, second). For asks we go (0, 1) — i.e.
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
            stack: vec![],
            next_leaf: None,
            first,
            second,
        };
        if let Some(handle) = root.node() {
            iter.next_leaf = iter.walk_to_first_leaf(handle);
        }
        iter
    }

    pub fn peek(&self) -> Option<(NodeHandle, &'a LeafNode)> {
        self.next_leaf
    }

    fn walk_to_first_leaf(&mut self, start: NodeHandle) -> Option<(NodeHandle, &'a LeafNode)> {
        let mut current = start;
        loop {
            match self.nodes.node(current)?.case()? {
                NodeRef::Inner(inner) => {
                    self.stack.push(inner);
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
        self.next_leaf = match self.stack.pop() {
            None => None,
            Some(inner) => {
                let start = inner.children[self.second];
                self.walk_to_first_leaf(start)
            }
        };
        Some(current)
    }
}
