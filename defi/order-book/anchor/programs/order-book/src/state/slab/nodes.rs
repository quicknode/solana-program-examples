// Adapted from openbook-dex/openbook-v2 (commit f3e17421e675b083b584867594bf3cf4f675d156),
// MIT-licensed. See LICENSE-OPENBOOK in this directory.
//
// Trimmed from the upstream nodes.rs:
//   - dropped oracle-pegged price helpers (this CLOB is fixed-price only)
//   - dropped time-in-force / expiry tracking (no IOC/post-only here)
//   - dropped client_order_id and owner_slot (the on-chain `Order` PDA
//     already owns that bookkeeping)
// The slab/tree mechanics (NodeTag, InnerNode/LeafNode/FreeNode, AnyNode,
// walk_down, new_node_key) are preserved as-is so future contributors can
// diff against upstream cleanly.

use std::mem::{align_of, size_of};

use anchor_lang::prelude::*;
use bytemuck::{cast_mut, cast_ref};
use static_assertions::const_assert_eq;

use crate::state::OrderSide;

/// Index into `OrderTreeNodes::nodes`. u32 is plenty for 1024 slots; the type
/// is kept wide to match upstream so the layout stays compatible.
pub type NodeHandle = u32;

/// Every node — Inner, Leaf, Free — is padded to the same 88 bytes so the
/// underlying `[AnyNode; N]` array is a true slab: we can swap a Leaf for an
/// Inner in place without reallocating. Matches the upstream Openbook layout.
pub const NODE_SIZE: usize = 88;

/// Tag stored in the first byte of every slab slot.
///
/// `Uninitialized` (the zero value) is what fresh account memory looks like
/// before any insert, so a freshly-zeroed slab is automatically empty.
#[repr(u8)]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum NodeTag {
    Uninitialized = 0,
    InnerNode = 1,
    LeafNode = 2,
    FreeNode = 3,
    LastFreeNode = 4,
}

impl NodeTag {
    pub fn from_u8(tag: u8) -> Option<Self> {
        match tag {
            0 => Some(NodeTag::Uninitialized),
            1 => Some(NodeTag::InnerNode),
            2 => Some(NodeTag::LeafNode),
            3 => Some(NodeTag::FreeNode),
            4 => Some(NodeTag::LastFreeNode),
            _ => None,
        }
    }
}

/// Build the 128-bit tree key for a new leaf.
///
/// Layout: `[price_data : u64][seq_num_bits : u64]`.
///
/// For asks the seq_num is stored as-is, so earlier (smaller) seq_num sorts
/// before a later one at the same price → time priority is preserved when
/// walking the tree ascending.
///
/// For bids we invert the seq_num so that *earlier* still sorts first when
/// the tree is walked descending. (Reading bids in descending order means
/// largest key first; inverting seq_num makes the earlier order's seq_num
/// the larger one at any given price.)
pub fn new_node_key(side: OrderSide, price_data: u64, seq_num: u64) -> u128 {
    let seq_num = if side == OrderSide::Bid { !seq_num } else { seq_num };
    ((price_data as u128) << 64) | (seq_num as u128)
}

/// Extract the price half of a tree key. Same shape on both sides because the
/// price is stored in the high 64 bits unmodified.
#[inline(always)]
pub fn price_from_key(key: u128) -> u64 {
    (key >> 64) as u64
}

/// One internal tree node. Two children (referenced by slab handle), a key
/// holding the shared prefix bits, and `prefix_len` telling consumers how many
/// of the high bits of `key` are meaningful.
///
/// `tag` is the first byte at offset 0 — same offset as on `LeafNode` and
/// `FreeNode` — so `AnyNode::tag` reads the variant tag from a fixed offset
/// regardless of which variant is in the slot.
///
/// `repr(C, packed(8))` caps field alignment at 8 bytes. Without this, u128
/// (which has 16-byte alignment on x86_64) would force the struct to align
/// to 16, mismatching `AnyNode`'s 8-byte alignment and tripping both the
/// `align_of` const-asserts and bytemuck's `Pod` derive. Capping at 8 is
/// safe: every field is still naturally 8-aligned within the struct, so
/// references and reads work normally.
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C, packed(8))]
pub struct InnerNode {
    /// `NodeTag::InnerNode`
    pub tag: u8,
    pub padding: [u8; 3],

    /// Number of high `key` bits that all descendants share.
    pub prefix_len: u32,

    /// Only the top `prefix_len` bits of `key` are meaningful — the rest is
    /// whichever leaf happened to be inserted first below this node.
    pub key: u128,

    /// Slab handles for the left (`children[0]`) and right (`children[1]`)
    /// subtree. Left = 0 critbit, right = 1 critbit.
    pub children: [NodeHandle; 2],

    /// Pads to NODE_SIZE so InnerNode / LeafNode / FreeNode are
    /// interchangeable in the slab. 88 - 1 - 3 - 4 - 16 - 8 = 56.
    pub reserved: [u8; 56],
}
const_assert_eq!(size_of::<InnerNode>(), NODE_SIZE);
const_assert_eq!(size_of::<InnerNode>() % 8, 0);

impl InnerNode {
    pub fn new(prefix_len: u32, key: u128) -> Self {
        Self {
            tag: NodeTag::InnerNode as u8,
            padding: [0; 3],
            prefix_len,
            key,
            children: [0; 2],
            reserved: [0; 56],
        }
    }

    /// Given a search key, return the child the key would descend into and
    /// the critbit (0 or 1) that decision was based on.
    ///
    /// The critbit is the bit immediately *after* the shared prefix
    /// (i.e. the first bit at which the search key can disagree with this
    /// node's stored prefix).
    #[inline(always)]
    pub fn walk_down(&self, search_key: u128) -> (NodeHandle, bool) {
        let crit_bit_mask = 1u128 << (127 - self.prefix_len);
        let crit_bit = (search_key & crit_bit_mask) != 0;
        (self.children[crit_bit as usize], crit_bit)
    }
}

/// One resting order in the slab.
///
/// All the per-order metadata callers care about lives on the corresponding
/// `Order` PDA — the slab leaf only stores what the matching engine needs:
/// the tree key (price + tie-break), the remaining quantity, the owner, and
/// the order_id (which the handler uses to verify the matching `Order`
/// account the caller passed in).
///
/// `tag` is at offset 0 to match `InnerNode` (see comment there).
/// `repr(C, packed(8))` for the same reason as `InnerNode`.
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C, packed(8))]
pub struct LeafNode {
    /// `NodeTag::LeafNode`
    pub tag: u8,
    pub padding: [u8; 7],

    /// The 128-bit tree key. See `new_node_key`.
    pub key: u128,

    /// Owner of this resting order. Same as the corresponding `Order`
    /// account's `owner`; cached here so the matching loop doesn't have to
    /// deserialize the maker account just to read the owner.
    pub owner: Pubkey,

    /// Quantity remaining (in base tokens). Decremented as fills consume the
    /// order; the leaf is removed when it hits 0.
    pub quantity: u64,

    /// Public order identifier. The handler uses this both to look up the
    /// owner's `Order` PDA and to sanity-check the maker account list passed
    /// as `remaining_accounts`.
    pub order_id: u64,

    /// Unix timestamp at which the order rested. Not used by matching (the
    /// seq_num inside `key` is the tie-break) — kept so off-chain tooling
    /// can show an "age" without re-deriving it from a different account.
    pub timestamp: i64,

    /// Pads to NODE_SIZE. 88 - 1 - 7 - 16 - 32 - 8 - 8 - 8 = 8.
    pub reserved: [u8; 8],
}
const_assert_eq!(size_of::<LeafNode>(), NODE_SIZE);
const_assert_eq!(size_of::<LeafNode>() % 8, 0);

impl LeafNode {
    pub fn new(
        key: u128,
        owner: Pubkey,
        quantity: u64,
        order_id: u64,
        timestamp: i64,
    ) -> Self {
        Self {
            tag: NodeTag::LeafNode as u8,
            padding: [0; 7],
            key,
            owner,
            quantity,
            order_id,
            timestamp,
            reserved: [0; 8],
        }
    }

    /// Price half of the tree key — convenience for callers.
    #[inline(always)]
    pub fn price(&self) -> u64 {
        price_from_key(self.key)
    }
}

/// Free-list link in the slab. When a leaf is removed its slot is replaced by
/// a FreeNode that points to the previous free slot, so subsequent inserts
/// reuse the slot in O(1).
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub(crate) struct FreeNode {
    pub(crate) tag: u8,
    pub(crate) padding: [u8; 3],
    /// Next free slot in the chain, or unused when this is the last.
    pub(crate) next: NodeHandle,
    pub(crate) reserved: [u8; NODE_SIZE - 16],
    /// Forces 8-byte alignment so all node variants share the same alignment.
    pub(crate) force_align: u64,
}
const_assert_eq!(size_of::<FreeNode>(), NODE_SIZE);
const_assert_eq!(size_of::<FreeNode>() % 8, 0);

/// Type-erased node. The slab is an array of these; the leading `tag` byte
/// tells callers which variant the slot actually holds.
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(C)]
pub struct AnyNode {
    pub tag: u8,
    pub data: [u8; 79],
    /// See FreeNode::force_align.
    pub force_align: u64,
}
const_assert_eq!(size_of::<AnyNode>(), NODE_SIZE);
const_assert_eq!(size_of::<AnyNode>() % 8, 0);
const_assert_eq!(align_of::<AnyNode>(), 8);
const_assert_eq!(size_of::<AnyNode>(), size_of::<InnerNode>());
const_assert_eq!(align_of::<AnyNode>(), align_of::<InnerNode>());
const_assert_eq!(size_of::<AnyNode>(), size_of::<LeafNode>());
const_assert_eq!(align_of::<AnyNode>(), align_of::<LeafNode>());
const_assert_eq!(size_of::<AnyNode>(), size_of::<FreeNode>());
const_assert_eq!(align_of::<AnyNode>(), align_of::<FreeNode>());

pub enum NodeRef<'a> {
    Inner(&'a InnerNode),
    Leaf(&'a LeafNode),
}

pub enum NodeRefMut<'a> {
    Inner(&'a mut InnerNode),
    Leaf(&'a mut LeafNode),
}

impl AnyNode {
    pub fn case(&self) -> Option<NodeRef> {
        match NodeTag::from_u8(self.tag)? {
            NodeTag::InnerNode => Some(NodeRef::Inner(cast_ref(self))),
            NodeTag::LeafNode => Some(NodeRef::Leaf(cast_ref(self))),
            _ => None,
        }
    }

    pub fn case_mut(&mut self) -> Option<NodeRefMut> {
        match NodeTag::from_u8(self.tag)? {
            NodeTag::InnerNode => Some(NodeRefMut::Inner(cast_mut(self))),
            NodeTag::LeafNode => Some(NodeRefMut::Leaf(cast_mut(self))),
            _ => None,
        }
    }

    pub fn key(&self) -> Option<u128> {
        Some(match self.case()? {
            NodeRef::Inner(inner) => inner.key,
            NodeRef::Leaf(leaf) => leaf.key,
        })
    }

    pub fn children(&self) -> Option<[NodeHandle; 2]> {
        match self.case()? {
            NodeRef::Inner(inner) => Some(inner.children),
            NodeRef::Leaf(_) => None,
        }
    }

    pub fn as_leaf(&self) -> Option<&LeafNode> {
        match self.case() {
            Some(NodeRef::Leaf(leaf)) => Some(leaf),
            _ => None,
        }
    }

    pub fn as_leaf_mut(&mut self) -> Option<&mut LeafNode> {
        match self.case_mut() {
            Some(NodeRefMut::Leaf(leaf)) => Some(leaf),
            _ => None,
        }
    }

    pub fn as_inner_mut(&mut self) -> Option<&mut InnerNode> {
        match self.case_mut() {
            Some(NodeRefMut::Inner(inner)) => Some(inner),
            _ => None,
        }
    }
}

impl AsRef<AnyNode> for InnerNode {
    fn as_ref(&self) -> &AnyNode {
        cast_ref(self)
    }
}

impl AsRef<AnyNode> for LeafNode {
    fn as_ref(&self) -> &AnyNode {
        cast_ref(self)
    }
}

impl FreeNode {
    pub(crate) fn new(tag: NodeTag, next: NodeHandle) -> Self {
        Self {
            tag: tag as u8,
            padding: [0; 3],
            next,
            reserved: [0; NODE_SIZE - 16],
            force_align: 0,
        }
    }
}
