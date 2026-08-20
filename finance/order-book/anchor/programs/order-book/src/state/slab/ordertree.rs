// Adapted from openbook-dex/openbook-v2 (commit f3e17421e675b083b584867594bf3cf4f675d156),
// MIT-licensed. See LICENSE-OPENBOOK in this directory.
//
// This port covers fixed-price orders with no expiry tracking (no
// time-in-force orders). Error type is rebased to this crate's `ErrorCode`.
// The tree mechanics (critbit / radix-trie insert + remove, min/max walks)
// are preserved so future contributors can diff against upstream cleanly.

use anchor_lang::prelude::*;
use bytemuck::{cast, cast_ref};
use static_assertions::const_assert_eq;

use super::nodes::{AnyNode, FreeNode, InnerNode, LeafNode, NodeHandle, NodeRef, NodeTag};
use crate::errors::ErrorCode;

/// Per-side slab capacity. 1024 leaves easily covers any realistic depth at
/// the prices a single market quotes; the 88-byte node size keeps each side
/// at ~90 KB, well under Solana's 10 MB per-account ceiling.
pub const MAX_TREE_NODES: usize = 1024;

/// Root pointer + leaf count for one side of the book.
///
/// `maybe_node` is only meaningful when `leaf_count > 0` - a freshly-zeroed
/// root represents an empty tree.
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct OrderTreeRoot {
    pub maybe_node: NodeHandle,
    pub leaf_count: u32,
}
const_assert_eq!(std::mem::size_of::<OrderTreeRoot>(), 8);

// These types are internal slab implementation details that don't need a
// rich IDL representation. The default IdlBuild impls (create_type → None,
// insert_types → no-op) satisfy anchor build's IDL generation without
// exposing the raw slab internals to clients.
#[cfg(feature = "idl-build")]
impl anchor_lang::IdlBuild for OrderTreeRoot {}

#[cfg(feature = "idl-build")]
impl anchor_lang::IdlBuild for OrderTreeNodes {}

impl OrderTreeRoot {
    pub fn node(&self) -> Option<NodeHandle> {
        if self.leaf_count == 0 {
            None
        } else {
            Some(self.maybe_node)
        }
    }
}

/// Which side of the book this tree represents.
///
/// Iteration order is determined by this: bids walk highest-key-first (best
/// bid first), asks walk lowest-key-first (best ask first). Since price is
/// stored in the high bits of the key, that's also best-price-first on both
/// sides.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OrderTreeType {
    Bids = 0,
    Asks = 1,
}

/// The slab itself. A fixed-size array of nodes plus the bookkeeping needed
/// to allocate and free slots in O(1):
///   - `bump_index` is the next never-used slot (used when the free list is
///     empty)
///   - `free_list_head` + `free_list_len` chain reclaimed slots, so cancels
///     reuse memory without compaction.
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct OrderTreeNodes {
    /// `OrderTreeType`, as a u8 so the struct stays Pod.
    pub order_tree_type: u8,
    pub padding: [u8; 3],
    pub bump_index: u32,
    pub free_list_len: u32,
    pub free_list_head: NodeHandle,
    pub nodes: [AnyNode; MAX_TREE_NODES],
}
const_assert_eq!(
    std::mem::size_of::<OrderTreeNodes>(),
    1 + 3 + 4 + 4 + 4 + 88 * MAX_TREE_NODES
);

impl OrderTreeNodes {
    pub fn order_tree_type(&self) -> OrderTreeType {
        match self.order_tree_type {
            0 => OrderTreeType::Bids,
            1 => OrderTreeType::Asks,
            _ => panic!("invalid order_tree_type"),
        }
    }

    pub fn node(&self, handle: NodeHandle) -> Option<&AnyNode> {
        let node = &self.nodes[handle as usize];
        match NodeTag::from_u8(node.tag) {
            Some(NodeTag::InnerNode) | Some(NodeTag::LeafNode) => Some(node),
            _ => None,
        }
    }

    pub fn node_mut(&mut self, handle: NodeHandle) -> Option<&mut AnyNode> {
        let node = &mut self.nodes[handle as usize];
        match NodeTag::from_u8(node.tag) {
            Some(NodeTag::InnerNode) | Some(NodeTag::LeafNode) => Some(node),
            _ => None,
        }
    }

    /// Best-priced leaf for this tree.
    ///
    /// For asks ("min" means lowest price) this is the leftmost leaf; for
    /// bids ("max" means highest price) this is the rightmost leaf.
    pub fn best_leaf(&self, root: &OrderTreeRoot) -> Option<(NodeHandle, &LeafNode)> {
        let find_max = self.order_tree_type() == OrderTreeType::Bids;
        self.leaf_min_max(find_max, root)
    }

    fn leaf_min_max(
        &self,
        find_max: bool,
        root: &OrderTreeRoot,
    ) -> Option<(NodeHandle, &LeafNode)> {
        let mut node_handle: NodeHandle = root.node()?;
        let critbit = usize::from(find_max);
        loop {
            match self.node(node_handle)?.case()? {
                NodeRef::Inner(inner) => node_handle = inner.children[critbit],
                NodeRef::Leaf(leaf) => return Some((node_handle, leaf)),
            }
        }
    }

    /// Look up a leaf by its full 128-bit key.
    pub fn find_by_key(
        &self,
        root: &OrderTreeRoot,
        search_key: u128,
    ) -> Option<(NodeHandle, &LeafNode)> {
        let mut handle = root.node()?;
        loop {
            let node = self.node(handle)?;
            match node.case()? {
                NodeRef::Inner(inner) => {
                    let (next, _) = inner.walk_down(search_key);
                    handle = next;
                }
                NodeRef::Leaf(leaf) => {
                    if leaf.key == search_key {
                        return Some((handle, leaf));
                    }
                    return None;
                }
            }
        }
    }

    pub fn remove_by_key(
        &mut self,
        root: &mut OrderTreeRoot,
        search_key: u128,
    ) -> Option<LeafNode> {
        // Stack of (handle, critbit) pairs so we can walk back up to the
        // root after splicing the leaf out - same trick the upstream uses.
        let mut stack: Vec<(NodeHandle, bool)> = vec![];

        let mut parent_h = root.node()?;
        let (mut child_h, mut crit_bit) = match self.node(parent_h)?.case()? {
            NodeRef::Leaf(&leaf) if leaf.key == search_key => {
                // Special case: the root is the matching leaf. Clear root,
                // free the slot.
                assert_eq!(root.leaf_count, 1);
                root.maybe_node = 0;
                root.leaf_count = 0;
                let _ = self.free(parent_h)?;
                return Some(leaf);
            }
            NodeRef::Leaf(_) => return None,
            NodeRef::Inner(inner) => inner.walk_down(search_key),
        };
        stack.push((parent_h, crit_bit));

        loop {
            match self.node(child_h)?.case()? {
                NodeRef::Inner(inner) => {
                    parent_h = child_h;
                    let (next, bit) = inner.walk_down(search_key);
                    child_h = next;
                    crit_bit = bit;
                    stack.push((parent_h, crit_bit));
                }
                NodeRef::Leaf(leaf) => {
                    if leaf.key != search_key {
                        return None;
                    }
                    break;
                }
            }
        }

        // Replace the parent inner-node with its remaining child. We free
        // both the old parent slot's content and the child slot.
        let other_child_h = self.node(parent_h)?.children()?[!crit_bit as usize];
        let other_child = self.free(other_child_h)?;
        *self.nodes.get_mut(parent_h as usize)? = other_child;

        root.leaf_count -= 1;
        let removed: LeafNode = cast(self.free(child_h)?);
        Some(removed)
    }

    /// Move the slot at `handle` onto the free list, return whatever was
    /// there.
    fn free(&mut self, handle: NodeHandle) -> Option<AnyNode> {
        let val = *self.node(handle)?;
        let tag = if self.free_list_len == 0 {
            NodeTag::LastFreeNode
        } else {
            NodeTag::FreeNode
        };
        self.nodes[handle as usize] = cast(FreeNode::new(tag, self.free_list_head));
        self.free_list_len += 1;
        self.free_list_head = handle;
        Some(val)
    }

    /// Allocate a slot and write `val` into it. Reuses a free-list slot when
    /// one's available, otherwise consumes the next bump slot.
    fn alloc(&mut self, val: &AnyNode) -> Result<NodeHandle> {
        match NodeTag::from_u8(val.tag) {
            Some(NodeTag::InnerNode) | Some(NodeTag::LeafNode) => (),
            _ => return err!(ErrorCode::OrderBookFull),
        };

        if self.free_list_len == 0 {
            require!(
                (self.bump_index as usize) < self.nodes.len() && self.bump_index < u32::MAX,
                ErrorCode::OrderBookFull
            );
            self.nodes[self.bump_index as usize] = *val;
            let handle = self.bump_index;
            self.bump_index += 1;
            return Ok(handle);
        }

        let handle = self.free_list_head;
        let next = cast_ref::<AnyNode, FreeNode>(&self.nodes[handle as usize]).next;
        self.free_list_head = next;
        self.free_list_len -= 1;
        self.nodes[handle as usize] = *val;
        Ok(handle)
    }

    /// Insert `new_leaf` into the tree rooted at `root`.
    ///
    /// Returns the handle of the new leaf and, when a duplicate key collided,
    /// the leaf that got overwritten. (Callers in this order book embed a
    /// monotonically increasing seq_num in every key, so collisions cannot
    /// actually happen - the case is kept just to match the upstream API.)
    pub fn insert_leaf(
        &mut self,
        root: &mut OrderTreeRoot,
        new_leaf: &LeafNode,
    ) -> Result<(NodeHandle, Option<LeafNode>)> {
        // Empty tree: leaf becomes the root.
        let mut parent_handle: NodeHandle = match root.node() {
            Some(h) => h,
            None => {
                let handle = self.alloc(new_leaf.as_ref())?;
                root.maybe_node = handle;
                root.leaf_count = 1;
                return Ok((handle, None));
            }
        };

        let mut stack: Vec<(NodeHandle, bool)> = vec![];

        loop {
            let parent_contents = *self.node(parent_handle).ok_or(ErrorCode::OrderBookFull)?;
            let parent_key = parent_contents.key().unwrap();

            // Exact-key collision: only possible if the existing slot is a
            // leaf (inner nodes' `key` is just a shared prefix). Overwrite
            // and bail.
            if parent_key == new_leaf.key {
                if let Some(NodeRef::Leaf(&old_leaf)) = parent_contents.case() {
                    *self.node_mut(parent_handle).unwrap() = *new_leaf.as_ref();
                    return Ok((parent_handle, Some(old_leaf)));
                }
            }

            let shared_prefix_len: u32 = (parent_key ^ new_leaf.key).leading_zeros();

            if let Some(NodeRef::Inner(inner)) = parent_contents.case() {
                if shared_prefix_len >= inner.prefix_len {
                    // The new key shares at least this node's prefix -
                    // descend.
                    let (child, crit_bit) = inner.walk_down(new_leaf.key);
                    stack.push((parent_handle, crit_bit));
                    parent_handle = child;
                    continue;
                }
            }

            // Split: parent (leaf or inner with shorter shared prefix) and
            // new leaf disagree at bit `shared_prefix_len`. Replace parent
            // in place with a new InnerNode that has them both as children.
            let crit_bit_mask: u128 = 1u128 << (127 - shared_prefix_len);
            let new_leaf_crit_bit = (crit_bit_mask & new_leaf.key) != 0;
            let old_parent_crit_bit = !new_leaf_crit_bit;

            let new_leaf_handle = self.alloc(new_leaf.as_ref())?;
            let moved_parent_handle = match self.alloc(&parent_contents) {
                Ok(h) => h,
                Err(error) => {
                    // alloc rolled back from the failed half-insert.
                    let _ = self.free(new_leaf_handle);
                    return Err(error);
                }
            };

            // The slot at `parent_handle` currently holds a LeafNode (or an
            // InnerNode whose prefix is too long for the new key). We're
            // replacing it with a freshly-built InnerNode that has the new
            // leaf and the moved-aside old node as children. We can't go via
            // `node_mut().as_inner_mut()` here because that would refuse the
            // slot when its tag is still LeafNode - instead, write a complete
            // new InnerNode bit-pattern into the slot via AnyNode.
            let mut new_inner = InnerNode::new(shared_prefix_len, new_leaf.key);
            new_inner.children[new_leaf_crit_bit as usize] = new_leaf_handle;
            new_inner.children[old_parent_crit_bit as usize] = moved_parent_handle;
            self.nodes[parent_handle as usize] = *new_inner.as_ref();

            // `stack` not needed past this point; suppress unused-mut.
            let _ = stack;

            root.leaf_count += 1;
            return Ok((new_leaf_handle, None));
        }
    }

    pub fn is_full(&self) -> bool {
        self.free_list_len <= 1 && (self.bump_index as usize) >= self.nodes.len() - 1
    }
}
