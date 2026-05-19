// Slab + order-tree port of openbook-dex/openbook-v2 (commit
// f3e17421e675b083b584867594bf3cf4f675d156), MIT-licensed. See
// LICENSE-OPENBOOK in this directory.
//
// Public surface is intentionally small: the OrderBook wrapper in
// `state/order_book.rs` calls insert/best/remove/iter; nothing else in the
// program needs to know the tree exists.

pub mod iterator;
pub mod nodes;
pub mod ordertree;

pub use iterator::OrderTreeIter;
pub use nodes::{
    new_node_key, price_from_key, AnyNode, InnerNode, LeafNode, NodeHandle, NodeRef, NodeTag,
    NODE_SIZE,
};
pub use ordertree::{OrderTreeNodes, OrderTreeRoot, OrderTreeType, MAX_TREE_NODES};
