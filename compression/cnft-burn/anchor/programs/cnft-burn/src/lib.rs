// `diverging_sub_expression` is a false positive emitted from the Anchor
// `#[program]` macro expansion under this clippy/rustc version; the generated
// instruction-dispatch code is correct.
#![allow(clippy::diverging_sub_expression)]

use anchor_lang::prelude::*;

pub mod instructions;
use instructions::*;

declare_id!("C6qxH8n6mZxrrbtMtYWYSp8JR8vkQ55X1o4EBg7twnMv");

/// mpl-bubblegum program ID
pub const MPL_BUBBLEGUM_ID: Pubkey = pubkey!("BGUMAp9Gq7iTEuizy4pqaxsTyUCBK68MDfK752saRPUY");

/// SPL Account Compression program ID
pub const SPL_ACCOUNT_COMPRESSION_ID: Pubkey =
    pubkey!("cmtDvXumGCrqC1Age74AVPhSRVXJMd8PJS91L8KbNCK");

#[derive(Clone)]
pub struct SPLCompression;

impl anchor_lang::Id for SPLCompression {
    fn id() -> Pubkey {
        SPL_ACCOUNT_COMPRESSION_ID
    }
}

#[program]
pub mod cnft_burn {
    use super::*;

    pub fn burn_cnft<'info>(
        context: Context<'info, BurnCnftAccountConstraints<'info>>,
        root: [u8; 32],
        data_hash: [u8; 32],
        creator_hash: [u8; 32],
        nonce: u64,
        index: u32,
    ) -> Result<()> {
        instructions::burn_cnft::handle_burn_cnft(context, root, data_hash, creator_hash, nonce, index)
    }
}
